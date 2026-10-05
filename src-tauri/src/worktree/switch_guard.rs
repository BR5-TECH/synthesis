//! The worktree switches a project open can make
//! (`WTC-worktree-context.md` WTC-FR-TXKY).
//!
//! Opening the open project at another of its worktrees re-roots it like any
//! other switch, so the guard that binds the three commands of this module binds
//! it too.

use super::*;

/// WTC-FR-TXKY: whether an open lands the open project on a different worktree.
///
/// `anchor` is the project the open names and `active_worktree` the worktree it
/// resolved to. An open of another project is not a switch of this one; a
/// project change has its own rule (`GRD-graduation.md` GRD-FR-TWMA).
pub fn opens_other_worktree(project: &ProjectState, anchor: &str, active_worktree: &str) -> bool {
    if project.anchor().as_deref() != Some(anchor) {
        return false;
    }
    let Ok(root) = project.require_root() else {
        return false;
    };
    let Ok(current) = active_entry_for(&root) else {
        return false;
    };
    canonicalize_lenient(Path::new(&current.path)) != canonicalize_lenient(Path::new(active_worktree))
}
