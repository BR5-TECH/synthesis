//! The dispatch: what a turn is planned from, and the thread it runs on.
//!
//! [`dispatch_impl`] validates the request, registers the turn, and returns —
//! the call itself happens on a thread of its own, in `turn_body`.

use super::*;

/// Everything a running turn needs, gathered before the thread starts so the
/// thread borrows nothing.
pub(super) struct TurnPlan {
    pub(super) turn_id: String,
    pub(super) roots: OwnedRoots,
    pub(super) origin: ConversationOrigin,
    /// AGC-FR-16: the agent participant this turn contributes as, for whichever
    /// of its two contributions it makes — an answer, or a question.
    pub(super) author: Participant,
    /// CVL-FR-15: whether `ask_user_comment` posted a question during this turn.
    ///
    /// Written by the tool and read by the loop, which otherwise sees only a
    /// rendered result and could not tell a question that went out from one the
    /// conversation refused — and the whole of CVL-FR-15 turns on that
    /// difference: the first ends the turn, the second ends nothing.
    pub(super) asked: Arc<AtomicBool>,
    /// CVL-FR-15: whether `ask_discussion_questions` recorded a question set
    /// during this turn.
    ///
    /// A flag of its own rather than a second writer of `asked`, because the two
    /// terminal states differ in what they leave behind: a posted question is a
    /// comment of the conversation, and a recorded set appends nothing at all
    /// (ADQ-FR-DHZK). One flag for both would make the loop's own record say a
    /// question was posted when none was.
    pub(super) asked_questions: Arc<AtomicBool>,
    /// CVL-FR-15: whether `propose_draft_changes` recorded a proposal during this
    /// turn. The twin of `asked`, read on exactly the same terms.
    pub(super) proposed: Arc<AtomicBool>,
    pub(super) agent: Agent,
    /// AGC-FR-14: the AI API provider the open project resolved to when the turn
    /// was dispatched. An agent stores none, so this is what the turn's endpoint
    /// is resolved against, and a later change of the active provider reaches
    /// the next turn rather than this one.
    pub(super) provider: String,
    pub(super) request: AgentRequest,
    /// CVL-FR-08: the same six the request carries definitions of, in the form
    /// the loop dispatches by name.
    pub(super) tools: Vec<rig::tool::PortableDynamicTool>,
    /// CVL-FR-13 / CVL-FR-28: how many **logical** `complete` invocations the
    /// loop has made — the rounds of the loop, one per reply the model is asked
    /// for. Kept here rather than returned because both the success and the
    /// failure paths out of the loop need to be counted.
    pub(super) model_calls: AtomicUsize,
    /// CVL-FR-13 / CVL-FR-28: how many **physical** attempts actually reached a
    /// provider, retries included.
    ///
    /// Reported beside `model_calls` rather than instead of it, because a turn
    /// that made four calls and one that made four calls across nine attempts
    /// are the same turn to the loop and very different ones to the endpoint.
    pub(super) physical_calls: AtomicUsize,
    /// CVL-FR-28: the turn's input tokens, split by whether the provider served
    /// them from its cache (CVL-FR-23), accumulated over every call that
    /// reported the split.
    ///
    /// `tokens_reported` is what tells a turn whose provider said nothing from
    /// one that genuinely read nothing: without it, a provider that reports no
    /// usage at all would be recorded as a confident pair of zeroes, which reads
    /// as "cached nothing" when what happened is "would not say".
    pub(super) cached_input_tokens: AtomicU64,
    pub(super) uncached_input_tokens: AtomicU64,
    pub(super) tokens_reported: AtomicBool,
    pub(super) cancelled: Arc<AtomicBool>,
    /// AGC-FR-24: the label of the turn's progress operation, supplied here and
    /// displayed as it is.
    pub(super) progress_label: String,
}

/// AGC-FR-24: `@<nickname> is thinking about <draft name>…` for a draft,
/// `@<nickname> is thinking about <artifact path>…` for an artifact, and
/// `@<nickname> is thinking…` for a note.
///
/// A draft whose record cannot be read is named by its id, so the label always
/// names a subject for a draft. The artifact path is the artifact id, which is
/// the project-relative path (AGC-FR-07).
pub(super) fn progress_label_of(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    nickname: &str,
) -> String {
    let subject = match &origin.target {
        DiscussionTarget::Draft { draft_id } => Some(
            crate::drafts::read_draft_record(roots.worktree, draft_id)
                .map(|record| record.name)
                .unwrap_or_else(|_| draft_id.clone()),
        ),
        DiscussionTarget::Artifact { artifact_id } => Some(artifact_id.clone()),
        DiscussionTarget::Note { .. } => None,
    };
    match subject {
        Some(subject) => format!("@{nickname} is thinking about {subject}…"),
        None => format!("@{nickname} is thinking…"),
    }
}

/// AGC-FR-02: register the turn and return it `running` without waiting for the
/// model, so no surface is blocked by a conversation.
///
/// The refusals returned *before* registering are exactly the ones knowable
/// without a call — an unresolvable nickname, an unavailable agent, a locked
/// conversation. Every other outcome arrives as a state change on the registered
/// turn (AGC-FR-21).
///
/// Collaborators are read off the app handle rather than passed in, because the
/// spawned thread outlives this call and a `State<'_, T>` borrow does not. The
/// handle is the one thing that can cross that boundary.
pub fn dispatch_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nickname: &str,
    origin: ConversationOrigin,
    trigger_comment_id: &str,
) -> Result<AgentTurn, String> {
    use tauri::Manager;
    let store = app.state::<GlobalSettingsStore>();
    let turns = app.state::<TurnRegistry>();

    // AGC-FR-05: the owner and the fragment are read from the discussion the id
    // names, so the origin every later step works from is the stored one.
    let origin = normalise_origin(roots, origin);

    // AGC-FR-04: a turn names exactly one agent.
    let agent = agents::resolve_project_agent(&store, project_key, nickname).map_err(|e| {
        let failure = match e.as_str() {
            agents::ERR_AGENT_UNAVAILABLE => FAIL_AGENT_UNAVAILABLE,
            _ => FAIL_AGENT_NOT_FOUND,
        };
        // A refusal before registration produces no turn, so it emits no state
        // change either (AGC-FR-02) — this record is the only account of an
        // author whose `@mention` went nowhere.
        log_refusal(app, turns.buffer(), nickname, &origin, failure);
        failure.to_string()
    })?;

    // AGC-FR-14: the provider is the one the open project resolves to, read from
    // the registry alone. Availability has just found one that serves the
    // agent's model, so a refusal here means the registry changed between the
    // two reads; it is refused like any other unavailable agent.
    let provider = ai_api::resolve_agent_provider(&store, project_key).map_err(|_| {
        log_refusal(app, turns.buffer(), nickname, &origin, FAIL_AGENT_UNAVAILABLE);
        FAIL_AGENT_UNAVAILABLE.to_string()
    })?;

    // AGC-FR-30: a turn for a conversation whose **note has been deleted** is
    // refused *before it is registered*, so no turn record is created for a
    // deleted note by any route or any race. Knowable without a model call, like
    // the lock below, and refused here rather than left to fail as
    // `context_unavailable` on a turn that was published first — a registration
    // for a conversation that no longer exists is a turn a roster would show and
    // a surface would render a pending contribution for.
    if origin.note_id().is_some() {
        let resolves = resolve_origin(roots, &origin).is_some();
        if !resolves {
            log_refusal(app, turns.buffer(), nickname, &origin, FAIL_CONTEXT_UNAVAILABLE);
            return Err(FAIL_CONTEXT_UNAVAILABLE.to_string());
        }
    }

    // AGC-FR-19: a lock is what stops a conversation taking further
    // contributions, and an agent is subject to it exactly as an author is. The
    // lock in place *at dispatch* is knowable without a call, so it refuses here
    // rather than failing a registered turn.
    if thread_is_locked(roots, &origin) {
        log_refusal(app, turns.buffer(), nickname, &origin, FAIL_THREAD_LOCKED);
        return Err(FAIL_THREAD_LOCKED.to_string());
    }

    // AGC-FR-29: a reply is answered by a fresh turn and never by resuming the
    // one that asked. If this agent was awaiting a reply in this conversation,
    // that is what this dispatch is — so the wait is over: the awaiting turn
    // retires to `delivered`, having delivered its question, and stops being
    // outstanding. Nothing of it carries over into the turn below, which
    // assembles its input from the conversation as it now stands (AGC-FR-13) and
    // so reads the question and the answer as its last two comments.
    let retired = turns.retire_awaiting(&agent.id, &origin);
    if retired > 0 {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "the author answered a question an agent was waiting on",
            log_fields! {
                "agent" => &agent.nickname,
                "agentId" => &agent.id,
                "originKind" => origin.kind().as_str(),
                "discussionId" => origin.discussion_id(),
                "retired" => retired,
            },
        );
    }

    let (turn, cancelled) = turns.register(&agent, origin.clone(), trigger_comment_id, project_key);
    publish(app, &turn);
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "agent turn dispatched",
        log_fields! {
            "turnId" => &turn.id,
            "agent" => &agent.nickname,
            "agentId" => &agent.id,
            "provider" => &provider,
            "model" => &agent.model_id,
            "reasoning" => reasoning_label(agent.reasoning.as_ref()),
            "originKind" => origin.kind().as_str(),
            "discussionId" => origin.discussion_id(),
        },
    );

    // AGC-FR-24: the progress operation is registered when the turn holds a
    // call slot (see `run_turn_body`), so a turn that waits for a slot is not
    // reported as running. The label is decided here, where the roots are at
    // hand, and names the agent and the subject of the conversation.
    let progress_label = progress_label_of(roots, &origin, &agent.nickname);

    // AGC-FR-13: input is assembled at dispatch, so a turn answers the
    // conversation as it stood when the agent was addressed.
    let input = build_input(roots, &origin, trigger_comment_id);
    // The shape of the material and nothing of the material itself: which
    // sections a builder could produce, how much they came to, and whether the
    // bound of AGC-FR-11 bit — which is what explains a reply about half a
    // document, and what a `context_unavailable` failure is read against. The
    // bodies are an artifact's text and a participant's words (CVL-FR-07) and
    // stay out of the record.
    logging::log_debug(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "agent turn context assembled",
        log_fields! {
            "turnId" => &turn.id,
            "sections" => input.len(),
            "tags" => input.iter().map(|s| s.tag.as_str()).collect::<Vec<_>>().join(","),
            "chars" => input.iter().map(|s| s.body.chars().count()).sum::<usize>(),
            "truncated" => input.iter().any(|s| s.truncated),
        },
    );
    // Read before the origin is moved into the plan below.
    let origin_kind = origin.kind();
    // AGC-FR-16: the participant an agent contributes as, whether it is
    // answering or asking. Composed here so the two paths cannot disagree about
    // who a comment came from.
    // CMS-FR-65: the title snapshot is taken here, from the registry as it
    // stands at dispatch, and never resolved again. Composing the participant at
    // this one site is what makes AGC-FR-16, AUC-FR-04, and PDC-FR-13 agree by
    // construction: the answer, the question, and the proposal's comment are all
    // stamped from this value, so no append path can carry a different title
    // from another.
    //
    // `Some(_)` even when the agent carries no title: the empty string is what
    // this build recorded, and it is distinct from the `None` an older log
    // carries because nothing recorded anything (AGC-FR-16, CVL-FR-04, CMS-FR-65).
    let author = Participant::Agent {
        agent_id: agent.id.clone(),
        handle: agent.nickname.clone(),
        model: Some(agent.model_id.clone()),
        title: Some(agent.title.clone()),
    };
    // CVL-FR-15: set by the two turn-ending tools when a contribution actually
    // goes out — a question here, a proposal there.
    let asked = Arc::new(AtomicBool::new(false));
    let asked_questions = Arc::new(AtomicBool::new(false));
    let proposed = Arc::new(AtomicBool::new(false));
    // CVL-FR-08: named here, and decided by the origin kind. The definitions the
    // request carries are taken from the very tools the loop will dispatch, so
    // what the model is offered and what it can actually reach cannot drift
    // apart.
    let tools = conversation_tools(
        app,
        &turn.id,
        roots,
        &origin,
        &author,
        &asked,
        &asked_questions,
        &proposed,
    );
    let plan = TurnPlan {
        turn_id: turn.id.clone(),
        roots: roots.owned(),
        origin,
        author,
        asked,
        asked_questions,
        proposed,
        request: AgentRequest {
            // CVL-FR-04 / CVL-FR-03: the prompt the origin kind selects, with the
            // persona composed into it. What differs between two turns of one kind
            // is the persona and the material, never the shape of the request.
            instructions: compile_prompt(origin_kind, &agent.instructions, &agent.title),
            // CVL-FR-39: the material under discussion leads, and the discussion
            // itself follows it (CVL-FR-06), so the head is the run of sections
            // before the first one the conversation grows.
            stable_head_sections: stable_head_of(&input),
            input,
            tools: tools.iter().map(|tool| tool.definition()).collect(),
            // CVL-FR-30: decided by the provider carrying the call and by nothing
            // else. Read from the dispatch's resolution rather than from the
            // resolved endpoint because the two are the same provider by
            // construction (AGC-FR-14),
            // and deciding it here is what lets the seam see a request whose
            // entries are already provider-decided.
            native_tools: conversation_native_tools(&provider),
        },
        tools,
        model_calls: AtomicUsize::new(0),
        physical_calls: AtomicUsize::new(0),
        cached_input_tokens: AtomicU64::new(0),
        uncached_input_tokens: AtomicU64::new(0),
        tokens_reported: AtomicBool::new(false),
        agent,
        provider,
        cancelled,
        progress_label,
    };

    let app = app.clone();
    std::thread::spawn(move || {
        run_turn(&app, plan);
    });

    Ok(turn)
}

/// AGC-FR-02: a dispatch refused before it registered anything.
///
/// `WARN` rather than `ERROR`: the application is intact and the surface was
/// told, but the author's question went unanswered and the reason — a nickname
/// naming nobody, an agent whose provider stopped serving it, a thread someone
/// locked — is the thing they will be looking for.
fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    nickname: &str,
    origin: &ConversationOrigin,
    failure: &str,
) {
    logging::log_warn(
        app,
        buffer,
        &[Domain::Ai],
        "agent turn refused",
        log_fields! {
            "agent" => nickname,
            "failure" => failure,
            "originKind" => origin.kind().as_str(),
            "discussionId" => origin.discussion_id(),
        },
    );
}

/// Whether the conversation an origin names refuses further contributions.
pub(super) fn thread_is_locked(roots: Roots<'_>, origin: &ConversationOrigin) -> bool {
    resolve_origin(roots, origin).is_some_and(|d| d.locked)
}

/// The whole life of one turn after dispatch has returned.
pub(super) fn run_turn<R: tauri::Runtime>(app: &tauri::AppHandle<R>, plan: TurnPlan) {
    use tauri::Manager;
    // CVL-FR-28: the whole turn's elapsed time, measured from where its work
    // begins rather than from where it was registered — a turn that waited on a
    // concurrency slot did not spend that time on a model.
    let started = std::time::Instant::now();
    let store = app.state::<GlobalSettingsStore>();
    let ai = app.state::<AiApiIntegrations>();
    let turns = app.state::<TurnRegistry>();
    let progress_registry = app.state::<ProgressRegistry>();

    // CVL-FR-24: a session of its own, begun when the turn starts running and
    // ended when it terminates by any route. A failure to open one is not fatal:
    // only `read_file` needs it, and that tool refuses on its own terms
    // (RFT-FR-17) rather than costing the turn the other four and its answer.
    if let Some(fs_state) = app.try_state::<crate::fs::FsAccessState>() {
        if let Err(e) = fs_state.open_agent_session(&plan.turn_id) {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai],
                "agent session could not be opened",
                log_fields! { "turnId" => &plan.turn_id, "reason" => e.to_string() },
            );
        }
    }

    // A panic anywhere in the loop must still end the turn, on the same terms
    // `crate::search`'s spawned body is guarded (PRG-FR-09): without this the
    // turn never reaches a terminal state, so it stays `running` for the rest of
    // the session, its progress operation never resolves, and its filesystem
    // session leaks until the project closes. The loop widened what can panic —
    // arguments decoded from what a model composed, and three SDK paths — so the
    // guard is the difference between a defect that is reported and one that
    // silently strands a conversation.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_turn_body(app, &store, &ai, &turns, &plan)
    }));

    // Ended on every route out — delivered, failed, cancelled, timed out, or
    // panicked — which is what takes the session's temp directory with it
    // (CVL-FR-24).
    if let Some(fs_state) = app.try_state::<crate::fs::FsAccessState>() {
        fs_state.close_agent_session(&plan.turn_id);
    }

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(_) => {
            // The one failure the vocabulary of AGC-FR-15 has no word for, since
            // every value there names something an author could correct. The
            // record is where the truth lives: a reader seeing `unreachable`
            // against this message knows to look for a defect rather than for a
            // provider that was down.
            logging::log_error(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "agent turn panicked",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "agent" => &plan.agent.nickname,
                    "modelCalls" => plan.model_calls.load(Ordering::SeqCst),
                },
            );
            Err(TurnEnd::Panicked)
        }
    };

    // CVL-FR-18: whether repeating the call could plausibly have helped, decided
    // by the path that ended the turn rather than read back off the failure
    // value — `timed_out` names two deadlines and only one of them is retryable.
    let (state, failure, recoverable) = match outcome {
        Ok(thread) => {
            // CMS-FR-51: an agent's answer is an append like any other, so the
            // thread it landed in announces itself on the same channel a human's
            // comment does. Emitted before the terminal turn event so a rail
            // that redraws on either sees the comment already present.
            comments::emit_discussion_changed(app, &thread);
            (AgentTurnState::Delivered, None, false)
        }
        Err(TurnEnd::Cancelled) => (AgentTurnState::Cancelled, None, false),
        // CVL-FR-16 / CVL-FR-18: the whole-turn deadline, whose budget is spent
        // by definition. Reported as `timed_out` like the per-call one and
        // offered no retry, unlike it.
        Err(TurnEnd::TimedOut) => (AgentTurnState::Failed, Some(FAIL_TIMED_OUT), false),
        Err(TurnEnd::Panicked) => (AgentTurnState::Failed, Some(FAIL_UNREACHABLE), false),
        // AGC-FR-28: terminal like any other — the session above is already
        // closed, the slot already surrendered, and the progress operation ends
        // with the turn. What survives is the registration alone.
        // AGC-FR-VRHM: a recorded question set is terminal on exactly these
        // terms, and is the one `awaiting_reply` that carries no ordinary
        // conversation contribution — the durable set is what the author sees.
        Err(TurnEnd::Asked) | Err(TurnEnd::AskedQuestions) | Err(TurnEnd::Proposed) => {
            (AgentTurnState::AwaitingReply, None, false)
        }
        Err(TurnEnd::Failed(reason)) => (AgentTurnState::Failed, Some(reason), is_recoverable(reason)),
    };
    finish(
        app,
        &turns,
        &progress_registry,
        &plan.turn_id,
        state,
        failure,
        TurnCounts {
            logical: Some(plan.model_calls.load(Ordering::SeqCst)),
            physical: Some(plan.physical_calls.load(Ordering::SeqCst)),
            elapsed_ms: Some(started.elapsed().as_millis() as u64),
            input_tokens: plan.tokens_reported.load(Ordering::SeqCst).then(|| {
                InputTokens {
                    cached: plan.cached_input_tokens.load(Ordering::SeqCst),
                    uncached: plan.uncached_input_tokens.load(Ordering::SeqCst),
                }
            }),
        },
        recoverable,
    );
}
