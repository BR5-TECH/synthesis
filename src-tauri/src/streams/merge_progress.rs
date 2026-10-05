//! The pre-conflict check of a merge, as one in-flight operation
//! (`PRG-progress-reporting.md`, `WKS-work-streams.md`).
//!
//! The check runs from the request until it settles: a clean merge, nothing to
//! merge, a refusal, or the handoff of a conflict to a merge run. The status bar
//! shows it as one operation of every window, so the progress is read from the
//! backend and not only from the window that started the merge.

use tauri::Manager;

/// A guard rather than a call at each exit: the check leaves by many paths, and
/// an operation left registered animates in the status bar for the rest of the
/// session (per `../ui/STB-status-bar.md` STB-FR-06).
pub(super) struct MergeProgress<'a, R: tauri::Runtime> {
    app: &'a tauri::AppHandle<R>,
    id: Option<crate::progress::OperationId>,
}

impl<'a, R: tauri::Runtime> MergeProgress<'a, R> {
    pub(super) fn begin(app: &'a tauri::AppHandle<R>, stream_name: &str) -> Self {
        let id = app
            .try_state::<crate::progress::ProgressRegistry>()
            .map(|registry| {
                crate::progress::register_and_publish(
                    app,
                    &registry,
                    "merge",
                    &format!("Merging {stream_name}"),
                    None,
                    None,
                )
            });
        Self { app, id }
    }
}

impl<R: tauri::Runtime> Drop for MergeProgress<'_, R> {
    fn drop(&mut self) {
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
