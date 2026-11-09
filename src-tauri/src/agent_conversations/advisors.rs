//! The advisors tool of a Meta turn (AGC-FR-HVQD): the call that starts the
//! advisors, waits for the end of an open speech, and returns their advice.
//!
//! The system starts no advisor by itself. A Meta turn calls `ask_advisors`
//! with its own question. Each call is one fork-join of its own
//! (AGC-FR-ZOCI), and its result is the advice of the advisors that finished.

use super::*;
use crate::agent_groups::{
    ready_nicknames, render_advisors_section, GroupMode, GroupState, JoinOutcome, MemberStatus,
};

/// AGC-FR-HVQD: the name of the advisors tool.
pub const ADVISORS_TOOL_NAME: &str = "ask_advisors";

/// CVL-FR-RBQV: the fixed text the model reads.
const ADVISORS_TOOL_DESCRIPTION: &str = "Ask the other agents for advice before you answer. Give your own question in `question`. Each other agent reads the message of the author and your question, and answers with important points and key questions. Call it at the start of a discussion. A later call is optional, and you can answer with no call. The call can take some time. The result holds the advice of each agent that answered.";

const QUESTION_DESCRIPTION: &str = "Your own question for the advisors, in your own words. It says what you want to know before you answer the author.";

/// AGC-FR-HVQD: the longest question, in characters.
const QUESTION_LIMIT: usize = 1000;

/// AGC-FR-WBZN: the time a call can take in the tool dispatcher. The join
/// timeout and the cancel of the turn bound the call, so this limit is only a
/// backstop for a defect.
pub(super) const ADVISORS_CALL_LIMIT: Duration = Duration::from_secs(6 * 60 * 60);

/// AGC-FR-HVQD: the result of a call that found no advisor to ask.
const NO_ADVISOR_READY: &str =
    "No other agent is ready to advise. Go on without advice and answer the author.";

/// AGC-FR-ANSK: the result of a call whose advisors gave no advice.
const NO_ADVICE: &str = "No advice came: every advisor failed or ran out of time. Go on without advice and answer the author.";

/// AGC-FR-HVQD: what a Meta turn needs to make the call.
#[derive(Clone)]
pub(super) struct AdvisorsSetup {
    /// AGC-FR-ZLWN: the join timeout of each call.
    pub(super) timeout: Duration,
}

/// AGC-FR-VDOO: the groups of the calls that one Meta turn made, in order.
#[derive(Default)]
pub(super) struct AdvisorCalls {
    states: Mutex<Vec<Arc<GroupState>>>,
}

impl AdvisorCalls {
    fn push(&self, state: Arc<GroupState>) {
        self.states.lock().unwrap_or_else(|e| e.into_inner()).push(state);
    }

    /// AGC-FR-VDOO: the `advisors` attachment over all calls. An advisor that a
    /// later call asked again shows the state of its last call.
    pub(super) fn attachment(&self) -> Option<comments::Attachment> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut advisors: Vec<comments::AdvisorEntry> = Vec::new();
        for state in &states {
            let entries = state.advice_entries();
            for (handle, status) in state.statuses() {
                let entry = advisor_entry(state, &entries, handle, status);
                match advisors.iter_mut().find(|known| known.handle == entry.handle) {
                    Some(known) => *known = entry,
                    None => advisors.push(entry),
                }
            }
        }
        (!advisors.is_empty()).then_some(comments::Attachment::Advisors { advisors })
    }
}

/// Everything the tool of one Meta turn is bound to.
struct CallContext<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    project_key: String,
    origin: ConversationOrigin,
    meta_turn_id: String,
    meta_nickname: String,
    author: Participant,
    cancelled: Arc<AtomicBool>,
    setup: AdvisorsSetup,
    calls: Arc<AdvisorCalls>,
}

/// What the call of the tool needs from the turn that makes it.
pub(super) struct AdvisorsBinding<R: tauri::Runtime> {
    pub(super) app: tauri::AppHandle<R>,
    pub(super) roots: OwnedRoots,
    pub(super) project_key: String,
    pub(super) origin: ConversationOrigin,
    pub(super) meta_turn_id: String,
    pub(super) meta_nickname: String,
    pub(super) author: Participant,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) setup: AdvisorsSetup,
    pub(super) calls: Arc<AdvisorCalls>,
}

fn tool_name() -> rig::completion::message::ToolName {
    rig::completion::message::ToolName::new(ADVISORS_TOOL_NAME).expect("a tool name")
}

/// AGC-FR-HVQD: the advisors tool of one Meta turn.
pub(super) fn advisors_tool<R: tauri::Runtime>(
    binding: AdvisorsBinding<R>,
) -> rig::tool::DynamicTool {
    use rig::tool::IntoToolOutput;
    let context = Arc::new(CallContext {
        app: binding.app,
        roots: binding.roots,
        project_key: binding.project_key,
        origin: binding.origin,
        meta_turn_id: binding.meta_turn_id,
        meta_nickname: binding.meta_nickname,
        author: binding.author,
        cancelled: binding.cancelled,
        setup: binding.setup,
        calls: binding.calls,
    });
    let parameters = serde_json::json!({
        "type": "object",
        "properties": { "question": { "type": "string", "description": QUESTION_DESCRIPTION } },
        "required": ["question"],
        "additionalProperties": false,
    });
    rig::tool::DynamicTool::new(
        tool_name(),
        ADVISORS_TOOL_DESCRIPTION,
        parameters,
        move |arguments: serde_json::Value| {
            let context = Arc::clone(&context);
            Box::pin(async move {
                match run_call(&context, &arguments) {
                    Ok(text) => text.into_tool_output(),
                    Err(error) => Err(error),
                }
            })
        },
    )
}

fn invalid(message: &str) -> rig::tool::ToolExecutionError {
    rig::tool::ToolExecutionError::new(rig::tool::ToolErrorKind::InvalidArgs, message)
        .with_retryable(true)
}

/// AGC-FR-HVQD: the question of Meta, trimmed. An empty question or a question
/// over the limit is refused.
pub(super) fn question_of(
    arguments: &serde_json::Value,
) -> Result<String, rig::tool::ToolExecutionError> {
    let question = arguments.get("question").and_then(|q| q.as_str()).unwrap_or("").trim();
    if question.is_empty() {
        return Err(invalid("The question is empty. Give your own question for the advisors."));
    }
    if question.chars().count() > QUESTION_LIMIT {
        return Err(invalid(
            "The question is too long. Give a shorter question, up to 1000 characters.",
        ));
    }
    Ok(question.to_string())
}

fn meta_question_section(question: &str) -> InputSection {
    InputSection {
        tag: "meta_question".to_string(),
        attributes: Vec::new(),
        body: question.to_string(),
        truncated: false,
        parts: Vec::new(),
    }
}

/// AGC-FR-HVQD, AGC-FR-KRTP: one call of the tool.
fn run_call<R: tauri::Runtime>(
    context: &CallContext<R>,
    arguments: &serde_json::Value,
) -> Result<String, rig::tool::ToolExecutionError> {
    use tauri::Manager;
    let turns = context.app.state::<TurnRegistry>();
    let question = question_of(arguments)?;
    logging::log_info(
        &context.app,
        turns.buffer(),
        &[Domain::Ai],
        "advisors tool called",
        log_fields! {
            "turnId" => &context.meta_turn_id,
            "agent" => &context.meta_nickname,
            "questionChars" => question.chars().count(),
        },
    );
    // AGC-FR-HVQD: the Meta turn frees its call slot while it waits, so the
    // advisors can use it. The turn takes a slot again before its next model call.
    turns.release_slot(&context.meta_turn_id);
    // AGC-FR-KRTP: with the speech open, no advisor starts before its comment.
    let comment_id =
        match turns.wait_for_speech_end(&context.app, &context.meta_turn_id, &context.cancelled) {
            SpeechWait::Ended(Some(comment_id)) => comment_id,
            SpeechWait::Ended(None) | SpeechWait::Cancelled => {
                logging::log_info(
                    &context.app,
                    turns.buffer(),
                    &[Domain::Ai],
                    "advisors tool call ended with no comment",
                    log_fields! { "turnId" => &context.meta_turn_id },
                );
                return Err(rig::tool::ToolExecutionError::other(
                    "The turn ended before the advisors could start.",
                ));
            }
        };
    let store = context.app.state::<GlobalSettingsStore>();
    let roots = context.roots.borrowed();
    let roster = agents::list_project_agents_impl(&store, &context.project_key, roots.worktree)
        .unwrap_or_default();
    let advisors: Vec<String> = ready_nicknames(&roster)
        .into_iter()
        .filter(|nickname| !same(nickname, &context.meta_nickname))
        .collect();
    if advisors.is_empty() {
        return Ok(NO_ADVISOR_READY.to_string());
    }
    let state = Arc::new(GroupState::new(new_group_id(), GroupMode::Advisor));
    let mut started = 0usize;
    for advisor in &advisors {
        match dispatch_grouped(
            &context.app,
            roots,
            &context.project_key,
            advisor,
            context.origin.clone(),
            Trigger::Comment(&comment_id),
            Some(GroupSetup {
                state: Arc::clone(&state),
                role: GroupRole::Advisor,
                meta_turn_id: Some(context.meta_turn_id.clone()),
                extra_sections: vec![meta_question_section(&question)],
                instructions_suffix: Some(ADVISOR_PROMPT.to_string()),
                advisors: None,
            }),
        ) {
            Ok(_) => started += 1,
            Err(refusal) => logging::log_warn(
                &context.app,
                turns.buffer(),
                &[Domain::Ai],
                "an advisor could not be dispatched",
                log_fields! { "refusal" => refusal, "groupId" => &state.id },
            ),
        }
    }
    // The wait below is reached on every path.
    state.seal(context.setup.timeout, std::time::Instant::now());
    context.calls.push(Arc::clone(&state));
    logging::log_info(
        &context.app,
        turns.buffer(),
        &[Domain::Ai],
        "advisor group dispatched",
        log_fields! { "groupId" => &state.id, "advisors" => started },
    );
    if started == 0 {
        return Ok(NO_ADVICE.to_string());
    }
    match state.wait_join(&context.cancelled) {
        JoinOutcome::Cancelled => {
            // AGC-FR-ANSK: an advisor has no pending row and no Cancel of its
            // own, so the cancelled Meta turn ends the advisors that still run.
            let pending = state.skip_pending(SkipReason::Cancelled);
            skip_pending(&context.app, &pending, SkipReason::Cancelled);
            return Err(rig::tool::ToolExecutionError::other("The turn was cancelled."));
        }
        JoinOutcome::Over { pending, timed_out } => {
            skip_pending(&context.app, &pending, SkipReason::TimedOut);
            logging::log_info(
                &context.app,
                turns.buffer(),
                &[Domain::Ai],
                "advisor group joined",
                log_fields! {
                    "turnId" => &context.meta_turn_id,
                    "groupId" => &state.id,
                    "finished" => state.finished_count(),
                    "advisors" => state.member_count(),
                    "timedOut" => timed_out,
                },
            );
        }
    }
    // AGC-FR-VDOO: the comment of a question or a proposal that a tool of this
    // turn appends carries the list too.
    if let Some(attachment) = context.calls.attachment() {
        comments::set_turn_marks(context.origin.discussion_id(), &context.author, vec![attachment]);
    }
    Ok(result_text(&state))
}

/// AGC-FR-HVQD: the result of a call.
fn result_text(state: &GroupState) -> String {
    let entries = state.advice_entries();
    if entries.is_empty() {
        return NO_ADVICE.to_string();
    }
    let mut text = render_advisors_section(&entries);
    for (nickname, status) in state.statuses() {
        if let MemberStatus::Skipped(reason) = status {
            text.push_str(&format!("\n\n@{nickname}: skipped ({})", reason.label()));
        }
    }
    text
}

/// AGC-FR-WBZN: the call of the advisors tool ran, so the whole-turn deadline
/// restarts. The name tells the dispatcher which call it is.
pub(super) fn is_advisors_call(name: &str) -> bool {
    name == ADVISORS_TOOL_NAME
}
