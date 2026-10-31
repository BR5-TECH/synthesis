//! The content root that is gone (`WTC-worktree-context.md` WTC-FR-CPRM,
//! WTC-FR-DZLO, WTC-FR-NQLS, WTC-FR-ZGYJ, WTC-FR-ZXRF).
//!
//! A linked worktree can be removed outside the application. Each read here
//! starts from the primary worktree, because a gone root opens nothing.

use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// WTC-FR-NQLS: `leave_gone_worktree` asked of a content root that is not gone.
pub const ERR_WORKTREE_NOT_GONE: &str = "worktree not gone";

/// WTC-FR-CPRM: what can be said of the content root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveRootHealth {
    Healthy,
    Gone,
    Unknown,
}

/// WTC-FR-CPRM: the content root's state, with the primary worktree the author
/// can go back to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRootState {
    pub state: ActiveRootHealth,
    pub primary_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_branch: Option<String>,
}

/// WTC-FR-ZXRF: where `root` sits inside its worktree, while it can be read.
pub fn offset_of(root: &Path) -> PathBuf {
    open_repo(root).map(|repo| root_offset(&repo, root)).unwrap_or_default()
}

/// The branch the primary worktree holds, or nothing where its `HEAD` is on none.
fn branch_of(primary: &Repository) -> Option<String> {
    let head = primary.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    head.shorthand().ok().map(str::to_string)
}

/// A failed Git read says `gone` on a positive "not found" alone. This answer
/// discards unsaved changes, so any other failure is not proof of it.
fn state_of_read(read: Result<Repository, git2::Error>) -> ActiveRootHealth {
    match read {
        Ok(_) => ActiveRootHealth::Healthy,
        Err(e) if e.code() == git2::ErrorCode::NotFound => ActiveRootHealth::Gone,
        Err(_) => ActiveRootHealth::Unknown,
    }
}

/// WTC-FR-DZLO: `root` stands on a registered worktree that opens, the deepest
/// one where one registered worktree stands inside another.
///
/// `Repository::open` walks no parent, so a directory with no Git files fails
/// here. A registration alone proves nothing: Git keeps one after `rm -rf`.
fn registered_worktree_state(primary: &Repository, root: &Path) -> ActiveRootHealth {
    let Ok(names) = primary.worktrees() else {
        return ActiveRootHealth::Unknown;
    };
    let mut unread = false;
    let mut deepest: Option<PathBuf> = None;
    for name in names.iter().filter_map(|name| name.ok().flatten()) {
        let Ok(worktree) = primary.find_worktree(name) else {
            unread = true;
            continue;
        };
        let path = canonicalize_lenient(worktree.path());
        let deeper = deepest.as_ref().is_none_or(|held| path.starts_with(held));
        if root.starts_with(&path) && deeper {
            deepest = Some(path);
        }
    }
    match deepest {
        Some(path) => match state_of_read(Repository::open(&path)) {
            ActiveRootHealth::Healthy => state_of_read(Repository::discover(root)),
            other => other,
        },
        None if unread => ActiveRootHealth::Unknown,
        None => ActiveRootHealth::Gone,
    }
}

/// WTC-FR-CPRM, WTC-FR-DZLO: the state of `root`, read from `anchor`.
pub fn root_state_of(root: &Path, anchor: &Path, offset: &Path) -> ActiveRootState {
    let unknown = || ActiveRootState {
        state: ActiveRootHealth::Unknown,
        primary_path: anchor.to_string_lossy().into_owned(),
        primary_branch: None,
    };
    let Ok(repo) = open_repo(anchor) else {
        return unknown();
    };
    let Some(primary_path) = primary_worktree_root(&repo) else {
        return unknown();
    };
    let Ok(primary) = Repository::open(&primary_path) else {
        return unknown();
    };
    let root = canonicalize_lenient(root);
    // Compared with the offset rather than by prefix: a linked worktree can
    // stand inside the primary worktree's directory.
    let state = if root == content_root_in(&primary_path, offset) {
        ActiveRootHealth::Healthy
    } else {
        registered_worktree_state(&primary, &root)
    };
    ActiveRootState {
        state,
        primary_path: primary_path.to_string_lossy().into_owned(),
        primary_branch: branch_of(&primary),
    }
}

fn state_of_project(project: &ProjectState) -> Result<ActiveRootState, String> {
    let root = project.require_root()?;
    let anchor = project.anchor().ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    Ok(root_state_of(root.as_ref(), Path::new(&anchor), &project.root_offset()))
}

/// WTC-FR-CPRM: changes nothing.
#[tauri::command]
pub fn active_root_state(project: State<'_, ProjectState>) -> Result<ActiveRootState, String> {
    state_of_project(&project)
}

#[tauri::command]
pub fn leave_gone_worktree(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
    store: State<'_, GlobalSettingsStore>,
) -> Result<WorktreeContext, String> {
    leave_gone_worktree_at(&app, &project, &watcher, &store)
}

/// WTC-FR-NQLS, WTC-FR-ZGYJ, WTC-FR-ZXRF: re-root on the primary worktree.
///
/// A route of its own rather than `activate_worktree`, which keeps WTC-FR-09 and
/// WTC-FR-FBJQ exact for every switch from a root that is alive.
pub(crate) fn leave_gone_worktree_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
) -> Result<WorktreeContext, String> {
    let state = state_of_project(project)?;
    if state.state != ActiveRootHealth::Gone {
        return Err(ERR_WORKTREE_NOT_GONE.to_string());
    }
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "leaving a worktree that is gone",
        log_fields! { "primary" => state.primary_path.clone() },
    );
    let target = content_root_in(Path::new(&state.primary_path), &project.root_offset());
    reroot(app, project, watcher, store, &target)
}
