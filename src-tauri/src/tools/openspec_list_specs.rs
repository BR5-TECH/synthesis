//! List OpenSpec specs tool (`LOS-list-openspec-specs-tool.md`).
//!
//! Each capability of `openspec/specs/` by id, with its requirement count, as
//! the OpenSpec CLI of the project reports them. It answers through
//! `OSM-openspec-mode.md`'s `list_capabilities()` and runs no process of its
//! own (LOS-FR-RWGX).

use serde::{Deserialize, Serialize};

use super::openspec_shared::{common_refusal, on_blocking_thread, read_failed, resolve};
use super::{log_tool_refusal, log_tool_success, ToolRefusal};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

/// LOS-FR-QKCD.
pub const NAME: &str = "list_openspec_specs";

/// LOS-FR-UPKL: the fixed text the model reads.
pub const DESCRIPTION: &str = "List the capabilities this OpenSpec project has specified in `openspec/specs/`. Each entry gives the capability id and how many requirements it holds. Use this to find out which capabilities exist before you talk about a new one or a change to one. Then use `read_openspec_spec` with an id to read its requirements and scenarios. This does not list changes in progress, does not search, and changes nothing.";

/// LOS-FR-IEDQ: the tool's sentence of the **OpenSpec read failed** refusal.
pub const READ_FAILED: &str =
    "The OpenSpec CLI could not list the specs of this project. The OpenSpec files may be malformed.";

/// The arguments: none (LOS-FR-UPKL). Not `deny_unknown_fields`, so a model
/// that sent a field it invented is still answered.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListOpenSpecSpecsArgs {}

/// One capability (LOS-FR-XHLX).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityEntry {
    pub id: String,
    pub requirement_count: u64,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOpenSpecSpecsOutput {
    pub capabilities: Vec<CapabilityEntry>,
}

/// LOS-FR-UPKL: an object schema with no properties.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
}

/// LOS-FR-QKCD: `list_openspec_specs` as a `rig` portable tool.
pub struct ListOpenSpecSpecsTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ListOpenSpecSpecsTool<R> {
    /// TLC-FR-19: this tool's own constructor. `session` is the agent session
    /// of the turn, whose instance governs every read of the project.
    pub fn new(app: tauri::AppHandle<R>, session: &str) -> Self {
        ListOpenSpecSpecsTool { app, session: session.to_string(), buffer: &BUFFER }
    }

    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: &str,
        buffer: &'static LogBuffer,
    ) -> Self {
        ListOpenSpecSpecsTool { app, session: session.to_string(), buffer }
    }
}

/// The whole call, with its logging (LOS-FR-PAFH).
fn run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
    buffer: &'static LogBuffer,
) -> Result<ListOpenSpecSpecsOutput, ToolRefusal> {
    let outcome = resolve(app, session).and_then(|(root, seam)| {
        seam.list_capabilities(app, buffer, Some(&root))
            .map(|list| ListOpenSpecSpecsOutput {
                capabilities: list
                    .into_iter()
                    .map(|c| CapabilityEntry { id: c.id, requirement_count: c.requirement_count })
                    .collect(),
            })
            .map_err(|error| {
                common_refusal(&error, READ_FAILED).unwrap_or_else(|| read_failed(READ_FAILED))
            })
    });
    match &outcome {
        Ok(output) => log_tool_success(
            app,
            buffer,
            NAME,
            log_fields! { "capabilities" => output.capabilities.len() },
        ),
        Err(refusal) => log_tool_refusal(app, buffer, NAME, refusal),
    }
    outcome
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ListOpenSpecSpecsTool<R> {
    const NAME: &'static str = NAME;

    type Args = ListOpenSpecSpecsArgs;
    type Output = ListOpenSpecSpecsOutput;
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
