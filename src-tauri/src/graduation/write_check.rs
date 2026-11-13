//! The main branch write check of a graduation start (`GRD-graduation.md`
//! GRD-FR-WJLO, GRD-FR-MOWZ).
//!
//! A start, a restart, and a direct start run the check of `GTC-git.md`
//! GTC-FR-JWOL before they enqueue anything. The check covers the branch of the
//! active worktree, which takes the draft storage commit of the start, and the
//! pinned branch of a direct run. The later commits of the run, and the apply
//! of a merge run, run no check (GRD-FR-MOWZ): the author decided at the start.

use super::*;

/// GRD-FR-WJLO: check the branches a start of a run of `project_key` writes
/// to. The active worktree is checked only when it is a worktree of that
/// project, because only then does it take the draft storage commit.
pub(super) fn require_start_write<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    pinned: Option<&DirectTarget>,
    operation: &str,
) -> Result<(), String>
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .filter(|_| queue::project_key(app) == project_key);
    if let Some(root) = root.as_ref() {
        crate::git::write_guard::commands::require_active_branch_write(app, root, operation)?;
    }
    if let Some(target) = pinned {
        let active = root.as_deref().and_then(crate::git::write_guard::branch_at);
        if active.as_deref() != Some(target.branch.as_str()) {
            let at = Path::new(&target.worktree_path);
            crate::git::write_guard::commands::require_branch_write(
                app,
                root.as_deref().unwrap_or(at),
                Some(&target.branch),
                operation,
            )?;
        }
    }
    Ok(())
}

/// GRD-FR-WJLO: read the guards that [`require_start_write`] checks, before a
/// start takes the write lock of the graduation state (the prime runs first in
/// each start command). A read can reach GitHub,
/// and a slow read must hold no other graduation command. The check under the
/// lock then answers from the session memory. It refuses nothing.
pub(super) fn prime_start_write<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    pinned: Option<(&str, &str)>,
) where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    use crate::git::write_guard::{branch_at, commands::prime_branch_guard};
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .filter(|_| queue::project_key(app) == project_key);
    if let Some(root) = root.as_ref() {
        prime_branch_guard(app, root, branch_at(root).as_deref());
    }
    if let Some((worktree_path, branch)) = pinned {
        let at = root.as_deref().unwrap_or(Path::new(worktree_path));
        prime_branch_guard(app, at, Some(branch));
    }
}

/// GRD-FR-WJLO: [`prime_start_write`] for a restart of the run `run_id`, before
/// the write lock. The restart reads the run again under the lock.
pub(super) fn prime_restart_write<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str)
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let Ok(run) = runs::load_run(app, run_id) else {
        return;
    };
    // GRD-FR-WJLO: a run with no draft writes to its stream branch alone.
    if run.is_change_run() {
        change_runs::prime_stream_write(app, &run.stream_id);
        return;
    }
    let pinned = run.direct_target.as_ref();
    let pinned = pinned.map(|target| (target.worktree_path.as_str(), target.branch.as_str()));
    prime_start_write(app, &run.project_key, pinned);
}

/// GRD-FR-WJLO: the check of a restart of `discarded`, on the branches the
/// new run writes to.
pub(super) fn require_restart_write<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discarded: &GraduationRun,
) -> Result<(), String>
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let pinned = discarded.direct_target.as_ref();
    require_start_write(app, &discarded.project_key, pinned, "restart_graduation_run")
}
