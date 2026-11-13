//! The task progress of one change (`OSM-openspec-mode.md` OSM-FR-QLRW,
//! OSM-FR-ODHE, OSM-FR-PEAK, OSM-FR-WIQF).
//!
//! `openspec instructions apply --change <name> --json` is the one CLI read
//! that tells which tasks of a change are done and whether the change can be
//! applied. The seam runs it with the artifact `apply` alone (OSM-FR-VALR). It
//! reads nothing of the change itself: the CLI parses `tasks.md`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{change, OpenSpec, OpenSpecReadError};
use crate::fs::{EntryKind, RootFs};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

/// OSM-FR-WIQF: the three states of `ApplyInstructions`.
pub const APPLY_STATES: [&str; 3] = ["blocked", "ready", "all_done"];

/// OSM contract surface `ApplyInstructions`. The graduation loop carries it
/// whole in a task input, so its fields are in snake case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyInstructions {
    pub state: String,
    pub total: u64,
    pub complete: u64,
    /// In the order the CLI gives.
    pub tasks: Vec<ApplyTask>,
    /// Empty where the CLI names none.
    pub missing_artifacts: Vec<String>,
    /// OFP-FR-BWML: the CLI's warnings, for the diagnostics of
    /// `change_not_ready`. Not part of a task input.
    #[serde(default, skip_serializing)]
    pub warnings: Vec<String>,
    /// OFP-FR-BWML: the CLI's instruction text. Not part of a task input.
    #[serde(default, skip_serializing)]
    pub instruction: Option<String>,
}

/// One task of a change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyTask {
    pub description: String,
    pub done: bool,
}

impl ApplyInstructions {
    /// OSM-FR-PEAK: the description of the first task that is not done.
    pub fn next_task(&self) -> Option<String> {
        self.tasks.iter().find(|task| !task.done).map(|task| task.description.clone())
    }

    /// OFP-FR-BWML: the diagnostics of `change_not_ready`.
    pub fn not_ready_diagnostics(&self) -> Vec<super::CliDiagnostic> {
        super::failure::not_ready_diagnostics(
            &self.missing_artifacts,
            &self.warnings,
            self.instruction.as_deref(),
        )
    }

    /// OSM-FR-PEAK: what `"read apply progress"` returns.
    pub fn progress(&self) -> ApplyProgress {
        ApplyProgress {
            state: self.state.clone(),
            total: self.total,
            complete: self.complete,
            next_task: self.next_task(),
            missing_artifacts: self.missing_artifacts.clone(),
        }
    }
}

/// OSM contract surface `ApplyProgress`, as the frontend reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProgress {
    pub state: String,
    pub total: u64,
    pub complete: u64,
    pub next_task: Option<String>,
    pub missing_artifacts: Vec<String>,
}

// ---------------------------------------------------------------------------
// The CLI fields read (OSM contract surface, "CLI fields read")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstructionsOutput {
    state: String,
    progress: ProgressOutput,
    #[serde(default)]
    tasks: Vec<TaskOutput>,
    #[serde(default)]
    missing_artifacts: Option<Vec<String>>,
    #[serde(default)]
    warnings: Vec<serde_json::Value>,
    #[serde(default)]
    instruction: Option<serde_json::Value>,
}

/// OFP-FR-BWML: a warning or an instruction is a text, or an object whose
/// `message` is one. Any other value gives no text.
fn text_of(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        .map(str::to_string)
}

/// OSM-FR-WIQF: a count that is not an integer of 0 or more does not decode.
#[derive(Deserialize)]
struct ProgressOutput {
    total: u64,
    complete: u64,
}

#[derive(Deserialize)]
struct TaskOutput {
    #[serde(default)]
    description: String,
    #[serde(default)]
    done: bool,
}

/// OSM-FR-QLRW, OSM-FR-WIQF: the answer of `instructions apply`. A field the
/// shape does not name is ignored (OSM-FR-WWWK).
pub fn parse_apply_instructions(bytes: &[u8]) -> Result<ApplyInstructions, OpenSpecReadError> {
    let out: InstructionsOutput =
        serde_json::from_slice(bytes).map_err(|_| OpenSpecReadError::CliOutputUnreadable)?;
    if !APPLY_STATES.contains(&out.state.as_str()) {
        return Err(OpenSpecReadError::CliOutputUnreadable);
    }
    Ok(ApplyInstructions {
        state: out.state,
        total: out.progress.total,
        complete: out.progress.complete,
        tasks: out
            .tasks
            .into_iter()
            .map(|task| ApplyTask { description: task.description, done: task.done })
            .collect(),
        missing_artifacts: out.missing_artifacts.unwrap_or_default(),
        warnings: out.warnings.iter().filter_map(text_of).collect(),
        instruction: out.instruction.as_ref().and_then(text_of),
    })
}

impl OpenSpec {
    /// OSM-FR-QLRW, OSM-FR-ODHE: the task progress of one change at a
    /// location. An invalid name and a missing change folder start no process.
    pub fn apply_instructions<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        name: &str,
    ) -> Result<ApplyInstructions, OpenSpecReadError> {
        let result = self.apply_ready(sink, buffer, active, at, name).and_then(|exe| {
            let args = ["instructions", "apply", "--change", name, "--json"];
            let out = self
                .run_logged(sink, buffer, &exe, "instructions", &args, at.path())
                .map_err(OpenSpecReadError::from)?;
            parse_apply_instructions(&out)
        });
        match &result {
            Ok(read) => logging::log_info(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec apply progress read",
                log_fields! {
                    "state" => read.state.clone(),
                    "total" => read.total,
                    "complete" => read.complete,
                },
            ),
            Err(error) => logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec read failed",
                log_fields! { "read" => "apply_instructions", "code" => error.code() },
            ),
        }
        result
    }

    /// OSM-FR-XSOC, OSM-FR-ODHE: the mode, the name, and the folder are
    /// checked before the CLI is, so neither refusal of ODHE starts a process.
    fn apply_ready<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        active: Option<&RootFs>,
        at: &RootFs,
        name: &str,
    ) -> Result<PathBuf, OpenSpecReadError> {
        let active = active.ok_or(OpenSpecReadError::NoOpenProject)?;
        if !Self::mode_active(Some(active)) {
            return Err(OpenSpecReadError::OpenSpecModeInactive);
        }
        if !change::valid_change_name(name) {
            return Err(OpenSpecReadError::InvalidChangeName);
        }
        let folder = at.path().join("openspec/changes").join(name);
        if !at.file_info(folder).is_ok_and(|info| info.kind == EntryKind::Dir) {
            return Err(OpenSpecReadError::ChangeNotFound);
        }
        match self.cli_state_in(sink, buffer, Some(at)) {
            super::OpenSpecCliState::Available { path, .. } => Ok(PathBuf::from(path)),
            other => Err(OpenSpecReadError::CliUnavailable(other)),
        }
    }
}

/// OSM-FR-PEAK: `apply_instructions` in the working copy of a stream, with the
/// application's own seam. Blocking.
pub fn read_apply_progress_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    name: &str,
) -> Result<ApplyInstructions, OpenSpecReadError> {
    use tauri::Manager;
    let seam = app.try_state::<OpenSpec>().ok_or(OpenSpecReadError::OpenSpecModeInactive)?;
    let active =
        app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
    let location = change::ChangeLocation::Stream { stream_id: stream_id.to_string() };
    let at = change::working_copy(app, &location)?;
    seam.apply_instructions(app, &logging::BUFFER, active.as_ref(), &at, name)
}

/// OSM-FR-PEAK, OFP-FR-QLQJ: `"read apply progress"`, on a blocking thread.
/// Every failure rejects as an `OpenSpecFailure`.
#[tauri::command]
pub async fn read_apply_progress<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
) -> Result<ApplyProgress, Box<super::OpenSpecFailure>> {
    tauri::async_runtime::spawn_blocking(move || {
        read_apply_progress_now(&app, &stream_id, &change)
            .map(|read| read.progress())
            .map_err(|e| Box::new(e.to_failure()))
    })
    .await
    .map_err(|_| Box::new(super::OpenSpecFailure::new(super::failure::CODE_FAILED)))?
}
