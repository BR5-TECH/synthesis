//! Delivery of the answer, the record of the turn, and the finish.
//!
//! What a turn produced reaches the surface it came from as an ordinary
//! contribution, and what the turn cost is written to the log as shape
//! alone (AGC-FR-26).

use super::*;

/// AGC-FR-15 / AGC-FR-26: an endpoint that will not resolve terminates the turn
/// with the failure that names what to correct. No part of a key can appear
/// here: the values mapped are `crate::ai_api`'s typed refusals, never a payload.
pub(super) fn classify_endpoint_error(err: &str) -> &'static str {
    match err {
        ai_api::ERR_KEYCHAIN_UNAVAILABLE | ai_api::ERR_KEY_UNAVAILABLE => {
            FAIL_KEYCHAIN_UNAVAILABLE
        }
        _ => FAIL_AGENT_UNAVAILABLE,
    }
}

pub(super) fn deliver(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    author: &Participant,
    answer: String,
) -> Result<comments::Discussion, String> {
    let at = now_rfc3339();
    let discussion = resolve_origin(roots, origin).ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?;
    // CMS-FR-54 / AGC-FR-16: an agent writes into a discussion on exactly the
    // terms an author does, whatever its owner and whether or not it has a
    // fragment — the same append path, the same lock, the same event, stamped with
    // an agent participant.
    comments::append_agent_comment_to(
        roots.store,
        discussion.log_ref(),
        origin.discussion_id(),
        answer,
        author,
        &at,
    )
}

/// AGC-FR-21: one record per turn, wherever it terminated — the run's own exit
/// below, or a cancellation that got there first.
///
/// Emitted beside the terminal event so the panel can be read as a list of
/// conversations and their outcomes. `ERROR` only for a failure: a cancellation
/// is a decision the author made rather than a fault, and a delivery is the
/// ordinary end of a turn.
pub(super) fn log_turn_ended<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    turn: &AgentTurn,
    counts: TurnCounts,
) {
    let level = match turn.state {
        AgentTurnState::Failed => logging::LogLevel::Error,
        _ => logging::LogLevel::Info,
    };
    let mut fields = log_fields! {
            "turnId" => &turn.id,
            "agent" => &turn.nickname,
            "agentId" => &turn.agent_id,
            "state" => turn.state.as_str(),
            // One of AGC-FR-15's own values, never a provider's message.
            "failure" => turn.failure.as_deref().unwrap_or("none"),
            "retryPermitted" => turn.retry_permitted,
            "originKind" => turn.origin.kind().as_str(),
            "discussionId" => turn.origin.discussion_id(),
            // CVL-FR-28: the two counts separately, because a turn that made
            // four calls and one that made four calls across nine attempts are
            // the same turn to the loop and very different ones to the endpoint.
            // Counts and nothing drawn from the exchange — no input section, no
            // tool argument, no tool result, and no part of the answer.
            "modelCalls" => counts.logical,
            "physicalCalls" => counts.physical,
            // CVL-FR-28: how long the whole turn took, which is the number that
            // tells a slow provider from a turn that spent its time in tools.
            "elapsedMs" => counts.elapsed_ms,
    };
    // CVL-FR-28: named only where a provider actually reported the split, so a
    // record carrying no cached total says the provider would not say rather
    // than claiming a confident zero. Counts alone — nothing drawn from the
    // exchange and nothing of any key (CVL-FR-26, AGC-FR-26).
    if let Some(tokens) = counts.input_tokens {
        fields.insert(
            "cachedInputTokens".to_string(),
            serde_json::json!(tokens.cached),
        );
        fields.insert(
            "uncachedInputTokens".to_string(),
            serde_json::json!(tokens.uncached),
        );
    }
    logging::log(
        app,
        buffer,
        level,
        &[Domain::Ai, Domain::Backend],
        "agent turn ended",
        fields,
    );
}

/// CVL-FR-28: what the terminal record reports about a turn's cost.
///
/// `None` throughout where the terminating path did not run the loop and so
/// cannot know, rather than zeroes that would read as "made no calls".
#[derive(Clone, Copy, Default)]
pub(super) struct TurnCounts {
    pub(super) logical: Option<usize>,
    pub(super) physical: Option<usize>,
    pub(super) elapsed_ms: Option<u64>,
    /// CVL-FR-28: the turn's input tokens split by whether the provider had them
    /// cached (CVL-FR-23), or `None` where no call reported the split — which is
    /// the same record a turn that never cached anything leaves, and is meant to
    /// be: both mean the turn paid full price.
    pub(super) input_tokens: Option<InputTokens>,
}

/// AGC-FR-BVNT: one reliable provider-reported usage record, recorded against
/// the draft this turn belongs to.
///
/// A turn whose origin kind is `artifact_comment`, `artifact_discussion`, or
/// `note_discussion` records nothing anywhere: it has no draft to record
/// against, so no statistics log gains a line because of it (AGC-FR-LHRC).
///
/// The record's identity is the turn, the logical round, and the physical
/// attempt together, which is stable across a replay and unique within the
/// application — the providers this loop reaches name no usage identity of
/// their own, and CVL-FR-WQZD asks for one derived deterministically from the
/// call where they do not. Nothing here is estimated: a direction the provider
/// did not report is simply absent from the record.
pub(super) fn record_refinement_usage<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    plan: &TurnPlan,
    round: usize,
    attempt: u32,
    reply: &ModelReply,
) where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let Some(draft_id) = plan.origin.draft_id() else {
        return;
    };
    let tokens = crate::statistics::ReportedTokens {
        input_tokens: reply.prompt_tokens,
        cached_input_tokens: reply.input_tokens.map(|split| split.cached),
        uncached_input_tokens: reply.input_tokens.map(|split| split.uncached),
        output_tokens: reply.output_tokens,
    };
    let record = crate::statistics::UsageRecord {
        usage_id: format!("{}:{round}:{attempt}", plan.turn_id),
        round: round as u32,
        attempt,
        // CVL-FR-WQZD: every provider this loop reaches reports against one
        // call, so nothing here is a turn-level aggregate.
        representation: crate::statistics::Representation::PerCall,
        tokens,
    };
    let events = crate::statistics::usage_events(
        &crate::notes::now_rfc3339_millis(),
        crate::statistics::UsageScope::Conversation,
        crate::statistics::Bucket::Refinement,
        &plan.turn_id,
        std::slice::from_ref(&record),
    );
    crate::statistics::record_statistics_events(app, draft_id, events);
}

/// AGC-FR-YQMD: the one durable line a draft-scoped turn leaves behind.
///
/// Composed when the turn reaches a terminal state and appended once, keyed by
/// the origin's draft id. It carries the turn's own id as its stable identity,
/// the draft, the thread, the origin kind, the instants execution began and
/// ended, the terminal outcome, and whether the turn ever reached the model —
/// and nothing else: no request, no context, no exchange, no active tool call,
/// no failure diagnostic, no comment body, and no word the agent wrote
/// (AGC-FR-LHRC).
///
/// The interval is the turn's **own execution**, so a turn ending in
/// `awaiting_reply` ends it at the question or the proposal it asked with and
/// the wait for the author's reply is outside the record entirely. The whole
/// interval is reported and no telemetry boundary is applied here: the clip is
/// the fold's (per `DSS-draft-statistics-storage.md` DSS-FR-KYTB), so one rule
/// decides it for every producer.
pub(super) fn record_refinement_lifecycle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turn: &AgentTurn,
    reached_model: bool,
) where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let Some(draft_id) = turn.origin.draft_id() else {
        return;
    };
    let Some(ended_at) = turn.ended_at.clone() else {
        return;
    };
    crate::statistics::record_statistics_event(
        app,
        draft_id,
        crate::statistics::EventBody::ConversationTurn {
            turn_id: turn.id.clone(),
            discussion_id: turn.origin.discussion_id().to_string(),
            fragment_targeted: turn.origin.fragment_target.is_some(),
            origin_kind: None,
            reached_model,
            started_at: turn.started_at.clone(),
            ended_at,
            outcome: turn.state.as_str().to_string(),
        },
    );
}

/// The one exit every path takes: stamp the terminal state, publish it exactly
/// once, and release the progress operation (AGC-FR-21, AGC-FR-24).
pub(super) fn finish<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    progress_registry: &ProgressRegistry,
    turn_id: &str,
    state: AgentTurnState,
    failure: Option<&'static str>,
    counts: TurnCounts,
    // AGC-FR-31: whether this failure is one the author may ask again. Passed
    // rather than derived from `failure`, because `timed_out` names both the
    // retryable per-call deadline and the whole-turn one that is not
    // (CVL-FR-17, CVL-FR-18) — only the path that terminated the turn knows
    // which it was.
    recoverable: bool,
) {
    let Some((mut turn, operation, project_key, reached_model)) =
        turns.terminate(turn_id, state, failure)
    else {
        // Already terminated by another path. Publishing again would be a second
        // terminal event for one turn.
        return;
    };
    // AGC-FR-31: entered before the terminal event is published, so the event
    // carrying `retry_permitted` and the query a remounted surface reads cannot
    // disagree about whether an offer stands.
    // AGC-FR-39: a turn that reached its model and answered settles this
    // conversation's notice entry — entering or replacing one where it omitted
    // its images, retiring one where it carried them successfully or carried
    // none at all. A turn that failed or was cancelled reached no model, so it
    // says nothing about the pictures and leaves the entry as it stood.
    //
    // Entered before the terminal event is published, so the event carrying
    // `images_omitted` and the query a remounted surface reads cannot disagree.
    if matches!(state, AgentTurnState::Delivered | AgentTurnState::AwaitingReply) {
        turns.record_image_notice(&turn, &project_key);
    }
    if state == AgentTurnState::Failed && recoverable {
        turn = turns.remember_recoverable(turn, project_key);
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent turn failure can be retried",
            log_fields! {
                "turnId" => &turn.id,
                "agent" => &turn.nickname,
                "failure" => turn.failure.as_deref().unwrap_or("none"),
                "originKind" => turn.origin.kind().as_str(),
                "discussionId" => turn.origin.discussion_id(),
            },
        );
    }
    log_turn_ended(app, turns.buffer(), &turn, counts);
    // AGC-FR-YQMD: `reached_model` is true once one `complete` invocation has
    // been made for the turn, and false where it terminated before any — a
    // refused dispatch, a context the builder could not read, a cancellation
    // before the first call.
    record_refinement_lifecycle(app, &turn, reached_model);
    if let Some(operation) = operation {
        progress::terminate_and_publish(
            app,
            progress_registry,
            operation,
            match state {
                AgentTurnState::Delivered => OperationState::Finished,
                AgentTurnState::Cancelled => OperationState::Cancelled,
                _ => OperationState::Failed,
            },
        );
    }
    publish(app, &turn);
}
