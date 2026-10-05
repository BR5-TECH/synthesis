//! Which stream owns a branch or a working copy (WKS-FR-HLYM, WTC-FR-RDVK,
//! GTC-FR-JOWX).
//!
//! The Git panel and the worktree removal read the stream store through these,
//! so the question "is this a stream's" has one answer for every caller.

use super::*;

/// WKS-FR-HLYM: the stream whose branch is `branch`, if one exists.
///
/// A stream whose working copy has gone still owns its branch, so the answer
/// does not depend on a worktree being present.
pub fn stream_on_branch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    branch: &str,
) -> Option<WorkStream> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    let key = project_key_of(app);
    store::list_streams(&fs, &store, &key)
        .into_iter()
        .find(|stream| stream.branch == branch)
}

/// WTC-FR-RDVK: the stream whose working copy is the directory at `path`, if
/// one exists.
///
/// Both sides are compared in resolved form, because a worktree listing reports
/// resolved paths and a stream's record holds the path it was created at.
pub fn stream_at_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    path: &Path,
) -> Option<WorkStream> {
    let store = store_for(app)?;
    let fs = access_for(app)?;
    let key = project_key_of(app);
    let wanted = crate::changes::canonicalize_lenient(path);
    store::list_streams(&fs, &store, &key)
        .into_iter()
        .find(|stream| crate::changes::canonicalize_lenient(&stream.worktree()) == wanted)
}
