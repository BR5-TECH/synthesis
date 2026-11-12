//! List OpenSpec changes tool (`LOC-list-openspec-changes-tool.md`).
//!
//! Each active change of `openspec/changes/` with the state of each artifact,
//! its task progress, and the capabilities it holds deltas for. It answers
//! through `OSM-openspec-mode.md`'s `list_changes(location)` in the active
//! worktree location and runs no process of its own (LOC-FR-DSMK).

use serde::{Deserialize, Serialize};

use super::openspec_shared::{common_refusal, on_blocking_thread, read_failed, resolve};
use super::{log_tool_refusal, log_tool_success, ToolRefusal};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

/// LOC-FR-ZDJW.
pub const NAME: &str = "list_openspec_changes";

/// LOC-FR-YVDP: the fixed text the model reads.
pub const DESCRIPTION: &str = "List the changes in progress in this OpenSpec project, from `openspec/changes/`. Each entry gives the change name, its schema, the state of each artifact (proposal, specs, design, tasks, or those of its schema), the tasks done and the tasks in total, and the capabilities it holds deltas for. Use this to find planned work that touches the same capabilities, or to say how far a change has got. Read an artifact with `read_file` below the folder `path` names, for example `<path>proposal.md`. Archived changes are not listed. This changes nothing.";

/// LOC-FR-YEQH: the tool's sentence of the **OpenSpec read failed** refusal.
pub const READ_FAILED: &str = "The OpenSpec CLI refused to list the changes of this project. The OpenSpec files may be malformed.";

/// The arguments: none.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListOpenSpecChangesArgs {}

/// The state of one artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactEntry {
    pub id: String,
    pub status: String,
}

/// One change (LOC-FR-MGXX, LOC-FR-XTUV).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeEntry {
    pub name: String,
    pub path: String,
    pub schema: String,
    pub artifacts: Vec<ArtifactEntry>,
    pub is_complete: bool,
    pub completed_tasks: u64,
    pub total_tasks: u64,
    pub capabilities: Vec<String>,
    pub last_modified: String,
}

/// TLC-FR-08: the collection under a named field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOpenSpecChangesOutput {
    pub changes: Vec<ChangeEntry>,
}

/// LOC-FR-YVDP: an object schema with no properties.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
}

/// LOC-FR-ZDJW: `list_openspec_changes` as a `rig` portable tool.
pub struct ListOpenSpecChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ListOpenSpecChangesTool<R> {
    /// TLC-FR-19: this tool's own constructor, bound to the turn's session.
    pub fn new(app: tauri::AppHandle<R>, session: &str) -> Self {
        ListOpenSpecChangesTool { app, session: session.to_string(), buffer: &BUFFER }
    }

    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: &str,
        buffer: &'static LogBuffer,
    ) -> Self {
        ListOpenSpecChangesTool { app, session: session.to_string(), buffer }
    }
}

/// The whole call, with its logging (LOC-FR-KZDF).
fn run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
    buffer: &'static LogBuffer,
) -> Result<ListOpenSpecChangesOutput, ToolRefusal> {
    let outcome = resolve(app, session).and_then(|(root, seam)| {
        // LOC-FR-DSMK: the active worktree location.
        seam.list_changes_at(app, buffer, Some(&root), &root)
            .map(|changes| {
                let mut changes: Vec<ChangeEntry> = changes
                    .into_iter()
                    .map(|c| ChangeEntry {
                        path: format!("openspec/changes/{}/", c.name),
                        name: c.name,
                        schema: c.schema,
                        artifacts: c
                            .artifacts
                            .into_iter()
                            .map(|a| ArtifactEntry { id: a.id, status: a.status })
                            .collect(),
                        is_complete: c.is_complete,
                        completed_tasks: c.completed_tasks,
                        total_tasks: c.total_tasks,
                        capabilities: c.capabilities,
                        last_modified: c.last_modified,
                    })
                    .collect();
                // LOC-FR-MGXX: the most recent first. RFC 3339 text in one
                // zone sorts as time does.
                changes.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
                ListOpenSpecChangesOutput { changes }
            })
            .map_err(|error| {
                common_refusal(&error, READ_FAILED).unwrap_or_else(|| read_failed(READ_FAILED))
            })
    });
    match &outcome {
        Ok(output) => {
            log_tool_success(app, buffer, NAME, log_fields! { "changes" => output.changes.len() })
        }
        Err(refusal) => log_tool_refusal(app, buffer, NAME, refusal),
    }
    outcome
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ListOpenSpecChangesTool<R> {
    const NAME: &'static str = NAME;

    type Args = ListOpenSpecChangesArgs;
    type Output = ListOpenSpecChangesOutput;
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
        _arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        let app = self.app.clone();
        let session = self.session.clone();
        let buffer = self.buffer;
        on_blocking_thread(move || run(&app, &session, buffer), Err(read_failed(READ_FAILED)))
    }
}
