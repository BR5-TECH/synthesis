//! The bound on what one tool result brings into the exchange
//! (CVL-FR-XMGI, CVL-FR-ZJLK, CVL-FR-UOKM, CVL-FR-AXQN).
//!
//! A tool returns its whole result (TLC-FR-DUDT). The loop cuts what reaches
//! the model to the room that is still free in the window, so one large read
//! cannot fill the window by itself, and a turn with room can read a lot.

use super::compaction::CHARS_PER_TOKEN;
use super::*;

/// CVL-FR-XMGI: the share of the budget, in percent, of the minimum bound.
pub(super) const MINIMUM_BOUND_PERCENT: u64 = 2;

/// CVL-FR-XMGI: the minimum bound — the whole part of
/// `context_size × 2 / 100 × 4` characters.
pub(super) fn minimum_bound(context_size: u64) -> usize {
    let chars = context_size.saturating_mul(MINIMUM_BOUND_PERCENT * CHARS_PER_TOKEN) / 100;
    usize::try_from(chars).unwrap_or(usize::MAX)
}

/// CVL-FR-XMGI: the result bound — the free room before the compaction mark
/// of CVL-FR-EBJG, `(three quarters of context_size − estimate) × 4`
/// characters, and never below the minimum bound. `estimate` is the estimate
/// of CVL-FR-PGII of the next call as it stands.
pub(super) fn result_bound(context_size: u64, estimate: u64) -> usize {
    let mark = context_size.saturating_mul(3) / 4;
    let free = mark.saturating_sub(estimate).saturating_mul(CHARS_PER_TOKEN);
    usize::try_from(free).unwrap_or(usize::MAX).max(minimum_bound(context_size))
}

/// CVL-FR-ZJLK: the fixed line that ends a cut result of this tool.
pub(super) fn cut_line(tool: &str, kept: usize, whole: usize) -> String {
    let hint = if tool == crate::tools::file_read::NAME || tool == crate::tools::draft_read::NAME {
        " Read the rest in parts with offset and limit."
    } else if tool == crate::tools::document_get::NAME {
        " Read the rest in parts with line_offset and line_limit."
    } else {
        ""
    };
    format!("[The result was cut at {kept} of {whole} characters.{hint}]")
}

/// CVL-FR-XMGI, CVL-FR-ZJLK, CVL-FR-UOKM: the result as it reaches the model,
/// at most `bound` characters with its line, and the characters of the whole
/// and of the kept text where it was cut.
///
/// `recut` is set by the recovery alone (CVL-FR-QJLU): the text can then be a
/// result this module cut before, whose line names the size of the whole.
pub(super) fn cap_result(
    tool: &str,
    text: String,
    bound: usize,
    recut: bool,
) -> (String, Option<(usize, usize)>) {
    // CVL-FR-UOKM: half a procedure is worse than none (LSK-FR-14).
    if tool == crate::tools::skill_load::NAME {
        return (text, None);
    }
    // A result that was cut before keeps the size of the whole result in its
    // line; a second cut (CVL-FR-QJLU) cuts the kept text and names that size.
    let earlier = if recut { earlier_cut(&text) } else { None };
    let (body, whole) = match earlier {
        Some((body, whole)) => (body.to_string(), whole),
        None => {
            let whole = text.chars().count();
            (text, whole)
        }
    };
    if text_fits(&text_of(&body, whole, tool), bound) {
        return (text_of(&body, whole, tool), None);
    }
    // The line stands inside the bound, so a cut result never takes the next
    // call past the compaction mark of CVL-FR-EBJG by itself.
    let room = bound.saturating_sub(2 + cut_line(tool, bound, whole).chars().count());
    let kept: String = body.chars().take(room).collect();
    let line = cut_line(tool, room, whole);
    (format!("{kept}\n\n{line}"), Some((whole, room)))
}

/// The text of a result: the body alone where it is whole, else the body and
/// its line.
fn text_of(body: &str, whole: usize, tool: &str) -> String {
    let kept = body.chars().count();
    if kept == whole {
        body.to_string()
    } else {
        format!("{body}\n\n{}", cut_line(tool, kept, whole))
    }
}

fn text_fits(text: &str, bound: usize) -> bool {
    text.chars().count() <= bound
}

/// The kept text of a result that this module cut before, and the size of
/// the whole result its line names.
fn earlier_cut(text: &str) -> Option<(&str, usize)> {
    let at = text.rfind("\n\n[The result was cut at ")?;
    let line = &text[at + 2..];
    if !line.ends_with(']') || line.contains('\n') {
        return None;
    }
    let rest = line.strip_prefix("[The result was cut at ")?;
    let (_, after_of) = rest.split_once(" of ")?;
    let (whole, _) = after_of.split_once(" characters.")?;
    Some((&text[..at], whole.parse().ok()?))
}

/// CVL-FR-XMGI, CVL-FR-AXQN: append one tool result to the exchange, cut to the
/// free room the next call has as it stands, the results this round already
/// appended included. A cut emits one record of the sizes alone.
pub(super) fn push_tool_result<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    context_size: u64,
    conversation: &mut compaction::Conversation,
    call: &rig::completion::message::ToolCall,
    result: String,
) {
    let estimate =
        compaction::estimate_input_tokens(conversation.request(plan), &conversation.exchange);
    let bound = result_bound(context_size, estimate);
    let (text, cut) = cap_result(&call.function.name, result, bound, false);
    if let Some((whole, kept)) = cut {
        logging::log_debug(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent tool result cut to the result bound",
            log_fields! {
                "turnId" => &plan.turn_id,
                "agent" => &plan.agent.nickname,
                "agentId" => &plan.agent.id,
                "provider" => &plan.provider,
                "model" => &plan.agent.model_id,
                "tool" => &call.function.name,
                "resultChars" => whole,
                "keptChars" => kept,
            },
        );
    }
    // ERP-FR-NKLF: the stamp of a result that read one file.
    if let Some(stamp) = stamp_of(plan.roots.borrowed(), call) {
        conversation.stamps.push(stamp);
    }
    conversation.exchange.push(rig::completion::Message::tool_result(
        call.id.clone(),
        call.function.name.clone(),
        text,
    ));
}

#[cfg(test)]
mod tests;
