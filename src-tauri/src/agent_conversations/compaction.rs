//! The token budget of a turn and the compaction of its older history
//! (CVL-FR-LCXM .. CVL-FR-CSLX).
//!
//! A turn whose endpoint has a context size has a budget. Before each
//! invocation the loop estimates the input of the call, and when the estimate
//! is above three quarters of the budget, one extra model call summarises the
//! older history. The summary then stands where that history stood. A turn
//! with no budget compacts nothing.

use super::*;
use rig::completion::message::{AssistantContent, ToolResultContent, UserContent};
use rig::completion::Message;

/// CVL-FR-02 / CVL-FR-ZLSM: the instruction of the compaction call.
pub const COMPACT_PROMPT_TEMPLATE: &str = include_str!("../../../resources/prompts/compact.md");

/// CVL-FR-PGII: the estimate counts one token for each four characters.
pub(super) const CHARS_PER_TOKEN: u64 = 4;
/// CVL-FR-PGII: the estimate counts this many tokens for each image.
pub(super) const IMAGE_TOKENS: u64 = 1_000;

/// The first line of a section that holds a summary, so the model reads it as
/// a summary and not as comments.
pub(super) const SUMMARY_LEAD: &str =
    "Summary of the earlier discussion and of the material gathered earlier in this turn:";

/// Characters and images of one message, for the estimate.
fn message_size(message: &Message) -> (u64, u64) {
    let mut chars = 0u64;
    let mut images = 0u64;
    let count = |text: &str| text.chars().count() as u64;
    match message {
        Message::System { content } => chars += count(content),
        Message::User { content } => {
            for part in content {
                match part {
                    UserContent::Text(text) => chars += count(&text.text),
                    UserContent::Image(_) => images += 1,
                    UserContent::ToolResult(result) => {
                        for item in &result.content {
                            match item {
                                ToolResultContent::Text(text) => chars += count(&text.text),
                                ToolResultContent::Image(_) => images += 1,
                                ToolResultContent::Json { value } => {
                                    chars += count(&value.to_string())
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Message::Assistant(assistant) => {
            for part in &assistant.content {
                match part {
                    AssistantContent::Text(text) => chars += count(&text.text),
                    AssistantContent::ToolCall(call) => {
                        chars += count(call.function.name.as_str());
                        chars += count(
                            &serde_json::Value::Object(call.function.arguments.clone()).to_string(),
                        );
                    }
                    AssistantContent::Image(_) => images += 1,
                    _ => {}
                }
            }
        }
    }
    (chars, images)
}

/// CVL-FR-PGII: the estimated input of one call — one token for each four
/// characters of the instructions, of the tool definitions, and of the text of
/// every message, plus `IMAGE_TOKENS` for each image. Not a usage value.
pub(super) fn estimate_input_tokens(request: &AgentRequest, exchange: &[Message]) -> u64 {
    let mut chars = request.instructions.chars().count() as u64;
    for tool in &request.tools {
        chars += serde_json::to_string(tool).map(|t| t.chars().count() as u64).unwrap_or(0);
    }
    let mut images = 0u64;
    for message in exchange {
        let (c, i) = message_size(message);
        chars += c;
        images += i;
    }
    chars.div_ceil(CHARS_PER_TOKEN) + images * IMAGE_TOKENS
}

/// CVL-FR-EBJG: whether an estimate is above three quarters of the budget.
pub(super) fn over_budget(estimate: u64, budget: u64) -> bool {
    estimate.saturating_mul(4) > budget.saturating_mul(3)
}

/// CVL-FR-XCOY: where the latest round begins — the last reply that asked for
/// tools. The whole exchange after the input is older history when there is
/// no such reply.
pub(super) fn latest_round_start(exchange: &[Message]) -> usize {
    exchange
        .iter()
        .rposition(|message| {
            matches!(message, Message::Assistant(a)
                if a.content.iter().any(|c| matches!(c, AssistantContent::ToolCall(_))))
        })
        .unwrap_or(exchange.len())
        .max(1)
        .min(exchange.len())
}

/// The input's `discussion_history` section, where it holds any text.
fn discussion_history(request: &AgentRequest) -> Option<&InputSection> {
    request.input.iter().find(|s| s.tag == TAG_DISCUSSION_HISTORY && !s.body.trim().is_empty())
}

/// CVL-FR-XCOY: whether a turn has older history to compact.
pub(super) fn has_older_history(request: &AgentRequest, exchange: &[Message]) -> bool {
    discussion_history(request).is_some() || latest_round_start(exchange) > 1
}

/// CVL-FR-ZLSM: the text of a section, with each image replaced by its
/// context text.
fn section_text(section: &InputSection) -> String {
    if section.parts.is_empty() {
        return section.body.clone();
    }
    section
        .parts
        .iter()
        .map(|part| match part {
            InputPart::Text(text) => text.clone(),
            InputPart::Image(image) => format!("[image: {}]", image.context),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// CVL-FR-ZLSM: one message of the exchange as text.
fn message_text(message: &Message) -> String {
    let mut lines = Vec::new();
    match message {
        Message::System { content } => lines.push(content.clone()),
        Message::User { content } => {
            for part in content {
                match part {
                    UserContent::Text(text) => lines.push(text.text.clone()),
                    UserContent::ToolResult(result) => {
                        let body = result
                            .content
                            .iter()
                            .map(|item| match item {
                                ToolResultContent::Text(text) => text.text.clone(),
                                ToolResultContent::Image(_) => "[image]".to_string(),
                                ToolResultContent::Json { value } => value.to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        lines.push(format!("[result of {}]\n{body}", result.name));
                    }
                    UserContent::Image(_) => lines.push("[image]".to_string()),
                    _ => {}
                }
            }
        }
        Message::Assistant(assistant) => {
            for part in &assistant.content {
                match part {
                    AssistantContent::Text(text) => lines.push(format!("[agent]\n{}", text.text)),
                    AssistantContent::ToolCall(call) => lines.push(format!(
                        "[agent called {}] {}",
                        call.function.name,
                        serde_json::Value::Object(call.function.arguments.clone())
                    )),
                    _ => {}
                }
            }
        }
    }
    lines.join("\n")
}

/// CVL-FR-ZLSM: the older history as the one user message of the compaction
/// call.
pub(super) fn older_history_text(request: &AgentRequest, exchange: &[Message]) -> String {
    let mut out = String::new();
    if let Some(section) = discussion_history(request) {
        out.push_str("<discussion_history>\n");
        out.push_str(&section_text(section));
        out.push_str("\n</discussion_history>\n\n");
    }
    let end = latest_round_start(exchange);
    if end > 1 {
        out.push_str("<earlier_in_this_turn>\n");
        for message in &exchange[1..end] {
            out.push_str(&message_text(message));
            out.push_str("\n\n");
        }
        out.push_str("</earlier_in_this_turn>");
    }
    out.trim_end().to_string()
}

/// CVL-FR-ZLSM: the request of the compaction call — its own instruction, no
/// tool, and no input of its own.
pub(super) fn compaction_request() -> AgentRequest {
    AgentRequest {
        instructions: crate::prompts::instruction_text(COMPACT_PROMPT_TEMPLATE),
        ..Default::default()
    }
}

/// CVL-FR-BSMY: the input with its `discussion_history` body replaced by one
/// text part that holds the summary. An input with no such section receives
/// one before its current comment.
pub(super) fn compacted_request(request: &AgentRequest, summary: &str) -> AgentRequest {
    let mut compacted = request.clone();
    let body = format!("{SUMMARY_LEAD}\n\n{}", summary.trim());
    let section = InputSection {
        tag: TAG_DISCUSSION_HISTORY.to_string(),
        attributes: Vec::new(),
        body,
        truncated: false,
        parts: Vec::new(),
    };
    match compacted.input.iter().position(|s| s.tag == TAG_DISCUSSION_HISTORY) {
        Some(index) => compacted.input[index] = section,
        None => {
            let at = compacted
                .input
                .iter()
                .position(|s| s.tag == TAG_CURRENT_COMMENT)
                .unwrap_or(compacted.input.len());
            compacted.input.insert(at, section);
        }
    }
    compacted.stable_head_sections = rig_messages::stable_head_of(&compacted.input);
    compacted
}

/// CVL-FR-BSMY: the compacted exchange — the new input message, then the
/// latest round unchanged. Every older message is removed.
pub(super) fn compacted_exchange(
    compacted: &AgentRequest,
    with_images: bool,
    exchange: &[Message],
) -> Vec<Message> {
    let mut out = rig_messages::opening_exchange(compacted, with_images);
    out.extend_from_slice(&exchange[latest_round_start(exchange)..]);
    out
}

/// What became of one compaction, as its record names it (CVL-FR-CSLX).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Compacted,
    NothingOlder,
    Failed,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Compacted => "compacted",
            Outcome::NothingOlder => "nothing_older",
            Outcome::Failed => "failed",
        }
    }
}

/// The state compaction changes: the input a call carries and the exchange.
pub(super) struct Conversation {
    /// `None` until a compaction replaced the plan's own input.
    pub(super) compacted: Option<AgentRequest>,
    pub(super) exchange: Vec<Message>,
    pub(super) with_images: bool,
}

impl Conversation {
    /// The request a call carries now.
    pub(super) fn request<'a>(&'a self, plan: &'a TurnPlan) -> &'a AgentRequest {
        self.compacted.as_ref().unwrap_or(&plan.request)
    }
}

/// CVL-FR-LCXM .. CVL-FR-HFRL: compact the older history before invocation
/// `model_calls` where the turn has a budget and the estimate is over it.
///
/// A compaction call that fails or answers with no text leaves the exchange as
/// it was. Cancellation, the whole-turn deadline, and a locked conversation
/// end the turn as they do for any call.
///
/// `call` is the invocation that compaction comes before; the compaction call
/// takes its retry budget, its deadline, and its ordinal.
pub(super) fn compact_if_over_budget<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    conversation: &mut Conversation,
    call: ModelCall,
) -> Result<(), TurnEnd> {
    let Some(budget) = endpoint.context_size else {
        return Ok(());
    };
    let request = conversation.request(plan);
    let estimate = estimate_input_tokens(request, &conversation.exchange);
    if !over_budget(estimate, budget) {
        return Ok(());
    }
    let older_messages = latest_round_start(&conversation.exchange).saturating_sub(1);
    if !has_older_history(request, &conversation.exchange) {
        log_compaction(app, turns, plan, endpoint, estimate, budget, 0, Outcome::NothingOlder);
        return Ok(());
    }
    let compaction = compaction_request();
    let older = vec![Message::user(older_history_text(request, &conversation.exchange))];
    let result = complete_with_retries(
        app,
        turns,
        plan,
        endpoint,
        &compaction,
        &older,
        ModelCall { at_bound: true, compaction: true, ..call },
    );
    let compacted_messages = older_messages + usize::from(discussion_history(request).is_some());
    match result {
        Ok(reply) if !reply.text.trim().is_empty() => {
            let compacted = compacted_request(request, &reply.text);
            conversation.exchange =
                compacted_exchange(&compacted, conversation.with_images, &conversation.exchange);
            conversation.compacted = Some(compacted);
            log_compaction(
                app,
                turns,
                plan,
                endpoint,
                estimate,
                budget,
                compacted_messages,
                Outcome::Compacted,
            );
            Ok(())
        }
        Err(end @ (TurnEnd::Cancelled | TurnEnd::TimedOut)) => Err(end),
        Err(TurnEnd::Failed(FAIL_THREAD_LOCKED)) => Err(TurnEnd::Failed(FAIL_THREAD_LOCKED)),
        _ => {
            log_compaction(app, turns, plan, endpoint, estimate, budget, 0, Outcome::Failed);
            Ok(())
        }
    }
}

/// CVL-FR-CSLX: one record per compaction, with the correlation fields of
/// CVL-FR-28. Counts and an outcome, and no part of the history or the
/// summary.
#[allow(clippy::too_many_arguments)] // Reason: each value is a field of the one record.
fn log_compaction<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    estimate: u64,
    budget: u64,
    compacted_messages: usize,
    outcome: Outcome,
) {
    let level = match outcome {
        Outcome::Compacted => logging::LogLevel::Info,
        Outcome::NothingOlder | Outcome::Failed => logging::LogLevel::Warn,
    };
    logging::log(
        app,
        turns.buffer(),
        level,
        &[Domain::Ai, Domain::Backend],
        "conversation history compaction",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "provider" => &endpoint.provider,
            "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
            "estimatedTokens" => estimate,
            "budgetTokens" => budget,
            "compactedMessages" => compacted_messages,
            "outcome" => outcome.as_str(),
        },
    );
}
