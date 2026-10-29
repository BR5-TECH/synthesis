//! Measure document tool (`MDT-measure-document-tool.md`).
//!
//! The tool an agent reaches for before it decides to read a whole document.
//! It returns the size of a project file or of a draft's prompt — lines,
//! characters, and an estimate of tokens — with the size limit for documents in
//! this conversation and whether the document is above it. It returns no text.
//!
//! The estimate is the rule the conversation loop uses for its own budget
//! (CVL-FR-PGII), so the size an agent reads here and the size the discussion
//! surface shows the author agree.

use serde::{Deserialize, Serialize};

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_project_root, ToolRefusal,
    MEASURE_NOT_ONE_TARGET,
};
use crate::agent_conversations::{above_limit, DocumentBudget, DocumentSize};
use crate::fs::FsError;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (MDT contract surface)
// ---------------------------------------------------------------------------

/// MDT-FR-DRFB.
pub const NAME: &str = "measure_document";

/// MDT-FR-YFZQ: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Measure the size of a project file or of a draft's prompt without reading it. Use this before you decide to read a whole document: it tells you how many lines and characters the document holds, an estimate of how many tokens it takes, and whether it is above the size limit for documents in this conversation. Pass `path` for a project file or `draft_id` for a draft, not both. The token count is an estimate, not an exact count. When the document is above the limit, do not read it whole: read only the parts you need, with `offset` and `limit`. It returns no text of the document and changes nothing.";

const PATH_DESCRIPTION: &str = "The project-relative path of a file to measure, exactly as it was reported to you. Leave it out when you pass `draft_id`.";

const DRAFT_ID_DESCRIPTION: &str =
    "The ID of a draft to measure; its current prompt is measured. Leave it out when you pass `path`.";

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07), and a blank value counts as absent.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MeasureDocumentArgs {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, alias = "draftId")]
    pub draft_id: Option<String>,
}

/// TLC-FR-06: the JSON Schema for [`MeasureDocumentArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": PATH_DESCRIPTION },
            "draft_id": { "type": "string", "description": DRAFT_ID_DESCRIPTION },
        },
        // MDT-FR-OTUZ: exactly one is needed, which no schema keyword states,
        // so neither is listed as required.
        "required": [],
    })
}

/// MDT-FR-PPCH: the five fields and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeasureDocumentOutput {
    pub lines: u64,
    pub characters: u64,
    /// MDT-FR-QYLP: an estimate, never a provider-reported value.
    pub estimated_tokens: u64,
    /// MDT-FR-CTKN, MDT-FR-ADEI: null where no window is known.
    pub limit_tokens: Option<u64>,
    /// MDT-FR-FHWP.
    pub above_limit: bool,
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// The one target a call names (MDT-FR-OTUZ).
enum Target<'a> {
    Path(&'a str),
    Draft(&'a str),
}

fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn target_of(args: &MeasureDocumentArgs) -> Result<Target<'_>, ToolRefusal> {
    match (present(&args.path), present(&args.draft_id)) {
        (Some(path), None) => Ok(Target::Path(path)),
        (None, Some(id)) => Ok(Target::Draft(id)),
        _ => Err(ToolRefusal::InvalidArguments(MEASURE_NOT_ONE_TARGET)),
    }
}

/// MDT-FR-TWVZ, MDT-FR-VHHI: the read failures in this tool's terms. A path
/// that names nothing, or names somewhere this tool does not reach, is one
/// more path to correct.
fn file_refusal(error: FsError) -> ToolRefusal {
    match error {
        FsError::Utf8 { .. } | FsError::Io(_) => ToolRefusal::MeasureUnreadable,
        _ => ToolRefusal::MeasureFileNotFound,
    }
}

/// MDT-FR-TWVZ: the text of a project file.
fn file_text(root: &crate::fs::RootFs, path: &str) -> Result<String, ToolRefusal> {
    let absolute = super::file_read::resolve(root.path(), path)
        .map_err(|_| ToolRefusal::MeasureFileNotFound)?;
    if root.file_info(&absolute).map_err(file_refusal)?.kind == crate::fs::EntryKind::Dir {
        return Err(ToolRefusal::MeasureFileNotFound);
    }
    root.read_text(&absolute).map_err(file_refusal)
}

/// MDT-FR-TVWI: the live prompt of a draft, on the terms of `read_draft`.
fn draft_text(root: &crate::fs::RootFs, draft_id: &str) -> Result<String, ToolRefusal> {
    crate::drafts::read_draft_prompt(root, draft_id).map(|d| d.content).map_err(|error| match error
        .as_str()
    {
        crate::drafts::ERR_NOT_SINGLE_FILE => ToolRefusal::MeasureDraftInconsistent,
        crate::drafts::ERR_PROMPT_UNREADABLE => ToolRefusal::MeasureUnreadable,
        _ => ToolRefusal::DraftNotFound,
    })
}

/// MDT-FR-LWAK … MDT-FR-ADEI: measure the one target against `budget`.
pub fn measure(
    root: &crate::fs::RootFs,
    args: &MeasureDocumentArgs,
    budget: &DocumentBudget,
) -> Result<MeasureDocumentOutput, ToolRefusal> {
    let text = match target_of(args)? {
        Target::Path(path) => file_text(root, path)?,
        Target::Draft(id) => draft_text(root, id)?,
    };
    let size = DocumentSize::of(&text);
    let limit_tokens = budget.limit_tokens();
    Ok(MeasureDocumentOutput {
        lines: size.lines,
        characters: size.characters,
        estimated_tokens: size.estimated_tokens,
        limit_tokens,
        above_limit: above_limit(Some(&size), limit_tokens),
    })
}

// ---------------------------------------------------------------------------
// Logging (MDT-FR-DAVA)
// ---------------------------------------------------------------------------

/// MDT-FR-DAVA: how long a target argument may be before a record carries only
/// its head.
const LOGGED_TARGET_LIMIT: usize = 512;

/// MDT-FR-DAVA: the target argument the model composed, bounded. Never any
/// text of the document.
fn logged_target(args: &MeasureDocumentArgs) -> crate::logging::Fields {
    let bound = |value: &Option<String>| {
        value.as_deref().map(|v| bounded_argument(v, LOGGED_TARGET_LIMIT)).unwrap_or_default()
    };
    log_fields! { "path" => bound(&args.path), "draft_id" => bound(&args.draft_id) }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// MDT-FR-DRFB, MDT-FR-EEFA: `measure_document` as a `rig` portable tool,
/// constructed per turn with that turn's window and percent. No argument names
/// either, so a model cannot state a larger limit for itself.
pub struct MeasureDocumentTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    budget: DocumentBudget,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> MeasureDocumentTool<R> {
    /// TLC-FR-15, TLC-FR-19: this tool's own constructor.
    pub fn new(
        app: tauri::AppHandle<R>,
        session: impl Into<String>,
        budget: DocumentBudget,
    ) -> Self {
        MeasureDocumentTool { app, session: session.into(), budget, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: impl Into<String>,
        budget: DocumentBudget,
        buffer: &'static LogBuffer,
    ) -> Self {
        MeasureDocumentTool { app, session: session.into(), budget, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14, MDT-FR-DAVA).
    fn run(&self, args: MeasureDocumentArgs) -> Result<MeasureDocumentOutput, ToolRefusal> {
        // MDT-FR-OPKM: no project, nothing to measure.
        let outcome = require_project_root(&self.app, &self.session)
            .and_then(|root| measure(&root, &args, &self.budget));
        match &outcome {
            Ok(output) => {
                let mut fields = logged_target(&args);
                fields.extend(log_fields! { "estimated_tokens" => output.estimated_tokens });
                log_tool_success(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => {
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, logged_target(&args))
            }
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for MeasureDocumentTool<R> {
    const NAME: &'static str = NAME;

    type Args = MeasureDocumentArgs;
    type Output = MeasureDocumentOutput;
    type Error = ToolRefusal;

    fn description(&self) -> String {
        DESCRIPTION.to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        parameters()
    }

    fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
        error.to_execution_error()
    }

    fn call(
        &self,
        arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        // MDT-FR-CGXO: answers from disk as it stands and waits on nothing.
        std::future::ready(self.run(arguments))
    }
}
