//! Recovery from a context overflow (CVL-FR-PFNV, CVL-FR-QJLU, CVL-FR-LQDM,
//! CVL-FR-LZDN).
//!
//! An undetected context window defaults to 1,000,000 tokens (AAP-FR-BKBF), so
//! a provider can refuse a call for its size before the estimate starts
//! compaction. The loop then compacts and cuts the latest results one time,
//! and makes the call again.

use super::*;
use rig::completion::message::{ToolResultContent, UserContent};
use rig::completion::Message;

/// CVL-FR-PFNV: the markers of a context overflow, matched without case in
/// the provider's own error code or message.
pub(super) const OVERFLOW_MARKERS: &[&str] = &[
    "context_length_exceeded",
    "prompt is too long",
    "maximum context length",
    "context window",
    "too many tokens",
    "input is too long",
    "request_too_large",
];

/// CVL-FR-PFNV: whether a failed attempt is a context overflow — a structured
/// provider error with status 400 or 413 whose code or message holds a marker.
pub(super) fn is_context_overflow(failure: &CallFailure) -> bool {
    if !matches!(failure.status, Some(400) | Some(413)) {
        return false;
    }
    [failure.provider_code.as_deref(), failure.provider_message.as_deref()]
        .into_iter()
        .flatten()
        .map(str::to_lowercase)
        .any(|text| OVERFLOW_MARKERS.iter().any(|marker| text.contains(marker)))
}

/// CVL-FR-QJLU: cut each tool result of the latest round, other than that of
/// `load_skill`, to `max_chars`. Returns how many results were cut.
#[cfg(test)]
pub(super) fn cut_latest_results(exchange: &mut [Message], max_chars: usize) -> usize {
    cut_latest_results_after(exchange, max_chars, 0)
}

/// [`cut_latest_results`] in an exchange whose input message stands at
/// `opening`. A replayed result is never cut (ERP-FR-WZAV).
pub(super) fn cut_latest_results_after(
    exchange: &mut [Message],
    max_chars: usize,
    opening: usize,
) -> usize {
    let start = compaction::latest_round_start_after(exchange, opening);
    let mut cut = 0;
    for message in exchange.iter_mut().skip(start) {
        let Message::User { content } = message else {
            continue;
        };
        for part in content.iter_mut() {
            let UserContent::ToolResult(result) = part else {
                continue;
            };
            let text: String = result
                .content
                .iter()
                .map(|item| match item {
                    ToolResultContent::Text(text) => text.text.clone(),
                    ToolResultContent::Json { value } => value.to_string(),
                    ToolResultContent::Image(_) => String::new(),
                })
                .collect();
            let (capped, was_cut) =
                result_cap::cap_result(result.name.as_str(), text, max_chars, true);
            if was_cut.is_some() {
                result.content = vec![ToolResultContent::text(capped)];
                cut += 1;
            }
        }
    }
    cut
}

/// CVL-FR-QJLU, CVL-FR-LQDM, CVL-FR-LZDN: recover from a context overflow.
/// Compacts the older history whatever the estimate, cuts the latest results
/// to the minimum bound, and emits one record. Returns whether the
/// exchange changed; where it did not, the loop makes no further call.
pub(super) fn recover<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    conversation: &mut compaction::Conversation,
    call: ModelCall,
) -> Result<bool, TurnEnd> {
    let compacted = compaction::compact(
        app,
        turns,
        plan,
        endpoint,
        conversation,
        ModelCall { recovery: true, ..call },
        true,
    )?;
    let opening = opening_of(conversation.request(plan));
    let cut = cut_latest_results_after(
        &mut conversation.exchange,
        result_cap::minimum_bound(endpoint.context_size),
        opening,
    );
    let changed = compacted || cut > 0;
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "agent turn recovering from a context overflow; set the provider's max context size to the real window of the model",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "provider" => &endpoint.provider,
            "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
            "budgetTokens" => endpoint.context_size,
            "compacted" => compacted,
            "resultsCut" => cut,
            "changed" => changed,
        },
    );
    Ok(changed)
}
