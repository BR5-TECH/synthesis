//! What a provider-native call reports and logs (CVL-FR-33 .. CVL-FR-35).
//!
//! A native call is carried by the provider, so the only record of it is what
//! the reply says it did — reported as progress and logged as shape.

use super::*;

/// CVL-FR-31 / CVL-FR-33: report what the provider ran for itself, and record
/// it.
///
/// Nothing is dispatched here and nothing is appended to the exchange. OpenRouter
/// runs its own tool loop **inside** the model call this module had already made:
/// it executes the search or the fetch, gives the result to the model, and
/// returns the reply the model composed with that result already in front of it.
/// So by the time one of these reaches this function the work is finished and
/// the model has read it — replaying the call and the result into the next
/// request would present the provider with a function call for a tool the
/// request never declared, which is a malformed request rather than a
/// continuation of the conversation.
///
/// What is left to do with one is therefore what a surface and a log need: each
/// call is reported begun and immediately finished, in the order the provider
/// reported them, so a conversation shows the search while the turn is still
/// running (AGC-FR-33); and each is recorded (CVL-FR-34).
pub(super) fn report_native_calls<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    model_calls: usize,
    reply: &ModelReply,
) {
    // CVL-FR-33: what to report as active. Ordinarily one per call the provider
    // reported. But a provider that serves a search *natively* reports no call
    // for it at all — its reply carries reasoning blocks naming no tool, and its
    // citations ride elsewhere — and the only thing that says a search happened
    // is the count. Reporting nothing there would leave a conversation blank
    // while its agent searched, which is the whole of what this reporting is
    // for, so the count stands in for the calls the provider did not name.
    //
    // The counts of CVL-FR-34 are recorded whether or not any call came with
    // them: a provider that counted a call and executed none reports no call at
    // all, which is the one case the counts exist to make visible.
    let reportable: Vec<&str> = if reply.native_calls.is_empty() {
        let counted = reply
            .native_usage
            .and_then(|usage| usage.web_searches)
            .unwrap_or(0)
            // The provider's own step budget bounds this; the cap is what stops
            // a number this application did not choose becoming a number of
            // events it emits.
            .min(MAX_COUNTED_NATIVE_REPORTS) as usize;
        vec![crate::tools::web_search::ENTRY_TYPE; counted]
    } else {
        reply.native_calls.iter().map(|call| call.name.as_str()).collect()
    };
    let ids: Vec<Option<String>> = reportable
        .iter()
        .map(|name| report_tool_begun(app, turns, &plan.turn_id, name))
        .collect();
    log_native_calls(app, turns, plan, endpoint, model_calls, reply);
    for id in ids.into_iter().flatten() {
        report_tool_finished(app, turns, &plan.turn_id, &id);
    }
}

/// CVL-FR-34: one record per tool the provider ran for itself.
///
/// The tool is named, because a reader diagnosing a turn that reached the web
/// needs to know whether it searched or fetched, and the two fail in entirely
/// different ways. The name is the provider's rather than this application's —
/// nothing here declares it and nothing here rewrites it (WST-FR-07, WFT-FR-07)
/// — so a record says what the provider said it ran. What is never named is
/// anything the model composed or anything the provider produced (CVL-FR-26,
/// CVL-FR-27): the arguments and the result are reported by their **length**
/// alone, which says whether a call carried a query at all and whether anything
/// came back for it without putting a query, an address, or a line of a page
/// into a log.
///
/// A call whose result is empty is recorded at `WARN`. The provider produced
/// nothing for it, which is not this turn's failure (CVL-FR-32) — the model
/// reads it and carries on — but it is the shape a refused, exhausted, or
/// unserved server tool takes, and it is worth finding in a log without
/// reproducing the turn.
pub(super) fn log_native_calls<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    model_calls: usize,
    reply: &ModelReply,
) {
    if reply.native_calls.is_empty() && reply.native_usage.is_none() {
        return;
    }
    let model_label = endpoint.model_id.as_deref().unwrap_or("");
    for (index, call) in reply.native_calls.iter().enumerate() {
        let empty_result = call.result.trim().is_empty();
        let fields = log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "provider" => &endpoint.provider,
            "model" => model_label,
            "modelCall" => model_calls,
            // Which of the provider's tools ran, and where in the reply it stood.
            "tool" => &call.name,
            "callId" => &call.id,
            "callIndex" => index,
            // Shape and never content (CVL-FR-26): how much the model composed
            // and how much came back, in characters.
            "argumentChars" => call.arguments.chars().count(),
            "resultChars" => call.result.chars().count(),
            "emptyResult" => empty_result,
        };
        if empty_result {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend, Domain::Remote],
                "provider tool produced no result",
                fields,
            );
        } else {
            logging::log_debug(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend, Domain::Remote],
                "provider ran its own tool",
                fields,
            );
        }
    }

    // CVL-FR-34: the provider's own counts beside the calls it reported, which
    // is what tells the two apart — a request the provider counted but did not
    // execute leaves no call to report and would otherwise be invisible.
    if let Some(usage) = reply.native_usage {
        let unexecuted = matches!(
            (usage.requested, usage.executed),
            (Some(requested), Some(executed)) if requested > executed
        );
        let fields = log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "provider" => &endpoint.provider,
            "model" => model_label,
            "modelCall" => model_calls,
            "reportedCalls" => reply.native_calls.len(),
            // CVL-FR-33: a provider that named no call had its searches
            // reported from this count instead, which is what a reader needs to
            // know before comparing the two numbers.
            "reportedFromCounts" => reply.native_calls.is_empty()
                && usage.web_searches.is_some_and(|n| n > 0),
            "toolCallsRequested" => usage.requested,
            "toolCallsExecuted" => usage.executed,
            "webSearchRequests" => usage.web_searches,
        };
        if unexecuted {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend, Domain::Remote],
                "provider did not execute every tool call it was asked for",
                fields,
            );
        } else {
            logging::log_debug(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend, Domain::Remote],
                "provider tool usage reported",
                fields,
            );
        }
    }
}
