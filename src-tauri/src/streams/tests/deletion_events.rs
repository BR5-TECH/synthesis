//! The branch announcement of a stream's creation and deletion, and a forced
//! deletion after a merge that left its result uncommitted.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tauri::Listener;

use super::merge_support::stream_ahead;
use super::*;

/// How many `"branches changed"` events the app delivers from now on.
fn count_branches_changed(fx: &Fixture) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&count);
    fx.app.listen(crate::worktree::BRANCHES_CHANGED, move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    count
}

// WKS-FR-SPVK: a created stream announces its new branch once.
#[test]
fn creating_a_stream_emits_branches_changed_once() {
    let fx = Fixture::new();
    let count = count_branches_changed(&fx);

    fx.create("announced", None).expect("created");

    assert_eq!(count.load(Ordering::SeqCst), 1);
}

// WKS-FR-SPVK: a refused creation emits nothing.
#[test]
fn a_refused_creation_emits_nothing() {
    let fx = Fixture::new();
    fx.create("taken", None).expect("created");
    let count = count_branches_changed(&fx);

    assert_eq!(fx.create("taken", None).expect_err("refused"), ERR_STREAM_NAME_TAKEN);

    assert_eq!(count.load(Ordering::SeqCst), 0);
}

// WKS-FR-SPVK: a deleted stream announces its removed branch once.
#[test]
fn deleting_a_stream_emits_branches_changed_once() {
    let fx = Fixture::new();
    let stream = fx.create("spent", None).expect("created");
    let count = count_branches_changed(&fx);

    delete_work_stream(fx.app.clone(), stream.id, false, false).expect("deleted");

    assert_eq!(count.load(Ordering::SeqCst), 1);
}

// WKS-FR-SPVK: a refused deletion emits nothing.
#[test]
fn a_refused_deletion_emits_nothing() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "kept");
    let count = count_branches_changed(&fx);

    let refusal =
        delete_work_stream(fx.app.clone(), stream.id, false, false).expect_err("refused");

    assert!(refusal.starts_with(ERR_STREAM_UNMERGED), "{refusal}");
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

// WKS-FR-EIBC / `WSS-work-stream-selector.md` WSS-FR-UFZP: a merge left
// uncommitted moves no branch, so the stream still counts as unmerged, and
// only `force` removes it.
#[test]
fn force_deletes_a_stream_whose_merge_was_left_uncommitted() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "merged-loose");
    merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");

    let refusal = delete_work_stream(fx.app.clone(), stream.id.clone(), false, false)
        .expect_err("still unmerged");
    assert!(refusal.starts_with(ERR_STREAM_UNMERGED), "{refusal}");

    delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).expect("deleted");
    let repo = fx.repo();
    assert!(!branch_names(&repo).contains(&stream.branch));
    assert!(!Path::new(&stream.worktree_path).exists());
}

/// Restores a directory's permissions when dropped, so a failed assertion
/// cannot leave a temporary directory the test runner cannot remove.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    fn new(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
        ReadOnly(path.to_path_buf())
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

// WKS-FR-SPVK: a deletion that returns `stream_cleanup_failed` still announces
// the branch set once, because part of it may have gone.
#[cfg(unix)]
#[test]
fn a_deletion_that_strands_part_of_the_stream_emits_branches_changed_once() {
    let fx = Fixture::new();
    let stream = fx.create("stuck", None).expect("created");
    let worktree = PathBuf::from(&stream.worktree_path);
    // The working copy's parent refuses removal of its entries.
    let _guard = ReadOnly::new(worktree.parent().expect("a parent"));
    let count = count_branches_changed(&fx);

    let error = delete_work_stream(fx.app.clone(), stream.id, false, false)
        .expect_err("part of the stream stays");

    assert!(error.starts_with(ERR_STREAM_CLEANUP_FAILED), "{error}");
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

// WTC-FR-25 / WKS-FR-SPVK: the announcement names the repository's primary
// worktree root.
#[test]
fn the_announcement_names_the_primary_worktree_root() {
    let fx = Fixture::new();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&seen);
    fx.app.listen(crate::worktree::BRANCHES_CHANGED, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });

    fx.create("named", None).expect("created");

    let payloads = seen.lock().unwrap().clone();
    assert_eq!(payloads.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
    let root = payload["repositoryRoot"].as_str().expect("a repository root");
    assert_eq!(
        crate::changes::canonicalize_lenient(Path::new(root)),
        crate::changes::canonicalize_lenient(&fx.root())
    );
}

