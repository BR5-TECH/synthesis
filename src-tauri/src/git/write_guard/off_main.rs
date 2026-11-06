//! The write commands that run the main branch write check off the main
//! thread (`GTC-git.md` GTC-FR-JWOL, GTC-FR-TTSV).
//!
//! The check can read GitHub when the session has no protection read yet, and
//! one read can take some seconds. A sync command runs on the main thread, so a
//! write command that runs the check does its work through [`run_write`]: on a
//! blocking thread. The writes take [`in_sequence`] after the check, so they
//! stay in sequence as they did on the main thread, and a slow check holds no
//! other write.

use std::sync::{Mutex, MutexGuard};

/// The lock that keeps the writes of these commands in sequence.
static SEQUENCE: Mutex<()> = Mutex::new(());

/// Take the lock of the write sequence. A write takes it after its check.
pub(crate) fn in_sequence() -> MutexGuard<'static, ()> {
    SEQUENCE.lock().unwrap_or_else(|error| error.into_inner())
}

/// Run `work` on a blocking thread, with the handle of the application.
/// `operation` names the command in the error of a thread that failed.
pub(crate) async fn run_write<R, T, F>(
    app: tauri::AppHandle<R>,
    operation: &'static str,
    work: F,
) -> Result<T, String>
where
    R: tauri::Runtime,
    T: Send + 'static,
    F: FnOnce(tauri::AppHandle<R>) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || work(app))
        .await
        .map_err(|error| format!("{operation} failed: {error}"))?
}

/// The project root of `app`, or the typed "no project open" error.
pub(crate) fn project_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<crate::fs::RootFs, String> {
    use tauri::Manager;
    app.try_state::<crate::project::ProjectState>()
        .ok_or_else(|| "no project open".to_string())?
        .require_root()
}

/// The machine store of the project of `app`, or the typed error.
pub(crate) fn project_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<crate::fs::RootFs, String> {
    use tauri::Manager;
    app.try_state::<crate::project::ProjectState>()
        .ok_or_else(|| "no project open".to_string())?
        .require_store()
}
