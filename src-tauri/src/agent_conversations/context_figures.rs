//! The context figures of a turn and the size of the document under discussion
//! (AGC-FR-GMSF, AGC-FR-OYVH, AGC-FR-NJXW, AGC-FR-BCHZ, CVL-FR-OPTI,
//! CVL-FR-AAPA).
//!
//! Every token value here except `input_tokens` is an estimate by the fixed rule
//! of CVL-FR-PGII. None of them is a usage value (CVL-FR-HLVG), and nothing here
//! is written anywhere: the figures live on the in-memory turn (AGC-FR-NUKP).

use super::*;
use rig::completion::message::{ToolResultContent, UserContent};
use rig::completion::Message;

use super::compaction::{CHARS_PER_TOKEN, IMAGE_TOKENS};

/// AGC-FR-OYVH, CVL-FR-PGII: the estimated tokens of `chars` characters — one
/// token for each four characters, rounded up.
pub fn estimate_tokens(chars: u64) -> u64 {
    chars.div_ceil(CHARS_PER_TOKEN)
}

/// AGC-FR-OYVH: the size of a whole document.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSize {
    /// A line ends at a line feed, a last line without one still counts, and an
    /// empty text has no line (MDT-FR-LWAK).
    pub lines: u64,
    /// Unicode scalar values, with no line-ending normalisation.
    pub characters: u64,
    /// An estimate: `characters` divided by four, rounded up.
    pub estimated_tokens: u64,
}

impl DocumentSize {
    /// The size of `text`, counted on the whole text as read.
    pub fn of(text: &str) -> Self {
        let characters = text.chars().count() as u64;
        DocumentSize {
            lines: text.split_inclusive('\n').count() as u64,
            characters,
            estimated_tokens: estimate_tokens(characters),
        }
    }
}

/// CVL-FR-AAPA: the estimated input of one call, in five parts.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSplit {
    /// The compiled prompt and the tool definitions.
    pub instructions: u64,
    /// The material section as sent.
    pub document: u64,
    /// The `discussion_history` section.
    pub history: u64,
    /// The `current_comment` section.
    pub current_comment: u64,
    /// Every tool result the exchange holds.
    pub tool_results: u64,
}

impl ContextSplit {
    /// AGC-FR-OBUA: the sum of the five parts.
    pub fn total(&self) -> u64 {
        self.instructions + self.document + self.history + self.current_comment + self.tool_results
    }
}

/// AGC-FR-NJXW: the context figures of a turn's latest answered model call.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TurnContext {
    /// The effective context size of the turn's model (AAP-FR-RANS). Never
    /// absent.
    pub window: u64,
    /// The input total the provider reported for the latest call.
    pub input_tokens: Option<u64>,
    pub split: ContextSplit,
    /// Null on a note target.
    pub document: Option<DocumentSize>,
    /// The document size limit (AGC-FR-GMSF). Never absent.
    pub limit_tokens: u64,
    /// AGC-FR-BCHZ.
    pub above_limit: bool,
}

/// AGC-FR-JFBZ: the same figures before any turn.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionContext {
    pub window: u64,
    /// The sum of the split (AGC-FR-OBUA).
    pub estimated_tokens: u64,
    pub split: ContextSplit,
    pub document: Option<DocumentSize>,
    pub limit_tokens: u64,
    pub above_limit: bool,
}

/// AGC-FR-GMSF: the window of the turn's model and the Document size limit
/// percent, resolved at dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentBudget {
    pub window: u64,
    pub percent: u8,
}

impl DocumentBudget {
    /// AGC-FR-OBUA: the budget of the default context size, for an estimate
    /// with no agent or no provider (AAP-FR-RANS).
    pub fn default_window() -> Self {
        DocumentBudget {
            window: ai_api::DEFAULT_CONTEXT_SIZE,
            percent: crate::global_settings::DOCUMENT_LIMIT_PERCENT_DEFAULT,
        }
    }

    /// AGC-FR-GMSF: the window and the stored percent of this provider's model.
    pub fn resolve(store: &GlobalSettingsStore, provider: &str, model_id: &str) -> Self {
        DocumentBudget {
            window: ai_api::resolve_context_window(store, provider, model_id),
            percent: store
                .load_document_limit_percent()
                .unwrap_or(crate::global_settings::DOCUMENT_LIMIT_PERCENT_DEFAULT),
        }
    }

    /// AGC-FR-GMSF: the whole part of `window × percent / 100`.
    pub fn limit_tokens(&self) -> u64 {
        self.window.saturating_mul(u64::from(self.percent)) / 100
    }

    /// AGC-FR-XHHT: the bound of the `artifact` section in characters.
    pub fn bound_chars(&self) -> usize {
        usize::try_from(self.limit_tokens().saturating_mul(CHARS_PER_TOKEN)).unwrap_or(usize::MAX)
    }
}

/// AGC-FR-BCHZ: true only where the document's estimate is above the limit.
pub fn above_limit(document: Option<&DocumentSize>, limit_tokens: u64) -> bool {
    document.is_some_and(|document| document.estimated_tokens > limit_tokens)
}

/// The estimated tokens of one section as sent: its text, and each image it
/// carries (CVL-FR-PGII).
fn section_tokens(section: &InputSection) -> u64 {
    let images = section.parts.iter().filter(|p| matches!(p, InputPart::Image(_))).count() as u64;
    estimate_tokens(section.body.chars().count() as u64) + images * IMAGE_TOKENS
}

/// CVL-FR-AAPA: the estimated tokens of the instructions and the tool
/// definitions.
pub(super) fn instruction_tokens(
    instructions: &str,
    tools: &[rig::completion::ToolDefinition],
) -> u64 {
    let mut chars = instructions.chars().count() as u64;
    for tool in tools {
        chars += serde_json::to_string(tool).map(|t| t.chars().count() as u64).unwrap_or(0);
    }
    estimate_tokens(chars)
}

/// CVL-FR-AAPA: the four parts of the split that the input sections give.
/// `tool_results` is left at 0.
pub(super) fn split_of_sections(instructions: u64, sections: &[InputSection]) -> ContextSplit {
    let mut split = ContextSplit { instructions, ..ContextSplit::default() };
    for section in sections {
        let tokens = section_tokens(section);
        match section.tag.as_str() {
            TAG_ARTIFACT | TAG_NOTE_CONTEXT => split.document += tokens,
            TAG_DISCUSSION_HISTORY => split.history += tokens,
            TAG_CURRENT_COMMENT => split.current_comment += tokens,
            _ => split.history += tokens,
        }
    }
    split
}

/// CVL-FR-AAPA: the estimated tokens of every tool result the exchange holds.
pub(super) fn tool_result_tokens(exchange: &[Message]) -> u64 {
    let mut chars = 0u64;
    let mut images = 0u64;
    for message in exchange {
        let Message::User { content } = message else { continue };
        for part in content.iter() {
            let UserContent::ToolResult(result) = part else { continue };
            for item in result.content.iter() {
                match item {
                    ToolResultContent::Text(text) => chars += text.text.chars().count() as u64,
                    ToolResultContent::Image(_) => images += 1,
                    ToolResultContent::Json { value } => {
                        chars += value.to_string().chars().count() as u64
                    }
                }
            }
        }
    }
    estimate_tokens(chars) + images * IMAGE_TOKENS
}

/// CVL-FR-OPTI, CVL-FR-AAPA: the split of one answered call.
pub(super) fn split_of_call(request: &AgentRequest, exchange: &[Message]) -> ContextSplit {
    let instructions = instruction_tokens(&request.instructions, &request.tools);
    let mut split = split_of_sections(instructions, &request.input);
    split.tool_results = tool_result_tokens(exchange);
    split
}

/// CVL-FR-OPTI → AGC-FR-NJXW: the `report_context` seam. Completes the figures
/// of one answered call with what dispatch resolved, holds them on the turn,
/// and emits the turn. Writes nothing anywhere (AGC-FR-NUKP).
pub(super) fn report_context<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    plan: &TurnPlan,
    window: u64,
    input_tokens: Option<u64>,
    split: ContextSplit,
) {
    use tauri::Manager;
    let limit_tokens = plan.document_budget.limit_tokens();
    let context = TurnContext {
        window,
        input_tokens,
        split,
        document: plan.document,
        limit_tokens,
        above_limit: above_limit(plan.document.as_ref(), limit_tokens),
    };
    let turns = app.state::<TurnRegistry>();
    turns.hold_turn_context(app, &plan.turn_id, context);
}
