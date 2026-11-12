//! What the three OpenSpec read tools share (`LOS-list-openspec-specs-tool.md`,
//! `ROS-read-openspec-spec-tool.md`, `LOC-list-openspec-changes-tool.md`).
//!
//! The project resolution, the refusals that read the same in the three, and
//! the move of a read onto a blocking thread. Each tool keeps its own name,
//! description, schema, output, and log record.

use tauri::Manager;

use super::refusals::FixedKind;
use super::{require_project_root, ToolRefusal};
use crate::openspec::{OpenSpec, OpenSpecReadError};

#[cfg(test)]
mod tests;

/// LOS, ROS, LOC: the **OpenSpec unavailable** refusal.
pub const UNAVAILABLE: &str = "OpenSpec cannot be read in this project now. The OpenSpec mode is off, or the OpenSpec CLI is not installed on this machine.";

/// LOS, ROS, LOC: the **OpenSpec unavailable** refusal, kind `Other`, not
/// retryable.
pub fn unavailable() -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::Other,
        message: UNAVAILABLE,
        code: "openspec_unavailable",
        retryable: false,
    }
}

/// LOS, ROS, LOC: the **OpenSpec read failed** refusal, with the tool's own
/// sentence. Kind `Other`, not retryable.
pub fn read_failed(message: &'static str) -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::Other,
        message,
        code: "openspec_read_failed",
        retryable: false,
    }
}

/// The refusal of a seam error that reads the same in the three tools, or
/// `None` for an error that only `read_openspec_spec` can meet. No CLI output
/// reaches a message: the error carries none.
pub fn common_refusal(
    error: &OpenSpecReadError,
    read_failed_message: &'static str,
) -> Option<ToolRefusal> {
    match error {
        OpenSpecReadError::NoOpenProject => Some(ToolRefusal::NoProjectOpen),
        OpenSpecReadError::OpenSpecModeInactive | OpenSpecReadError::CliUnavailable(_) => {
            Some(unavailable())
        }
        OpenSpecReadError::CliTimedOut
        | OpenSpecReadError::CliFailed { .. }
        | OpenSpecReadError::CliOutputUnreadable => Some(read_failed(read_failed_message)),
        OpenSpecReadError::InvalidCapabilityId | OpenSpecReadError::CapabilityNotFound => None,
        // No read tool names a change or a stream, so none of these reach a tool.
        OpenSpecReadError::UnknownStream
        | OpenSpecReadError::InvalidChangeName
        | OpenSpecReadError::ChangeNotFound
        | OpenSpecReadError::ChangeArchived { .. } => Some(read_failed(read_failed_message)),
    }
}

/// TLC-FR-13: the agent session's root, or the shared refusal. The seam is
/// the application's one instance; a missing one reads as unavailable.
pub fn resolve<'a, R: tauri::Runtime>(
    app: &'a tauri::AppHandle<R>,
    session: &str,
) -> Result<(crate::fs::RootFs, tauri::State<'a, OpenSpec>), ToolRefusal> {
    let root = require_project_root(app, session)?;
    let seam = app.try_state::<OpenSpec>().ok_or_else(unavailable)?;
    Ok((root, seam))
}

/// A read of the seam runs CLI processes, so it runs on a blocking thread and
/// never on the thread that drives the turn (TLC-FR-16).
pub async fn on_blocking_thread<T, F>(work: F, lost: T) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work).await.unwrap_or(lost)
}
