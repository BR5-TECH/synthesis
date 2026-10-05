//! Removing a stream, and the refusals that protect one.

use super::*;

// WKS-FR-EIBC / WKS-FR-OVLQ — deletion
// ---------------------------------------------------------------------------

// WKS-FR-EIBC: deleting removes the working copy, the branch and the record.
#[test]
fn deleting_a_merged_stream_removes_its_branch_and_its_working_copy() {
    let fx = Fixture::new();
    let stream = fx.create("spent", None).expect("created");
    let path = stream.worktree_path.clone();

    delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect("deleted");

    let repo = fx.repo();
    assert!(!branch_names(&repo).contains(&stream.branch));
    assert!(worktree_names(&repo).is_empty());
    assert!(!Path::new(&path).exists());
    assert_eq!(
        get_work_stream(fx.app.clone(), stream.id).expect_err("gone"),
        ERR_UNKNOWN_STREAM
    );
}

// WKS-FR-EIBC: a stream carrying commits its base does not hold is refused
// without force, and the refusal names how many.
#[test]
fn deleting_an_unmerged_stream_is_refused_and_names_the_count() {
    let fx = Fixture::new();
    let stream = fx.create("ahead", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(Path::new(&stream.worktree_path).join("new.txt"), "work\n").unwrap();
    commit_all(&worktree, "stream work");

    let refusal =
        delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect_err("refused");
    assert!(refusal.starts_with(ERR_STREAM_UNMERGED), "{refusal}");
    assert!(refusal.contains('1'), "the count is named: {refusal}");

    // Nothing was removed.
    let repo = fx.repo();
    assert!(branch_names(&repo).contains(&stream.branch));
    assert!(Path::new(&stream.worktree_path).is_dir());
}

// WKS-FR-EIBC: force deletes a stream the refusal would have kept.
#[test]
fn force_deletes_an_unmerged_stream() {
    let fx = Fixture::new();
    let stream = fx.create("forced", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(Path::new(&stream.worktree_path).join("new.txt"), "work\n").unwrap();
    commit_all(&worktree, "stream work");

    delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).expect("deleted");
    let repo = fx.repo();
    assert!(!branch_names(&repo).contains(&stream.branch));
}

// WKS-FR-OVLQ: a stream a run holds is refused for deletion, and nothing is
// removed.
#[test]
fn deleting_a_busy_stream_is_refused() {
    let fx = Fixture::new();
    let mut stream = fx.create("held", None).expect("created");
    stream.busy_run_id = Some("g1".into());
    let fs = fx
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("instance");
    let store = StreamStore::new(canonical(fx.store.path()));
    store::write_stream(&fs, &store, &stream).expect("written");

    assert_eq!(
        delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect_err("refused"),
        ERR_STREAM_BUSY
    );
    let repo = fx.repo();
    assert!(branch_names(&repo).contains(&stream.branch));
    assert!(Path::new(&stream.worktree_path).is_dir());
}

// ---------------------------------------------------------------------------
