//! Read draft tool (`RDT-read-draft-tool.md`).
//!
//! The tool an agent reaches for once it knows which draft it wants and needs
//! the whole of what that draft currently says. It takes a draft id and returns
//! that draft's stable metadata together with the complete text of its current
//! live prompt.
//!
//! ## Why an id and not a path
//!
//! A draft's storage is private working material whose position on disk is a
//! fact about how drafts are filed rather than something a model should compose
//! — `.synthesis/drafts/<folder-path>/<draft-id>/` (DRS-FR-01), where the folder
//! path is a fact only the drafts walk holds (DRS-FR-36). Addressing a draft by
//! id is what keeps this tool from being a second `read_file` pointed at a
//! directory the model would have to guess its way into.
//!
//! ## Why the live prompt and nothing else
//!
//! The accepted history beside it records versions the author settled and moved
//! on from (`DHS-draft-history.md`), and an agent handed a superseded version
//! would be reasoning about a draft its author has already left behind. There is
//! no version selector, no entry id, and no path in this tool's schema, so no
//! argument a model composes reaches one (RDT-FR-04) — and the resolution it
//! delegates to reads `files/` alone (DRS-FR-38), so the exclusion holds in the
//! storage module rather than resting on this tool's restraint.
//!
//! ## Why the output is data rather than an unwrapped document
//!
//! TLC-FR-08 returns a document as text and wrapped in nothing, which is what
//! `read_file` does. This tool is the declared exception: a prompt without its
//! draft's name and status is material the model cannot place, and the metadata
//! without the prompt is a row it cannot act on, so returning the document alone
//! would cost a second call for facts this one already resolved (RDT-FR-05).

use serde::{Deserialize, Serialize};

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_project_root, ToolRefusal,
    DRAFT_ID_BLANK,
};
use crate::drafts::DraftStatus;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (RDT contract surface)
// ---------------------------------------------------------------------------

/// RDT-FR-01.
pub const NAME: &str = "read_draft";

/// RDT-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Read the complete current live prompt of a known draft. Use this after `search_drafts` gives you a draft ID, or when you already know that ID. It returns the draft's stable metadata together with its full current prompt; use the returned prompt path only as information, not as an argument to `read_file`. It never returns an accepted-history snapshot or another version, does not search for drafts, and changes nothing.";

const DRAFT_ID_DESCRIPTION: &str = "The draft ID returned by `search_drafts` or otherwise already known. This is not a file path, prompt path, history-entry ID, or version selector.";

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07), which is what
/// makes an `offset` or a `limit` a model brought over from `read_file` cost it
/// nothing: the parameter does not exist here, and the complete prompt comes
/// back regardless (RDT-FR-06).
#[derive(Debug, Clone, Deserialize)]
pub struct ReadDraftArgs {
    /// The `alias` is TLC-FR-07's forgiveness applied to a name rather than to a
    /// value: `draft_id` is the schema's spelling and the one the description
    /// uses, and a model that reached for the camelCase spelling anyway has made
    /// a mistake no answer of ours improves on by refusing. Without it serde
    /// fails the decode as a missing field, which never reaches this tool and so
    /// cannot even produce a clean `InvalidArgs`.
    #[serde(alias = "draftId")]
    pub draft_id: String,
}

/// TLC-FR-06: the JSON Schema for [`ReadDraftArgs`].
///
/// One parameter and no other: RDT-FR-04 turns on there being nothing here a
/// model could set to reach a version that is not the live one.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "draft_id": {
                "type": "string",
                "description": DRAFT_ID_DESCRIPTION,
            },
        },
        "required": ["draft_id"],
    })
}

/// RDT-FR-05, RDT-FR-07: the structured record, carrying exactly the documented
/// five fields.
///
/// The draft's `created_at`, `updated_at`, filed folder, build state,
/// pending-proposal flag, and graduation run are omitted: each is a fact about
/// how the author is managing the draft rather than about what it says, and a
/// model can act on none of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadDraftOutput {
    pub draft_id: String,
    pub name: String,
    pub status: DraftStatus,
    /// RDT-FR-09: draft-relative and informational, on the same terms
    /// `search_drafts` returns it. Not a path `read_file` resolves to this
    /// prompt.
    pub prompt_path: String,
    /// RDT-FR-06: the complete live prompt. No ceiling, no truncation, no
    /// elision, and no summary.
    pub content: String,
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// RDT-FR-03, RDT-FR-04: the draft resolved through the storage module at the
/// moment of the call, and its live prompt read from that draft's own `files/`.
///
/// Consults no index, so a successful read reflects the newest persisted live
/// prompt even while the drafts index is still catching up — the freshness
/// `search_drafts` does not share.
pub fn read(root: &crate::fs::RootFs, args: &ReadDraftArgs) -> Result<ReadDraftOutput, ToolRefusal> {
    // RDT-FR-11: a blank id names no draft and is the model's own to correct.
    // Checked before resolution so the refusal says which mistake was made
    // rather than reporting a draft that does not exist.
    if args.draft_id.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(DRAFT_ID_BLANK));
    }
    // Trimmed, because a model that padded an id meant the id (TLC-FR-07).
    let resolved = crate::drafts::read_draft_prompt(root, args.draft_id.trim())
        .map_err(|error| refusal_for(&error))?;
    Ok(ReadDraftOutput {
        draft_id: resolved.draft.id,
        name: resolved.draft.name,
        status: resolved.draft.status,
        prompt_path: resolved.prompt_path,
        content: resolved.content,
    })
}

/// The storage module's typed failures in this tool's terms (RDT-FR-12,
/// RDT-FR-13, RDT-FR-18).
///
/// Each is matched on the constant `crate::drafts` publishes for it rather than
/// on a substring of a message, so a reworded error cannot silently change which
/// refusal a model gets. What remains — an id naming no draft, an id that is not
/// well-formed, a record that cannot be read — is the same thing from the
/// model's side: no draft it can reach answers to what it sent, and a different
/// id is what would help.
fn refusal_for(error: &str) -> ToolRefusal {
    match error {
        crate::drafts::ERR_NOT_SINGLE_FILE => ToolRefusal::DraftInconsistent,
        // RDT-FR-18: the draft resolved and its prompt would not read. Told
        // apart from the two above because the model's id was *right*, and a
        // "no such draft" here would send it searching for one it already had.
        crate::drafts::ERR_PROMPT_UNREADABLE => ToolRefusal::DraftPromptUnreadable,
        _ => ToolRefusal::DraftNotFound,
    }
}

// ---------------------------------------------------------------------------
// Logging (RDT-FR-17)
// ---------------------------------------------------------------------------

/// RDT-FR-17: how long a `draft_id` may be before a record carries only its
/// head.
///
/// A real draft id is nowhere near it, so the bound costs a genuine call nothing
/// and only clips the pathological one — see [`bounded_argument`] for why an
/// unbounded argument would cost the record the tool name and the reason.
const LOGGED_DRAFT_ID_LIMIT: usize = 512;

/// RDT-FR-17: the `draft_id` as the model composed it, bounded.
///
/// Verbatim otherwise — untrimmed — because the point of the field is to say
/// which draft the model asked for, and the trimmed form answers a slightly
/// different question.
fn logged_draft_id(draft_id: &str) -> String {
    bounded_argument(draft_id, LOGGED_DRAFT_ID_LIMIT)
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// RDT-FR-01: `read_draft` as a `rig` portable tool.
///
/// Holds the app handle rather than a root, so a project opened after the tool
/// was constructed is visible to it and one closed since is not (TLC-FR-15).
/// Unlike `propose_draft_changes`, nothing binds this tool to a conversation or
/// to a draft: it reads any draft the worktree holds from any origin kind, and
/// it changes nothing, so there is no effect that could land somewhere the model
/// did not intend (CVL-FR-08). `session` names the agent session whose
/// `FsAccess` the read runs through (FSA-FR-29), bound here for the reason
/// `read_file` binds its own: an agent's whole filesystem reach is one instance,
/// discarded with the turn.
pub struct DraftReadTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> DraftReadTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>, session: impl Into<String>) -> Self {
        DraftReadTool {
            app,
            session: session.into(),
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: impl Into<String>,
        buffer: &'static LogBuffer,
    ) -> Self {
        DraftReadTool {
            app,
            session: session.into(),
            buffer,
        }
    }

    /// The whole call, with its logging (TLC-FR-14, RDT-FR-17).
    fn run(&self, args: ReadDraftArgs) -> Result<ReadDraftOutput, ToolRefusal> {
        // RDT-FR-14: no project, no worktree to resolve a draft in, so the
        // shared refusal rather than an attempt against nothing.
        let outcome =
            require_project_root(&self.app, &self.session).and_then(|root| read(&root, &args));
        match &outcome {
            // RDT-FR-17: the id asked for and the size of what came back. Not
            // the prompt, not the name, not the status, not the prompt path —
            // the prompt is the author's own unpublished material and the rest
            // are fields this call returned, none of which anything downstream
            // would redact.
            Ok(output) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! {
                    "draft_id" => logged_draft_id(&args.draft_id),
                    "bytes" => output.content.len(),
                },
            ),
            // Which draft was refused is the whole of what a model needs to
            // correct, and a refused id is one no read ever reached.
            Err(refusal) => log_tool_refusal_with(
                &self.app,
                self.buffer,
                NAME,
                refusal,
                log_fields! { "draft_id" => logged_draft_id(&args.draft_id) },
            ),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for DraftReadTool<R> {
    const NAME: &'static str = NAME;

    type Args = ReadDraftArgs;
    /// RDT-FR-05: a record rather than a bare `String`, which is the declared
    /// data-with-document case of TLC-FR-08 — the metadata and the prompt cannot
    /// be acted on apart.
    type Output = ReadDraftOutput;
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
        // RDT-FR-15: answers from the filesystem as it stands, consulting no
        // index, no scan, and no watcher, so it resolves before the future is
        // ever polled.
        std::future::ready(self.run(arguments))
    }
}
