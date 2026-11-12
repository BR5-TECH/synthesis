//! Read OpenSpec spec tool (`ROS-read-openspec-spec-tool.md`).
//!
//! One capability of an OpenSpec project: its title, overview, and each
//! requirement with its scenarios, or only the requirements a name selects. It
//! answers through `OSM-openspec-mode.md`'s `read_capability(id)` and runs no
//! process of its own (ROS-FR-XEHR).

use serde::{Deserialize, Serialize};

use super::openspec_shared::{common_refusal, on_blocking_thread, read_failed, resolve};
use super::refusals::FixedKind;
use super::{bounded_argument, log_tool_refusal_with, log_tool_success, ToolRefusal};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::openspec::OpenSpecReadError;

/// ROS-FR-XPYM.
pub const NAME: &str = "read_openspec_spec";

/// ROS-FR-AZNY: the fixed text the model reads.
pub const DESCRIPTION: &str = "Read one capability of this OpenSpec project from `openspec/specs/<capability>/spec.md`: its title, its overview, and each requirement with its scenarios. Use this when you need what a capability requires, or the exact name of a requirement. Get the capability id from `list_openspec_specs` first. Pass `requirement` with a requirement name to read only that requirement. A change cites a requirement by this name when it modifies or removes it. This does not read a change, does not search, and changes nothing.";

const CAPABILITY_DESCRIPTION: &str = "The capability id that `list_openspec_specs` returned, for example `auth` or `ui/theme`. This is not a file path.";
const REQUIREMENT_DESCRIPTION: &str = "The name of one requirement to read, as this tool or a change gives it. Letter case does not matter. Leave it out to read every requirement of the capability.";

/// ROS refusal sentences.
pub const INVALID_CAPABILITY: &str = "The capability must be an id that `list_openspec_specs` returned, such as `auth` or `ui/theme`. Call again with such an id.";
pub const UNKNOWN_CAPABILITY: &str = "No capability with that id exists in this project. Call `list_openspec_specs` to see the ids that exist.";
pub const UNKNOWN_REQUIREMENT: &str = "That capability has no requirement with that name. Call again without `requirement` to see the names of its requirements.";
pub const READ_FAILED: &str =
    "The OpenSpec CLI refused to read this capability. Its spec file may be malformed.";

/// ROS-FR-PRDW: the longest `capability` a log record carries whole.
const LOGGED_CAPABILITY_CHARS: usize = 512;

/// The arguments (ROS contract surface).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReadOpenSpecSpecArgs {
    #[serde(default)]
    pub capability: String,
    #[serde(default)]
    pub requirement: Option<String>,
}

/// One scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScenarioEntry {
    pub name: String,
    pub text: String,
}

/// One requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequirementEntry {
    pub name: String,
    pub text: String,
    pub scenarios: Vec<ScenarioEntry>,
}

/// TLC-FR-08: the data-with-document shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadOpenSpecSpecOutput {
    pub id: String,
    pub title: String,
    pub overview: String,
    pub requirements: Vec<RequirementEntry>,
}

/// ROS-FR-AZNY: the two documented arguments.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "capability": { "type": "string", "description": CAPABILITY_DESCRIPTION },
            "requirement": { "type": "string", "description": REQUIREMENT_DESCRIPTION },
        },
        "required": ["capability"],
    })
}

fn fixed(kind: FixedKind, message: &'static str, code: &'static str) -> ToolRefusal {
    ToolRefusal::Fixed { kind, message, code, retryable: true }
}

/// ROS-FR-CSZP: the names match after both are trimmed, without regard to
/// letter case.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// ROS-FR-XPYM: `read_openspec_spec` as a `rig` portable tool.
pub struct ReadOpenSpecSpecTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ReadOpenSpecSpecTool<R> {
    /// TLC-FR-19: this tool's own constructor, bound to the turn's session.
    pub fn new(app: tauri::AppHandle<R>, session: &str) -> Self {
        ReadOpenSpecSpecTool { app, session: session.to_string(), buffer: &BUFFER }
    }

    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: &str,
        buffer: &'static LogBuffer,
    ) -> Self {
        ReadOpenSpecSpecTool { app, session: session.to_string(), buffer }
    }
}

fn read<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
    buffer: &'static LogBuffer,
    args: &ReadOpenSpecSpecArgs,
) -> Result<ReadOpenSpecSpecOutput, ToolRefusal> {
    // ROS-FR-QSEO: the open project comes first, so a closed project is the
    // shared refusal whatever the arguments are.
    let (root, seam) = resolve(app, session)?;
    if args.capability.trim().is_empty() {
        return Err(fixed(FixedKind::InvalidArgs, INVALID_CAPABILITY, "invalid_capability"));
    }
    let spec =
        seam.read_capability(app, buffer, Some(&root), &args.capability).map_err(|error| {
            common_refusal(&error, READ_FAILED).unwrap_or_else(|| match error {
                OpenSpecReadError::CapabilityNotFound => {
                    fixed(FixedKind::NotFound, UNKNOWN_CAPABILITY, "unknown_capability")
                }
                _ => fixed(FixedKind::InvalidArgs, INVALID_CAPABILITY, "invalid_capability"),
            })
        })?;
    let requirements: Vec<RequirementEntry> = spec
        .requirements
        .into_iter()
        .filter(|r| args.requirement.as_deref().is_none_or(|wanted| same_name(&r.name, wanted)))
        .map(|r| RequirementEntry {
            name: r.name,
            text: r.text,
            scenarios: r
                .scenarios
                .into_iter()
                .map(|s| ScenarioEntry { name: s.name, text: s.text })
                .collect(),
        })
        .collect();
    if args.requirement.is_some() && requirements.is_empty() {
        return Err(fixed(FixedKind::NotFound, UNKNOWN_REQUIREMENT, "unknown_requirement"));
    }
    Ok(ReadOpenSpecSpecOutput {
        id: spec.id,
        title: spec.title,
        overview: spec.overview,
        requirements,
    })
}

/// The whole call, with its logging (ROS-FR-PRDW).
fn run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
    buffer: &'static LogBuffer,
    args: ReadOpenSpecSpecArgs,
) -> Result<ReadOpenSpecSpecOutput, ToolRefusal> {
    let outcome = read(app, session, buffer, &args);
    let capability = bounded_argument(&args.capability, LOGGED_CAPABILITY_CHARS);
    match &outcome {
        Ok(output) => log_tool_success(
            app,
            buffer,
            NAME,
            log_fields! {
                "capability" => capability,
                "requirementGiven" => args.requirement.is_some(),
                "requirements" => output.requirements.len(),
            },
        ),
        Err(refusal) => log_tool_refusal_with(
            app,
            buffer,
            NAME,
            refusal,
            log_fields! { "capability" => capability },
        ),
    }
    outcome
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ReadOpenSpecSpecTool<R> {
    const NAME: &'static str = NAME;

    type Args = ReadOpenSpecSpecArgs;
    type Output = ReadOpenSpecSpecOutput;
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
        let app = self.app.clone();
        let session = self.session.clone();
        let buffer = self.buffer;
        on_blocking_thread(
            move || run(&app, &session, buffer, arguments),
            Err(read_failed(READ_FAILED)),
        )
    }
}
