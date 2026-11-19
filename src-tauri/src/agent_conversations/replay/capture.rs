//! Where a turn meets its replay (`ERP-exchange-replay.md` ERP-FR-FEAD to
//! ERP-FR-CRJL, ERP-FR-GOXP): the selection at dispatch, and the capture when
//! the turn ends.

use super::super::*;
use super::layout::{plan_replay, ReplayInputs, ReplayPlan};
use super::memory::{ReplayEntry, ReplayKey, ReplayMemory};
use super::rounds::ending_results;

/// ERP-FR-UFBV: the most characters of a stored state.
pub(in crate::agent_conversations) const STATE_MAX_CHARS: usize = 4_000;

/// ERP-FR-UFBV: the stored state of a state above [`STATE_MAX_CHARS`].
pub(in crate::agent_conversations) const STATE_OMITTED_LINE: &str =
    "[The discussion state of this turn is omitted.]";

/// What the dispatch of a turn hands its replay.
pub(in crate::agent_conversations) struct DispatchReplay<'a> {
    pub turn_id: &'a str,
    pub roots: Roots<'a>,
    pub origin: &'a ConversationOrigin,
    pub agent: &'a Agent,
    pub provider: &'a str,
    /// The trigger comment. `None` for a turn that a speech registered, which
    /// selects nothing (ERP-FR-CMMA).
    pub trigger_comment_id: Option<&'a str>,
    pub tools: &'a [rig::completion::ToolDefinition],
    pub instructions: &'a str,
    pub budget: &'a DocumentBudget,
    /// ERP-FR-DHKJ: an advisor turn selects nothing.
    pub advisor: bool,
}

/// ERP-FR-OPRO, ERP-FR-GOXP: plan the replay of a turn at its dispatch, and
/// emit its record where it selected entries.
pub(in crate::agent_conversations) fn replay_for_dispatch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static logging::LogBuffer,
    turn: DispatchReplay<'_>,
    input: &mut Vec<InputSection>,
) -> ReplayPlan {
    let Some(trigger) = turn.trigger_comment_id.filter(|_| !turn.advisor) else {
        return ReplayPlan::default();
    };
    let Some(thread) = resolve_origin(turn.roots, turn.origin) else {
        return ReplayPlan::default();
    };
    // An application that holds no replay memory replays nothing.
    let Some(memory) = app.try_project_store::<ReplayMemory>() else {
        return ReplayPlan::default();
    };
    let inputs = ReplayInputs {
        roots: turn.roots,
        thread: &thread,
        trigger_comment_id: trigger,
        key: ReplayKey {
            discussion_id: turn.origin.discussion_id().to_string(),
            agent_id: turn.agent.id.clone(),
        },
        offered: turn.tools.iter().map(|t| t.name.to_string()).collect(),
        provider: turn.provider,
        model_id: &turn.agent.model_id,
        window: turn.budget.window,
        history_bound: turn.budget.history_bound_chars(),
        instruction_tokens: instruction_tokens(turn.instructions, turn.tools),
        update_memory: true,
    };
    let plan = plan_replay(&memory, &inputs, input);
    if !plan.blocks.is_empty() || plan.demoted > 0 {
        logging::log_debug(
            app,
            buffer,
            &[Domain::Ai, Domain::Backend],
            "agent turn replay planned",
            log_fields! {
                "turnId" => turn.turn_id,
                "agent" => &turn.agent.nickname,
                "agentId" => &turn.agent.id,
                "provider" => turn.provider,
                "model" => &turn.agent.model_id,
                "selected" => plan.blocks.len(),
                "demoted" => plan.demoted,
                "omittedResults" => plan.omitted,
                "outdatedResults" => plan.outdated,
                "replayTokens" => plan.tokens,
            },
        );
    }
    plan
}

/// How a captured turn ended (ERP-FR-FEAD).
pub(in crate::agent_conversations) enum CaptureEnd<'a> {
    /// The turn delivered the answer comment of this id.
    Answer(String),
    /// The turn ended on the call at `ending` of the reply that asked for
    /// `calls`, and that call gave `result` (ERP-FR-QZNV).
    Contribution { calls: &'a [rig::completion::message::ToolCall], ending: usize, result: &'a str },
    /// The turn ended on its plan card (CVL-FR-KYBE).
    Card,
}

/// ERP-FR-CRJL: why a turn is not captured, or `None` where it is.
pub(in crate::agent_conversations) fn not_captured_because(
    plan: &TurnPlan,
    conversation: &compaction::Conversation,
) -> Option<&'static str> {
    match role_of(plan) {
        Some(GroupRole::Advisor) => return Some("advisor"),
        Some(GroupRole::Synthesis) => return Some("synthesis"),
        _ => {}
    }
    if plan.trigger_comment_id.is_none() {
        return Some("speech");
    }
    if conversation.compacted.is_some() {
        return Some("compacted");
    }
    if !conversation.replayable {
        return Some("steered");
    }
    None
}

/// ERP-FR-FEAD: the id of the answer comment that the agent `agent_id` just
/// appended to `discussion`: its newest comment.
pub(in crate::agent_conversations) fn answer_comment_of(
    discussion: &comments::Discussion,
    agent_id: &str,
) -> Option<String> {
    discussion
        .comments
        .iter()
        .rev()
        .find(|c| matches!(&c.author, Participant::Agent { agent_id: id, .. } if id == agent_id))
        .map(|c| c.id.clone())
}

/// ERP-FR-UFBV: the stored state of an input.
fn stored_state(input: &[InputSection]) -> Vec<InputSection> {
    input
        .iter()
        .filter(|s| s.tag == TAG_DISCUSSION_STATE)
        .map(|s| {
            if s.body.chars().count() <= STATE_MAX_CHARS {
                s.clone()
            } else {
                state_section(STATE_OMITTED_LINE.to_string(), false)
            }
        })
        .collect()
}

/// ERP-FR-GWPG, ERP-FR-QZNV: the rounds of a turn: its messages after its
/// input, less the line of CVL-FR-FSAY, with the results of an ending reply.
pub(in crate::agent_conversations) fn rounds_of(
    plan: &TurnPlan,
    conversation: &compaction::Conversation,
    end: &CaptureEnd<'_>,
) -> Vec<rig::completion::Message> {
    let opening = opening_of(&plan.request);
    let last_call = rig::completion::Message::user(LAST_CALL_LINE);
    let mut rounds: Vec<rig::completion::Message> = conversation
        .exchange
        .iter()
        .skip(opening + 1)
        .filter(|m| **m != last_call)
        .cloned()
        .collect();
    if let CaptureEnd::Contribution { calls, ending, result } = end {
        rounds.extend(ending_results(calls, *ending, result));
    }
    rounds
}

/// ERP-FR-FEAD: capture a turn that ended on the terms of `end` into the
/// replay memory, where ERP-FR-CRJL does not leave it out.
pub(in crate::agent_conversations) fn capture_exchange<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static logging::LogBuffer,
    plan: &TurnPlan,
    conversation: &compaction::Conversation,
    end: CaptureEnd<'_>,
) {
    if let Some(reason) = not_captured_because(plan, conversation) {
        logging::log_debug(
            app,
            buffer,
            &[Domain::Ai, Domain::Backend],
            "agent exchange not captured",
            log_fields! { "turnId" => &plan.turn_id, "reason" => reason },
        );
        return;
    }
    let Some(trigger) = plan.trigger_comment_id.clone() else { return };
    let rounds = rounds_of(plan, conversation, &end);
    let entry = ReplayEntry {
        seq: 0,
        trigger_comment_id: trigger,
        answer_comment_id: match end {
            CaptureEnd::Answer(id) => Some(id),
            _ => None,
        },
        provider: plan.provider.clone(),
        model_id: plan.agent.model_id.clone(),
        tool_names: plan.request.tools.iter().map(|t| t.name.to_string()).collect(),
        rounds,
        state: stored_state(&plan.request.input),
        stamps: conversation.stamps.clone(),
        last_used: 0,
    };
    let chars = entry.chars();
    let rounds = entry.rounds.len();
    let key = ReplayKey {
        discussion_id: plan.origin.discussion_id().to_string(),
        agent_id: plan.agent.id.clone(),
    };
    let Some(memory) = app.try_project_store::<ReplayMemory>() else { return };
    memory.capture(key, entry);
    logging::log_debug(
        app,
        buffer,
        &[Domain::Ai, Domain::Backend],
        "agent exchange captured",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "rounds" => rounds,
            "chars" => chars,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(body: &str) -> InputSection {
        state_section(body.to_string(), false)
    }

    // ERP-FR-UFBV: a state of 4,000 characters is kept, multibyte ones
    // counted as characters; a longer one is the stub; no state is none.
    #[test]
    fn the_stored_state_is_kept_or_stubbed() {
        let at_cap = "é".repeat(STATE_MAX_CHARS);
        let input = vec![
            InputSection { tag: TAG_CURRENT_COMMENT.into(), ..Default::default() },
            state(&at_cap),
        ];
        assert_eq!(stored_state(&input), vec![state(&at_cap)]);
        let over = vec![state(&"é".repeat(STATE_MAX_CHARS + 1))];
        assert_eq!(stored_state(&over), vec![state(STATE_OMITTED_LINE)]);
        assert!(stored_state(&input[..1]).is_empty());
    }
}
