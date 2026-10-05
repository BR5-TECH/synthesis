//! Remembering and resuming a project's active worktree
//! (`WTC-worktree-context.md` WTC-FR-17).
//!
//! Split out of `worktree.rs` so that file holds the enumeration, the
//! re-rooting and the commands.

use super::*;

/// Remember which worktree a project was last working in (WTC-FR-17 /
/// `GSS-global-settings-storage.md` GSS-FR-16). Best-effort: a settings write
/// that fails must not undo a switch that already happened.
pub(super) fn remember(store: &GlobalSettingsStore, anchor: &Path, worktree: &Path) {
    if let Err(e) = store.save_active_worktree(&to_string_path(anchor), &to_string_path(worktree)) {
        eprintln!("synthesis: failed to persist active worktree: {e}");
    }
}

/// The active worktree a project should resume in (WTC-FR-17 / WTC-FR-18).
///
/// Opening a *linked* worktree path activates exactly that one, superseding
/// whatever was remembered. Opening the project's anchor — the primary
/// worktree — resumes the remembered worktree instead, falling back to the
/// primary when it no longer exists. The bool reports that fallback so the UI
/// can surface it.
pub fn resume_active_worktree(
    store: &GlobalSettingsStore,
    anchor: &Path,
    opened: &Path,
) -> (PathBuf, bool) {
    let Ok(repo) = open_repo(opened) else {
        // Not a repository: there is only one place the project can be.
        return (opened.to_path_buf(), false);
    };
    let Some(opened_worktree) = containing_worktree(&repo) else {
        return (opened.to_path_buf(), false);
    };
    // A worktree other than the anchor was opened explicitly — that choice wins.
    if opened_worktree != canonicalize_lenient(anchor) {
        return (opened.to_path_buf(), false);
    }
    let Ok(Some(remembered)) = store.load_active_worktree(&to_string_path(anchor)) else {
        return (opened.to_path_buf(), false);
    };
    let remembered = PathBuf::from(remembered);
    if remembered == opened_worktree {
        return (opened.to_path_buf(), false);
    }
    // The remembered worktree must still be one of this repository's, and its
    // directory must still be there. A co-located project keeps its position
    // inside the checkout it resumes into.
    match validate_activation_target(opened, &remembered.to_string_lossy()) {
        Ok(worktree) => (
            content_root_in(&worktree, &root_offset(&repo, opened)),
            false,
        ),
        Err(_) => (opened.to_path_buf(), true),
    }
}
