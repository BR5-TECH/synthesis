//! What a running update tells every surface watching it (WKS-FR-FQLS).
//!
//! The sibling of `merge_progress.rs`, kept apart because an update carries the
//! strategy the author chose and a merge carries none: a surface that renders
//! one from the other's event would name the wrong direction of work.

use tauri::{Emitter, Manager};

use super::artifact::MergeConflict;
use super::update_record::StreamUpdateStrategy;
use super::*;

/// WKS-FR-FQLS: what a running update reports itself with.
///
/// `turn` is zero once the update has settled, so a surface reading this alone
/// can tell an update that is working from one that has stopped.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateProgressPayload {
    project_key: String,
    stream_id: String,
    attempt_id: String,
    strategy: String,
    turn: u32,
    turns_max: u32,
    reconciling_paths: Vec<String>,
}

/// WKS-FR-FQLS: tell every surface where this update has reached.
pub(super) fn report<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    stream_id: &str,
    attempt_id: &str,
    strategy: StreamUpdateStrategy,
    turn: u32,
    reconciling: &[MergeConflict],
) {
    let _ = app.emit(
        WORK_STREAM_UPDATE_PROGRESS,
        UpdateProgressPayload {
            project_key: project_key.to_string(),
            stream_id: stream_id.to_string(),
            attempt_id: attempt_id.to_string(),
            strategy: strategy.as_str().to_string(),
            turn,
            turns_max: UPDATE_ATTEMPTS_MAX,
            reconciling_paths: reconciling.iter().map(|c| c.path.clone()).collect(),
        },
    );
}

/// GRB-FR-CLRO: the update, as one determinate in-flight operation of
/// `PRG-progress-reporting.md`.
///
/// A guard rather than a call at each exit: the drive loop leaves by a dozen
/// paths, and an operation left registered animates in the status bar for the
/// rest of the session.
pub(super) struct UpdateProgress<'a, R: tauri::Runtime> {
    app: &'a tauri::AppHandle<R>,
    id: Option<crate::progress::OperationId>,
    project_key: String,
    stream_id: String,
    strategy: StreamUpdateStrategy,
}

impl<'a, R: tauri::Runtime> UpdateProgress<'a, R> {
    pub(super) fn begin(
        app: &'a tauri::AppHandle<R>,
        project_key: &str,
        stream_id: &str,
        stream_name: &str,
        strategy: StreamUpdateStrategy,
    ) -> Self {
        let id = app
            .try_state::<crate::progress::ProgressRegistry>()
            .map(|registry| {
                crate::progress::register_and_publish(
                    app,
                    &registry,
                    "stream-update",
                    &format!("Updating {stream_name}"),
                    None,
                    Some(UPDATE_ATTEMPTS_MAX as u64),
                )
            });
        Self {
            app,
            id,
            project_key: project_key.to_string(),
            stream_id: stream_id.to_string(),
            strategy,
        }
    }

    /// One more semantic turn has begun, so the bar advances by one and every
    /// surface is told which turn it is and what that turn reconciles.
    pub(super) fn turn(&self, turn: u32, attempt_id: &str, reconciling: &[MergeConflict]) {
        report(
            self.app,
            &self.project_key,
            &self.stream_id,
            attempt_id,
            self.strategy,
            turn,
            reconciling,
        );
        self.advance(turn);
    }

    fn advance(&self, turn: u32) {
        let (Some(id), Some(registry)) = (
            self.id,
            self.app.try_state::<crate::progress::ProgressRegistry>(),
        ) else {
            return;
        };
        crate::progress::update_and_publish(
            self.app,
            &registry,
            id,
            Some(turn as u64),
            Some(UPDATE_ATTEMPTS_MAX as u64),
        );
    }
}

impl<R: tauri::Runtime> Drop for UpdateProgress<'_, R> {
    fn drop(&mut self) {
        // WKS-FR-FQLS: the update has settled, whichever way it settled.
        report(
            self.app,
            &self.project_key,
            &self.stream_id,
            "",
            self.strategy,
            0,
            &[],
        );
        let (Some(id), Some(registry)) = (
            self.id,
            self.app.try_state::<crate::progress::ProgressRegistry>(),
        ) else {
            return;
        };
        crate::progress::terminate_and_publish(
            self.app,
            &registry,
            id,
            crate::progress::OperationState::Finished,
        );
    }
}
