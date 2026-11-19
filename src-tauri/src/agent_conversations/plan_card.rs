//! How a Map turn that posted a plan card ends (`CVL-conversation-loop.md`
//! CVL-FR-KYBE to CVL-FR-SUIZ).
//!
//! `propose_plan_changes` does not end the turn. Its first successful call
//! posts the card of the turn, and later calls update the card with no
//! comment. The turn ends when the model answers with no tool call. The answer
//! is a closing comment where a later call changed the card or the turn is a
//! Synthesis turn, and it is discarded otherwise.

use super::*;

/// CVL-FR-WJKS: what a call that asks the author, or records a question set or
/// an export request, is told after the turn posted its card.
pub(super) const CARD_POSTED_ASK_IN_REPLY: &str = "This turn already posted a plan proposal, so nothing was asked or recorded. Ask in your reply instead, and end the turn with it.";

/// CVL-FR-KYBE: whether the answer of the turn becomes a closing comment.
fn posts_closing(plan: &TurnPlan, card: &crate::tools::propose_plan_changes::CardState) -> bool {
    card.changed_after_comment() || role_of(plan) == Some(GroupRole::Synthesis)
}

/// CVL-FR-KYBE, CVL-FR-NUOY: whether the turn has a card and its answer is
/// discarded. A discarded answer can be empty and needs no spoken part.
pub(super) fn answer_is_discarded(plan: &TurnPlan) -> bool {
    let card = plan.plan_card.snapshot();
    card.proposal_id.is_some() && !posts_closing(plan, &card)
}

/// CVL-FR-KYBE, CVL-FR-NUOY: end a turn that has a plan card on its answer.
/// The turn terminates `awaiting_reply` whatever happens to the answer, because
/// the card is its contribution (AGC-FR-ZPOI). A cancellation that arrived
/// before the claim still cancels it.
pub(super) fn end_with_card<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    answer: String,
    model_calls: usize,
) -> Result<comments::Discussion, TurnEnd> {
    let card = plan.plan_card.snapshot();
    let proposal_id = card.proposal_id.clone().unwrap_or_default();
    let closing = posts_closing(plan, &card) && !answer.trim().is_empty();
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "agent turn ended on its plan card",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "discussionId" => plan.origin.discussion_id(),
            "modelCalls" => model_calls,
            "changed" => card.changed,
            "added" => card.added,
            "withdrawn" => card.withdrawn,
            "closingComment" => closing,
            // CVL-FR-NUOY: the length of a discarded answer, and never its text.
            "discardedReplyChars" => if closing { 0 } else { answer.chars().count() },
        },
    );
    // AGC-FR-20: past the claim, no cancellation can terminate this turn.
    if !turns.begin_delivery(&plan.turn_id) {
        return Err(TurnEnd::Cancelled);
    }
    // AGC-FR-CZCU: a participant of a synthesis group keeps what it said for
    // the synthesis, also where the answer is not posted.
    let participant = role_of(plan) == Some(GroupRole::Participant);
    if !closing {
        if participant && !answer.trim().is_empty() {
            record_reply(plan, &answer);
        }
        return Err(TurnEnd::Proposed);
    }
    if resolved_during_turn(plan) {
        log_closing_failed(app, turns, plan, comments::ERR_DISCUSSION_RESOLVED);
        return Err(TurnEnd::Proposed);
    }
    // CVL-FR-NUOY: the closing comment names the card and the counts of the
    // turn.
    let mut attachments = reply_attachments(plan);
    attachments
        .push(comments::Attachment::MapPlanProposal { proposal_id, update: Some(card.counts()) });
    let content = comments::AnswerContent {
        body: answer.clone(),
        attachments,
        tool_calls: saved_tool_calls(&turns.tool_calls_of(&plan.turn_id)),
    };
    match deliver_with(plan.roots.borrowed(), &plan.origin, &plan.author, content) {
        Ok(thread) => {
            // CMS-FR-51: the thread announces the new comment.
            comments::emit_discussion_changed(app, &thread);
            if participant {
                record_reply(plan, &answer);
            }
        }
        Err(reason) => log_closing_failed(app, turns, plan, &reason),
    }
    Err(TurnEnd::Proposed)
}

/// A closing comment that could not be appended. The card stays the
/// contribution of the turn.
fn log_closing_failed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    reason: &str,
) {
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "closing comment of the plan card could not be appended",
        log_fields! {
            "turnId" => &plan.turn_id,
            "discussionId" => plan.origin.discussion_id(),
            "reason" => reason,
        },
    );
}

/// MPR-FR-BCKW: whether a turn of the agent `nickname` runs in the Map
/// discussion `discussion_id`. A turn that awaits a reply does not run.
pub(crate) fn agent_turn_runs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discussion_id: &str,
    nickname: &str,
) -> bool {
    let Some(turns) = tauri::Manager::try_state::<TurnRegistry>(app) else { return false };
    turns.in_flight(None).iter().any(|turn| {
        turn.state == AgentTurnState::Running
            && turn.nickname == nickname
            && turn.origin.discussion_id() == discussion_id
    })
}

/// CVL-FR-SUIZ: a turn that failed or was cancelled after a later call changed
/// its card leaves the changes on the card. The record carries the counts and
/// no row text.
pub(super) fn log_card_left_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    state: AgentTurnState,
) {
    let card = plan.plan_card.snapshot();
    if !card.changed_after_comment()
        || !matches!(state, AgentTurnState::Failed | AgentTurnState::Cancelled)
    {
        return;
    }
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "agent turn ended after it changed its plan card",
        log_fields! {
            "turnId" => &plan.turn_id,
            "discussionId" => plan.origin.discussion_id(),
            "state" => format!("{state:?}"),
            "changed" => card.changed,
            "added" => card.added,
            "withdrawn" => card.withdrawn,
        },
    );
}
