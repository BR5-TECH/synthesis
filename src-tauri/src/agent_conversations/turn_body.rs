//! The body of a turn: the rounds, the bound, and how a turn ends.
//!
//! One pass of the tool loop per round, under the whole-turn deadline of
//! CVL-FR-16, up to the round bound of CVL-FR-13.

use super::*;

pub(super) enum TurnEnd {
    Cancelled,
    /// A defect in this module rather than anything the call did.
    ///
    /// Reported as `unreachable`, the failure vocabulary of AGC-FR-15 having no
    /// word for it — every value there names something an author could correct.
    /// Offered **no** retry all the same: CVL-FR-18's recoverable three are the
    /// failures where repeating the same call could plausibly succeed, and a
    /// deterministic fault repeats deterministically. Offering one would invite
    /// the author to press Retry against a defect until they gave up.
    Panicked,
    /// CVL-FR-16: the **whole-turn** deadline expired.
    ///
    /// Held apart from `Failed(FAIL_TIMED_OUT)` although the two report the same
    /// value, because they differ in the one thing the author can act on: the
    /// per-call deadline of CVL-FR-17 is recoverable and this is not (CVL-FR-18).
    /// A turn whose budget is spent by definition is not one repeating the call
    /// would help, and deriving that from the failure string alone is exactly the
    /// mistake the two spellings invite.
    TimedOut,
    Failed(&'static str),
    /// AGC-FR-RWPT: the provider's certificate was refused. Recoverable, and
    /// carries the host and the cause the turn reports.
    TlsUntrusted(crate::tls::TlsFailure),
    /// CVL-FR-15: the turn asked the author something and ended on it.
    ///
    /// Not a failure and not a delivery: the conversation gained the agent's
    /// question, which is this turn's one contribution (AGC-FR-01), and the
    /// answer will be a turn of its own (AGC-FR-29). It carries no thread
    /// because the tool announced the one it posted into itself — the loop's own
    /// announcement is for an answer this turn now never delivers.
    Asked,
    /// CVL-FR-15: the turn proposed a change and ended on it — to a draft's
    /// prompt, or to a prompt artifact the project holds, whichever its origin
    /// kind gave it the tool for (CVL-FR-08).
    ///
    /// The same terminal state as [`TurnEnd::Asked`] and told apart from it only
    /// so the record the turn ends with says which of its two contributions it
    /// made. What the agent is owed differs — an answer there, a decision here —
    /// but not how the wait is held: both release everything and survive as a
    /// registration alone (AGC-FR-28).
    Proposed,
    /// CVL-FR-15: the turn recorded a **question set** against its discussion
    /// and ended on it (ADQ-FR-FQPA).
    ///
    /// The same terminal state as the two above, and told apart from them
    /// because what it left behind is unlike either: no comment was appended at
    /// all (ADQ-FR-DHZK), so this is the one `awaiting_reply` carrying no
    /// ordinary conversation contribution (AGC-FR-VRHM). The durable pending set
    /// is the whole of the user-facing question state the turn leaves.
    AskedQuestions,
}

/// Returns the thread the answer landed in, so the caller can announce it
/// (CMS-FR-51). A turn that fails or is cancelled returns no thread, because it
/// appended nothing there is anything to announce about (AGC-FR-18).
pub(super) fn run_turn_body<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    turns: &TurnRegistry,
    plan: &TurnPlan,
) -> Result<comments::Discussion, TurnEnd> {
    // AGC-FR-25: a slot, not a refusal. The turn is already `running` and
    // already reported by the time this waits. Recorded because the wait is
    // otherwise indistinguishable from a slow model: a turn that sat behind
    // three others explains a latency the provider had nothing to do with.
    if turns.permits_available() == 0 {
        logging::log_debug(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "agent turn waiting for a call slot",
            log_fields! { "turnId" => &plan.turn_id, "concurrency" => turns.concurrency() },
        );
    }
    let _permit = turns.acquire();
    check_cancelled(plan)?;

    // AGC-FR-24: the turn holds a slot, so it is running and is reported. The
    // operation names the discussion it belongs to (PRG-FR-KXQW), so the status
    // bar reveals the conversation through `revealDiscussion`.
    report_turn_running(app, turns, plan);

    // AGC-FR-12: a turn for which neither `artifact` nor `discussion_history`
    // could be produced fails `context_unavailable` and contributes nothing.
    if !has_material(&plan.request.input) {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "agent turn has no material to send",
            log_fields! {
                "turnId" => &plan.turn_id,
                "originKind" => plan.origin.kind().as_str(),
                "discussionId" => plan.origin.discussion_id(),
            },
        );
        return Err(TurnEnd::Failed(FAIL_CONTEXT_UNAVAILABLE));
    }

    // AGC-FR-14: the provider the project resolved to at dispatch, with the
    // agent's own model and reasoning. The provider's stored model and
    // reasoning selections are never used for an agent.
    let endpoint = ai_api::resolve_ai_api_endpoint(
        store,
        ai,
        &plan.provider,
        &plan.agent.model_id,
        plan.agent.reasoning.clone(),
    )
    .map_err(|e| {
        let failure = classify_endpoint_error(&e);
        // The turn's own failure vocabulary collapses several refusals into
        // `agent_unavailable`, so the record carries `crate::ai_api`'s typed
        // reason as well — the distinction between a provider that was never
        // verified, a model it no longer serves, and a keychain that would not
        // open is the whole of what the author has to correct. Every one of
        // those values is a constant of that module and none is a payload
        // (AGC-FR-26).
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "ai endpoint did not resolve",
            log_fields! {
                "turnId" => &plan.turn_id,
                "provider" => &plan.provider,
                "model" => &plan.agent.model_id,
                "reason" => &e,
                "failure" => failure,
            },
        );
        TurnEnd::Failed(failure)
    })?;
    // The endpoint a call is about to be made against: everything but the key,
    // which exists here only for the duration of the call and is reported as a
    // presence and nothing more (AGC-FR-26 / AAP-FR-07).
    logging::log_debug(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "ai endpoint resolved",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "provider" => &endpoint.provider,
            "baseUrl" => crate::ai_shared::loggable_base_url(&endpoint.base_url),
            "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
            "reasoning" => reasoning_label(endpoint.reasoning.as_ref()),
            // AAP-FR-RTMZ: the route metadata of the model, `both` where it has none.
            "mode" => endpoint.model_mode.map(|m| m.label()).unwrap_or("both"),
            "keyPresent" => endpoint.api_key.is_some(),
        },
    );

    check_cancelled(plan)?;

    // AGC-FR-36: whether a turn may carry image content is decided **here**,
    // before the request is constructed, from the resolved endpoint's own
    // capability (AGC-FR-14, per `AAP-ai-api-integrations.md` AAP-FR-35) — and
    // it is **all or nothing for the turn**. A turn whose endpoint accepts image
    // content carries every image its material holds; one whose endpoint does
    // not carries none of them. A capability the provider declares nothing about
    // counts as absent, so the fallback is what an unknown endpoint gets.
    let carries_images = input_has_images(&plan.request.input);
    let with_images = carries_images && endpoint.accepts_image_input;
    // AGC-FR-37: recorded on the turn from the moment the decision is taken, so
    // every payload the turn appears in from here on carries it.
    let images_omitted = carries_images && !endpoint.accepts_image_input;
    turns.note_images_omitted(&plan.turn_id, images_omitted);
    if carries_images {
        // The count and the decision, and nothing of the pictures themselves:
        // no filename, no media type beyond the count, and above all no payload
        // (AGC-FR-40, CVL-FR-26).
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            if images_omitted {
                "agent turn sending image metadata in place of its images"
            } else {
                "agent turn carrying image content"
            },
            log_fields! {
                "turnId" => &plan.turn_id,
                "provider" => &endpoint.provider,
                "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
                "images" => plan
                    .request
                    .input
                    .iter()
                    .flat_map(|s| &s.parts)
                    .filter(|p| matches!(p, InputPart::Image(_)))
                    .count(),
                "acceptsImageInput" => endpoint.accepts_image_input,
            },
        );
    }

    // CVL-FR-16: one deadline for the whole turn — every model call, every tool
    // call, and the waiting between them. Applied per call it would let a turn
    // that keeps reaching for tools run without limit while no single call ever
    // exceeded it.
    // CVL-FR-PDXK: the bounds are read before the turn and bound it whole, so a
    // provider or a project that raised its turn timeout raises this turn's.
    let bounds = turns.bounds_for_turn(app, endpoint.turn_timeout_ms);
    let deadline = std::time::Instant::now() + bounds.timeout;
    // CVL-FR-12: the exchange opens with the input as its single first message
    // and grows by one entry per reply and one per tool result. CVL-FR-20: a
    // retry presents this exchange byte-for-byte as it stands — nothing
    // downstream inspects a provider, strips a part, downgrades a request, or
    // repairs one, so a request refused for any reason is never re-issued with
    // fewer of its images or with some of them turned into text (CVL-FR-38).
    let mut exchange = opening_exchange(&plan.request, with_images);
    let mut model_calls = 0usize;

    let answer = loop {
        check_cancelled(plan)?;
        if deadline
            .saturating_duration_since(std::time::Instant::now())
            .is_zero()
        {
            log_turn_out_of_time(app, turns, plan, model_calls);
            return Err(TurnEnd::TimedOut);
        }

        model_calls += 1;
        plan.model_calls.store(model_calls, Ordering::SeqCst);
        // CVL-FR-13: whether this is the invocation the logical bound falls on,
        // which is what decides whether a reply asking for tools carries usable
        // content. At the bound those calls are never dispatched, so prose is
        // the whole of what such a reply could still contribute.
        let at_bound = model_calls >= MAX_MODEL_CALLS;
        let reply = complete_with_retries(
            app,
            turns,
            plan,
            &endpoint,
            &exchange,
            bounds.retry,
            deadline,
            model_calls,
            at_bound,
        )?;

        // CVL-FR-31: what the provider ran for itself is reported and recorded
        // here, and goes no further. It was carried out inside the call that has
        // just returned, and the model composed this reply with every result of
        // it already read — so there is nothing to dispatch, nothing to append,
        // and no round of the loop is owed to it.
        report_native_calls(app, turns, plan, &endpoint, model_calls, &reply);

        // CVL-FR-12: a reply that asks for no tool ends the loop and its prose
        // is the answer, whether or not the provider searched or fetched on its
        // way to composing it.
        if reply.tool_calls.is_empty() {
            break reply.text;
        }

        // CVL-FR-13: at the bound the turn delivers whatever prose this reply
        // carried and dispatches no tool it asked for, so an agent that spent
        // its budget searching still contributes what it had concluded.
        if model_calls >= MAX_MODEL_CALLS {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai],
                "agent turn reached its model-call bound",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "agent" => &plan.agent.nickname,
                    "modelCalls" => model_calls,
                    "undispatchedToolCalls" => reply.tool_calls.len(),
                    "replyChars" => reply.text.chars().count(),
                },
            );
            break reply.text;
        }

        exchange.push(assistant_message(&reply));

        // CVL-FR-33: every call this reply asked for is reported begun here —
        // in the order the reply asked for them and before any of their results
        // is appended — so calls made in one round carry an order of their own
        // rather than arriving as a set, and the status a card reads is the one
        // the agent turned to last (CTA-FR-IWOJ).
        let mut activity = ToolActivity::begin(app, turns, &plan.turn_id, &reply.tool_calls);

        // CVL-FR-15: a reply asking for either turn-ending tool ends the turn on
        // that call. Whichever comes first is dispatched first — before anything
        // else the reply asked for — because if it lands, every other call in the
        // reply is left undispatched: their results would be read by nobody, this
        // turn being over, and an agent that has asked something or offered
        // something has said its piece.
        //
        // A reply asking for *both* is this same case rather than a new one: the
        // first is dispatched and the second falls to the one-at-a-time result
        // below, a turn having one contribution and not two (AGC-FR-01).
        //
        // A call that *refused* ends nothing. The loop falls through, the rest
        // of the reply is dispatched as usual, and the refusal goes back to the
        // model like any other (CVL-FR-14) — which also keeps the exchange
        // well-formed, every tool call in it answered by a result before the
        // next model call carries it.
        let ending_first = reply.tool_calls.iter().position(|call| {
            call.function.name == crate::tools::ask_user_comment::NAME
                // CVL-FR-15: a discussion turn's question set ends the turn on
                // exactly the terms a posted question does, although it appends
                // no comment at all (ADQ-FR-DHZK, ADQ-FR-FQPA).
                || call.function.name == crate::tools::ask_discussion_questions::NAME
                || call.function.name == crate::tools::propose_draft_changes::NAME
                // CVL-FR-15: the artifact origins' proposal tool ends a turn on
                // exactly the terms the draft origins' does. The two are never
                // attached together (CVL-FR-08), so at most one of these three
                // names can be present in any reply this loop ever reads.
                || call.function.name == crate::tools::propose_prompt_changes::NAME
        });
        if let Some(index) = ending_first {
            // CVL-FR-33: a call abandoned by cancellation is reported finished
            // like any other, so the turn holds none of this round's calls
            // active once the loop has stopped working on them.
            if let Err(end) = check_cancelled(plan) {
                activity.finish_all(app, turns, &plan.turn_id);
                return Err(end);
            }
            let call = &reply.tool_calls[index];
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let result = dispatch_tool(app, turns, plan, call, remaining);
            activity.finish(app, turns, &plan.turn_id, index);
            // Read after the dispatch rather than inferred from the call's name:
            // the name says what the model asked for, and only the flag says
            // whether it actually went out (CVL-FR-15).
            let ended = if plan.asked.load(Ordering::SeqCst) {
                Some((TurnEnd::Asked, "agent asked the author a question"))
            } else if plan.asked_questions.load(Ordering::SeqCst) {
                // Told apart from the two above in the record as well as in the
                // state: this turn left a durable set and no comment, so a
                // reader looking for the agent's contribution in the log would
                // otherwise find none and call the turn broken.
                Some((
                    TurnEnd::AskedQuestions,
                    "agent recorded a question set",
                ))
            } else if plan.proposed.load(Ordering::SeqCst) {
                // One flag for both proposal tools: which of them a turn holds is
                // decided by its origin kind (CVL-FR-08), so the flag alone says
                // whether an offer actually went out.
                Some((TurnEnd::Proposed, "agent proposed a change"))
            } else {
                None
            };
            if let Some((end, message)) = ended {
                logging::log_info(
                    app,
                    turns.buffer(),
                    &[Domain::Ai],
                    message,
                    log_fields! {
                        "turnId" => &plan.turn_id,
                        "agent" => &plan.agent.nickname,
                        "originKind" => plan.origin.kind().as_str(),
                        "discussionId" => plan.origin.discussion_id(),
                        "modelCalls" => model_calls,
                        // What was left on the table, so a reader can tell this
                        // turn from one that ended on its first move. Never the
                        // question or the proposed text, which are about to be a
                        // comment and a candidate anyone can read, and never an
                        // argument (CVL-FR-26).
                        "undispatchedToolCalls" => reply.tool_calls.len() - 1,
                        "discardedReplyChars" => reply.text.chars().count(),
                    },
                );
                // CVL-FR-33: the turn ends on this call, so every other call the
                // reply asked for is abandoned — reported finished rather than
                // left active behind a turn that has stopped working on them.
                activity.finish_all(app, turns, &plan.turn_id);
                return Err(end);
            }
            exchange.push(rig::completion::Message::tool_result(
                call.id.clone(),
                result,
            ));
        }

        for (index, call) in reply.tool_calls.iter().enumerate() {
            // Already dispatched above, and its result already in the exchange.
            if Some(index) == ending_first {
                continue;
            }
            // Reached only where the ending call above *refused*, since one that
            // landed ended the turn. A second is never carried out: were it
            // dispatched here it would land, and the loop — already past its
            // check — would carry on to answer as well, leaving one turn with
            // two contributions (AGC-FR-01). The model is told so rather than
            // left with an unanswered call. This is also what a reply asking for
            // one of each reaches, the first having been dispatched above.
            let one_at_a_time = if call.function.name == crate::tools::ask_user_comment::NAME {
                Some(crate::tools::ask_user_comment::ONE_AT_A_TIME)
            } else if call.function.name == crate::tools::ask_discussion_questions::NAME {
                Some(crate::tools::ask_discussion_questions::ONE_AT_A_TIME)
            } else if call.function.name == crate::tools::propose_draft_changes::NAME {
                Some(crate::tools::propose_draft_changes::ONE_AT_A_TIME)
            } else if call.function.name == crate::tools::propose_prompt_changes::NAME {
                Some(crate::tools::propose_prompt_changes::ONE_AT_A_TIME)
            } else {
                None
            };
            if let Some(message) = one_at_a_time {
                exchange.push(rig::completion::Message::tool_result(
                    call.id.clone(),
                    message.to_string(),
                ));
                // CVL-FR-33: a refusal is a result produced, so the call leaves
                // the active list exactly as a successful one does.
                activity.finish(app, turns, &plan.turn_id, index);
                continue;
            }
            // CVL-FR-25: cancellation reaches the loop wherever it stands, and a
            // result that arrives afterwards is discarded rather than appended.
            if let Err(end) = check_cancelled(plan) {
                activity.finish_all(app, turns, &plan.turn_id);
                return Err(end);
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let result = dispatch_tool(app, turns, plan, call, remaining);
            activity.finish(app, turns, &plan.turn_id, index);
            exchange.push(rig::completion::Message::tool_result(
                call.id.clone(),
                result,
            ));
        }
        // Every call of this round has been reported finished by now; this is
        // the backstop that keeps that true whatever is added above.
        activity.finish_all(app, turns, &plan.turn_id);
    };

    // AGC-FR-15 / CVL-FR-13: a model that answered with nothing at all — whether
    // it simply had nothing to say or spent its last call asking for tools it
    // will not get — leaves nothing to deliver, and a conversation gains a line
    // only when an agent actually answered (AGC-FR-18).
    //
    // A backstop rather than the path that ordinarily reports it: emptiness is
    // decided inside the invocation, where it is retried three times before the
    // turn is failed for it (CVL-FR-19), so every reply reaching here already
    // carried usable content for the position it broke the loop at. This stands
    // because the invariant is worth asserting rather than assuming — a future
    // break out of the loop that forgot it would otherwise append an empty
    // comment to somebody's conversation.
    if answer.trim().is_empty() {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "model answered with no text",
            log_fields! {
                "turnId" => &plan.turn_id,
                "agent" => &plan.agent.nickname,
                "modelCalls" => model_calls,
                "physicalCalls" => plan.physical_calls.load(Ordering::SeqCst),
                "failure" => FAIL_EMPTY_REPLY,
            },
        );
        return Err(TurnEnd::Failed(FAIL_EMPTY_REPLY));
    }

    // AGC-FR-20: a cancellation that arrived while the model was answering
    // contributes nothing. The claim is what makes that a decision rather than a
    // race: past it, no cancellation can terminate this turn, so the state the
    // author is shown always agrees with what is in the thread.
    if !turns.begin_delivery(&plan.turn_id) {
        logging::log_debug(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "agent answer discarded: the turn was cancelled",
            log_fields! { "turnId" => &plan.turn_id },
        );
        return Err(TurnEnd::Cancelled);
    }

    // AGC-FR-16: the reply is appended through the agent write path, stamped
    // with an agent participant carrying the agent's id, its nickname as the
    // handle, and the model that produced the answer. The result is a comment
    // the fold, the rail, and the Comments panel read exactly as a human's.
    deliver(plan.roots.borrowed(), &plan.origin, &plan.author, answer).map_err(|e| {
        let failure = if e == comments::ERR_DISCUSSION_LOCKED {
            // AGC-FR-19: locked while the turn was in flight.
            FAIL_THREAD_LOCKED
        } else {
            FAIL_CONTEXT_UNAVAILABLE
        };
        // The most expensive failure there is: the model answered, the tokens
        // were spent, and the conversation gained nothing (AGC-FR-18). Without
        // this record the turn reads as though it never got that far.
        logging::log_error(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "agent answer could not be appended",
            log_fields! {
                "turnId" => &plan.turn_id,
                "originKind" => plan.origin.kind().as_str(),
                "discussionId" => plan.origin.discussion_id(),
                "reason" => &e,
                "failure" => failure,
            },
        );
        TurnEnd::Failed(failure)
    })
}

/// CVL-FR-16: the whole-turn deadline expired. One record, wherever the loop
/// noticed.
pub(super) fn log_turn_out_of_time<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    model_calls: usize,
) {
    logging::log_error(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "agent turn ran out of time",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "modelCalls" => model_calls,
            "physicalCalls" => plan.physical_calls.load(Ordering::SeqCst),
            "failure" => FAIL_TIMED_OUT,
            // CVL-FR-18: the whole-turn deadline, which is never repeated. The
            // spelling it shares with the per-call one is exactly why the record
            // says which it was.
            "deadline" => "whole_turn",
        },
    );
}

/// CVL-FR-13 / CVL-FR-18: whether a reply supplies anything the turn can use.
///
/// An empty reply is a completion outcome that supplies neither usable prose nor
/// a tool request. At the logical bound a tool request is not usable either —
/// CVL-FR-13 dispatches none of them there — so prose is the whole of what such
/// a reply could still contribute, which is what makes the bound's own
/// `empty_reply` the same outcome as any other rather than a special case.
///
/// CVL-FR-UJXD: a reply that asks for no tool and whose whole text is a tool
/// call written as prose supplies no usable prose either, so it is retried as
/// the same logical call rather than delivered as raw JSON.
pub(super) fn reply_is_usable(
    reply: &ModelReply,
    at_bound: bool,
    tools: &[rig::completion::ToolDefinition],
) -> bool {
    if reply.tool_calls.is_empty() && tool_written_as_prose(&reply.text, tools).is_some() {
        return false;
    }
    !reply.text.trim().is_empty() || (!reply.tool_calls.is_empty() && !at_bound)
    // CVL-FR-31: provider-native calls do not make a reply usable. The provider
    // ran them inside this same call and gave their results to the model before
    // it composed this reply, so a reply that searched and then said nothing has
    // said nothing — there is no round that follows in which it would say what
    // it made of the result, and calling again with an unchanged exchange would
    // only ask the same question twice.
}

/// AGC-FR-24: register the progress operation of a turn that holds a slot.
///
/// A cancellation that terminated the turn between the slot being taken and
/// this call leaves no live turn to attach to, so the operation is ended at
/// once as cancelled and is never left in flight (PRG-FR-09).
fn report_turn_running<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
) {
    use tauri::Manager;
    let progress_registry = app.state::<ProgressRegistry>();
    let operation = progress::register_and_publish_with_activation(
        app,
        &progress_registry,
        "agent",
        &plan.progress_label,
        Some(plan.roots.worktree.to_path_buf()),
        None,
        Some(progress::Activation::Discussion {
            discussion_id: plan.origin.discussion_id().to_string(),
        }),
    );
    if !turns.attach_operation(&plan.turn_id, operation) {
        progress::terminate_and_publish(
            app,
            &progress_registry,
            operation,
            OperationState::Cancelled,
        );
    }
}
