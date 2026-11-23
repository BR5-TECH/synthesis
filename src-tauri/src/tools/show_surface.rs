//! `show_surface` — show the author a small interactive panel
//! (`SSF-show-surface-tool.md`).
//!
//! The agent composes the panel from the discussion catalog
//! (`AUI-agent-ui-surfaces.md`). The tool validates it, records it, and posts
//! it as one comment of the agent through `crate::agent_surfaces::store`
//! (SSF-FR-FCDS). The author presses a button, and the values come back as a
//! comment of the author.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal};

use crate::agent_conversations::ConversationOrigin;
use crate::agent_surfaces::store::{self, RecordError, SurfaceInput};
use crate::agent_surfaces::{catalog, validate::MAX_COMPONENTS};
use crate::comments::{self, Participant};
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};

#[cfg(test)]
mod tests;

/// SSF-FR-CETL.
pub const NAME: &str = "show_surface";

/// SSF-FR-FKWS: the fixed description of the contract surface. The catalog's
/// instructions follow it.
pub const DESCRIPTION: &str = "Show the author a small interactive panel in the conversation: a table that compares options, a checklist, a short form, or a set of choices. Use it when the author must see structured information side by side, or must give you several values at once, and plain text or one question is not enough. Before you compose a panel, load the `a2ui` skill with `load_skill`: it has the rules, the cards, and examples. For a common case use one card: `ChoiceCard`, `ConfirmCard`, `ComparisonCard`, or `ChecklistCard`, as the only component with the id `root`. Build the panel in `components` as a flat list from the components of the catalog below. Each entry has a unique `id` and a `component` type, and a container names its children by id. Exactly one entry has the id `root`. Bind a value to the data model with `{\"@path\": \"/name\"}`, and give the start values in `data_model`. An input needs a Button. A Button has `action.event.name` and a `context` that names the values it sends back, usually with `@path`. When the author presses a button, the panel locks, and the button and its values come back to you in the conversation as the author's comment. Tell the author in `message`, in a sentence or two, what the panel is for. Put one `<spoken>…</spoken>` tag in `message`: one short sentence that a voice can read aloud. A message with no tag is refused. Your turn ends when the panel is posted. To ask one question, use `ask_user_comment`. To propose a change to a draft, a prompt, a change, or a plan, use its proposal tool.";

const MESSAGE_DESCRIPTION: &str = "What the panel is for, in a sentence or two. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud.";
const COMPONENTS_DESCRIPTION: &str = "The panel as a flat list of components of the catalog. Each has a unique `id` and a `component` type, and exactly one has the id `root`. A container names its children by id.";
const DATA_MODEL_DESCRIPTION: &str = "A JSON object with the start values that the components bind to with `@path`. Leave it out when nothing binds.";

// SSF contract surface, "Refusals".
pub const BLANK_MESSAGE: &str = "Tell the author in `message` what the panel is for.";
pub const NO_SPOKEN_PART: &str = "The message has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `message`.";
pub const CONVERSATION_RESOLVED: &str = "This conversation is resolved and takes no further comments, so nothing was shown. Answer with what you already have; asking again will not succeed.";
pub const NOT_RECORDED: &str = "The panel could not be recorded.";

/// SSF-FR-YIXE: what a second panel in one reply is told.
pub const ONE_AT_A_TIME: &str = "Only one panel can be shown in a reply, so this one was not recorded. Show the next panel after the author answered this one.";

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07).
#[derive(Debug, Clone, Deserialize)]
pub struct ShowSurfaceArgs {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub components: Vec<Value>,
    #[serde(default)]
    pub data_model: Option<Value>,
}

/// SSF-FR-BWTD: the data object of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShowSurfaceOutput {
    pub shown: bool,
}

/// SSF-FR-FKWS: the description, the compact catalog, and the catalog's
/// instructions.
pub fn description() -> String {
    format!(
        "{DESCRIPTION}\n\nThe catalog (`!` marks a required property):\n{}\n\n{}",
        compact_catalog(),
        catalog().instructions()
    )
}

/// SSF-FR-XKQA: one line for each component with its properties, and one
/// line for each function with its arguments, generated from the catalog.
pub fn compact_catalog() -> String {
    let catalog = catalog();
    let mut lines = Vec::new();
    for name in catalog.component_names() {
        if let Some(schema) = catalog.component(name) {
            lines.push(format!("{name}: {}", fields_of(schema)));
        }
    }
    let functions = catalog.document().get("functions").and_then(Value::as_object);
    for (name, schema) in functions.into_iter().flatten() {
        let args = schema.get("properties").and_then(|p| p.get("args")).unwrap_or(&Value::Null);
        lines.push(format!("{name}({})", fields_of(args)));
    }
    lines.join("\n")
}

/// The property names of `schema`, each required one marked with `!`.
fn fields_of(schema: &Value) -> String {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let names =
        schema.get("properties").and_then(Value::as_object).into_iter().flat_map(|p| p.keys());
    names
        .map(|k| if required.contains(&k.as_str()) { format!("{k}!") } else { k.clone() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// SSF-FR-FKWS: the small JSON Schema of the contract surface. Each component
/// names its id and its type; the backend checks the rest (AUI-FR-EGAB).
pub fn parameters() -> Value {
    let names = catalog().component_names();
    json!({
        "type": "object",
        "properties": {
            "message": { "type": "string", "description": MESSAGE_DESCRIPTION },
            "components": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_COMPONENTS,
                "description": COMPONENTS_DESCRIPTION,
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "component": { "type": "string", "enum": names },
                    },
                    "required": ["id", "component"],
                },
            },
            "data_model": { "type": "object", "description": DATA_MODEL_DESCRIPTION },
        },
        "required": ["message", "components"],
    })
}

/// The stable code of a refusal, for the log (SSF-FR-BVSL).
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    match refusal {
        ToolRefusal::InvalidArguments(BLANK_MESSAGE) => Some("blank_message"),
        ToolRefusal::InvalidArguments(NO_SPOKEN_PART) => Some("no_spoken_part"),
        ToolRefusal::InvalidSurface { .. } => Some("invalid_surface"),
        _ => None,
    }
}

/// SSF-FR-CETL: `show_surface` as a `rig` portable tool. `asked` is how the
/// loop learns that a panel went out and the turn ends (CVL-FR-15).
pub struct ShowSurfaceTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    asked: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ShowSurfaceTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// SSF-FR-ZIRH: bound to the turn's conversation and agent.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        asked: Arc<AtomicBool>,
    ) -> Self {
        ShowSurfaceTool { app, roots, origin, author, asked, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// SSF-FR-BVSL / SSF-FR-JPNA: the call with its logging. The fields are
    /// ids, counts, and codes, and never a component, a text, or the message.
    fn run(&self, args: ShowSurfaceArgs) -> Result<ShowSurfaceOutput, ToolRefusal> {
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "components" => args.components.len(),
        };
        let outcome = self.show(&args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".into(), json!(code));
                }
                if let ToolRefusal::InvalidSurface { rule, .. } = refusal {
                    fields.insert("rule".into(), json!(rule));
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn show(&self, args: &ShowSurfaceArgs) -> Result<ShowSurfaceOutput, ToolRefusal> {
        require_open_project(&self.app)?;
        // SSF-FR-EFSU.
        if args.message.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(BLANK_MESSAGE));
        }
        if comments::spoken_part(&args.message).is_none() {
            return Err(ToolRefusal::InvalidArguments(NO_SPOKEN_PART));
        }
        // SSF-FR-CVBB: an absent data model is `{}`.
        let data_model = args.data_model.clone().unwrap_or_else(|| Value::Object(Map::new()));
        // SSF-FR-FCDS: the loop has already removed the citation markers from
        // every string argument (CVL-FR-VSSU).
        let input = SurfaceInput {
            components: &args.components,
            data_model: &data_model,
            message: &args.message,
            agent: &self.author,
            origin: &self.origin,
        };
        store::record_with(&self.app, self.buffer, &self.roots.store, input).map_err(|error| {
            match error {
                // SSF-FR-CVBB.
                RecordError::Invalid(violation) => ToolRefusal::InvalidSurface {
                    rule: violation.rule,
                    component_id: violation.component_id,
                    property: violation.property,
                    expected: violation.expected,
                },
                // SSF-FR-NHDH.
                RecordError::DiscussionResolved => ToolRefusal::Denied {
                    message: CONVERSATION_RESOLVED,
                    code: "conversation_resolved",
                    retryable: false,
                },
                // SSF-FR-WJHF.
                RecordError::WriteFailed => ToolRefusal::NotRecordedWith(NOT_RECORDED),
            }
        })?;
        // CVL-FR-15: set only after the record and its comment both landed.
        self.asked.store(true, Ordering::SeqCst);
        Ok(ShowSurfaceOutput { shown: true })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ShowSurfaceTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = ShowSurfaceArgs;
    type Output = ShowSurfaceOutput;
    type Error = ToolRefusal;

    fn description(&self) -> String {
        description()
    }

    fn parameters(&self) -> Value {
        parameters()
    }

    fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
        error.to_execution_error()
    }

    fn call(
        &self,
        arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        // SSF-FR-BWTD: it records and returns; nothing waits.
        std::future::ready(self.run(arguments))
    }
}
