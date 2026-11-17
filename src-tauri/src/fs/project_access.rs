//! The filesystem state of one open project (FSA-FR-TSHC, FSA-FR-ZZNJ,
//! PWN-FR-ISOL).
//!
//! Each open project holds its own [`FsAccessState`] in its scope: its IDE
//! instance, its agent instances, and its documents instance. The value that
//! the application manages holds the pre-project instance alone, and serves no
//! project.
//!
//! Code that serves one project gets the state of the project of its context
//! (PWN-FR-CTXT). With two projects open and no context, it gets the state of
//! the default scope, which holds no instance, so the call refuses. It never
//! uses the instance of either project.

use std::sync::Arc;

use tauri::{Manager, Runtime};

use super::access::FsAccess;
use super::access_state::FsAccessState;
use crate::project_scope::{ProjectScopes, ProjectStores, Scoped};

/// FSA-FR-TSHC, FSA-FR-ZZNJ: the filesystem state of the project of the running
/// code.
pub fn project_fs_state<R: Runtime, M: Manager<R>>(app: &M) -> Option<Scoped<'_, FsAccessState>> {
    app.try_project_store::<FsAccessState>()
}

/// FSA-FR-TSHC: the filesystem state of the project with the id `project_id`,
/// for a command that names its project by its own key. It does not read the
/// context, so a command with no project id reaches the default scope and never
/// the one open project.
pub fn project_fs_state_of<'a, R: Runtime, M: Manager<R>>(
    app: &'a M,
    project_id: &str,
) -> Option<Scoped<'a, FsAccessState>> {
    match app.try_state::<ProjectScopes>() {
        Some(scopes) => {
            Some(Scoped::Owned(scopes.get::<FsAccessState>(project_id), project_id.to_string()))
        }
        None => app.try_state::<FsAccessState>().map(Scoped::Managed),
    }
}

/// FSA-FR-21, FSA-FR-ZZNJ: the IDE instance of the project of the running code.
/// With no such instance it is the pre-project instance, which reaches the two
/// application-owned roots and no worktree of any project.
pub fn ide_instance<R: Runtime, M: Manager<R>>(app: &M) -> Option<Arc<FsAccess>> {
    project_fs_state(app)
        .and_then(|state| state.get())
        .or_else(|| app.try_state::<FsAccessState>().and_then(|state| state.get()))
}
