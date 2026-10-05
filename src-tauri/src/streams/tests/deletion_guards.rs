//! The guards of a stream deletion beyond the unmerged one: the active
//! working copy, the uncommitted paths, and the read a confirmation names them
//! from (WKS-FR-EIBC, WKS-FR-OVLQ, WKS-FR-DBXN, WKS-FR-GQSU, WKS-FR-HLYM).

use super::*;

/// Write a file in the stream's working copy, which stays uncommitted.
fn dirty(stream: &WorkStream, rel: &str) {
    let path = Path::new(&stream.worktree_path).join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, "uncommitted\n").unwrap();
}

/// Commit a file on the stream's branch, so the stream is ahead of its base.
fn commit_in(stream: &WorkStream, rel: &str) {
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(Path::new(&stream.worktree_path).join(rel), "work\n").unwrap();
    commit_all(&worktree, "stream work");
}

fn assert_intact(fx: &Fixture, stream: &WorkStream) {
    assert!(branch_names(&fx.repo()).contains(&stream.branch));
    assert!(Path::new(&stream.worktree_path).is_dir());
    assert!(get_work_stream(fx.app.clone(), stream.id.clone()).is_ok());
}

// ---------------------------------------------------------------------------
// WKS-FR-OVLQ — a stream the project is standing in
// ---------------------------------------------------------------------------

// WKS-FR-OVLQ
#[test]
fn deleting_the_stream_whose_working_copy_is_active_is_refused_whatever_the_flags() {
    let fx = Fixture::new();
    let stream = fx.create("standing", None).expect("created");
    commit_in(&stream, "ahead.txt");
    dirty(&stream, "scratch.txt");
    fx.app
        .state::<crate::project::ProjectState>()
        .set_root(canonical(Path::new(&stream.worktree_path)));

    for (force, discard) in [(false, false), (true, false), (false, true), (true, true)] {
        assert_eq!(
            delete_work_stream(fx.app.clone(), stream.id.clone(), force, discard)
                .expect_err("refused"),
            ERR_STREAM_ACTIVE,
            "force {force}, discard {discard}",
        );
    }
    assert_intact(&fx, &stream);
}

// WKS-FR-OVLQ
#[test]
fn a_stream_the_project_is_not_standing_in_is_not_active() {
    let fx = Fixture::new();
    let stream = fx.create("elsewhere", None).expect("created");

    delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect("deleted");
}

// WKS-FR-OVLQ, WKS-FR-DBXN
#[test]
fn discarding_uncommitted_paths_does_not_answer_the_busy_refusal() {
    let fx = Fixture::new();
    let mut stream = fx.create("held", None).expect("created");
    dirty(&stream, "scratch.txt");
    stream.busy_run_id = Some("g1".into());
    let fs = fx.app.state::<crate::fs::FsAccessState>().get().expect("instance");
    let store = StreamStore::new(canonical(fx.store.path()));
    store::write_stream(&fs, &store, &stream).expect("written");

    for (force, discard) in [(true, true), (false, true)] {
        assert_eq!(
            delete_work_stream(fx.app.clone(), stream.id.clone(), force, discard)
                .expect_err("refused"),
            ERR_STREAM_BUSY
        );
    }
    assert_intact(&fx, &stream);
}

// ---------------------------------------------------------------------------
// WKS-FR-DBXN — uncommitted paths
// ---------------------------------------------------------------------------

// WKS-FR-DBXN
#[test]
fn a_stream_with_uncommitted_paths_is_refused_with_every_path_and_nothing_is_removed() {
    let fx = Fixture::new();
    let stream = fx.create("messy", None).expect("created");
    std::fs::write(Path::new(&stream.worktree_path).join("README.md"), "edited\n").unwrap();
    dirty(&stream, "scratch.txt");
    dirty(&stream, "nested/deep.txt");

    let refusal =
        delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect_err("refused");

    assert_eq!(
        refusal,
        format!("{ERR_STREAM_DIRTY}: README.md, nested/deep.txt, scratch.txt")
    );
    assert_intact(&fx, &stream);
}

// WKS-FR-DBXN
#[test]
fn force_does_not_answer_the_dirty_refusal() {
    let fx = Fixture::new();
    let stream = fx.create("messy", None).expect("created");
    dirty(&stream, "scratch.txt");

    let refusal =
        delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).expect_err("refused");

    assert!(refusal.starts_with(ERR_STREAM_DIRTY), "{refusal}");
    assert_intact(&fx, &stream);
}

// WKS-FR-DBXN
#[test]
fn discarding_uncommitted_paths_deletes_the_stream_with_them() {
    let fx = Fixture::new();
    let stream = fx.create("discarded", None).expect("created");
    std::fs::write(Path::new(&stream.worktree_path).join("README.md"), "edited\n").unwrap();
    dirty(&stream, "scratch.txt");

    delete_work_stream(fx.app.clone(), stream.id.clone(), false, true).expect("deleted");

    assert!(!branch_names(&fx.repo()).contains(&stream.branch));
    assert!(worktree_names(&fx.repo()).is_empty());
    assert!(!Path::new(&stream.worktree_path).exists());
    assert_eq!(
        get_work_stream(fx.app.clone(), stream.id).expect_err("gone"),
        ERR_UNKNOWN_STREAM
    );
}

// WKS-FR-EIBC, WKS-FR-DBXN
#[test]
fn the_unmerged_refusal_comes_before_the_dirty_one_and_each_flag_answers_its_own() {
    let fx = Fixture::new();
    let stream = fx.create("both", None).expect("created");
    commit_in(&stream, "ahead.txt");
    dirty(&stream, "scratch.txt");

    let unmerged =
        delete_work_stream(fx.app.clone(), stream.id.clone(), false, true).expect_err("refused");
    assert!(unmerged.starts_with(ERR_STREAM_UNMERGED), "{unmerged}");

    let dirty_refusal =
        delete_work_stream(fx.app.clone(), stream.id.clone(), true, false).expect_err("refused");
    assert!(dirty_refusal.starts_with(ERR_STREAM_DIRTY), "{dirty_refusal}");
    assert_intact(&fx, &stream);

    delete_work_stream(fx.app.clone(), stream.id.clone(), true, true).expect("deleted");
    assert!(!branch_names(&fx.repo()).contains(&stream.branch));
}

// WKS-FR-DBXN, WKS-FR-JVLM
#[test]
fn an_untracked_path_of_application_storage_is_not_uncommitted_work() {
    let fx = Fixture::new();
    let stream = fx.create("storage", None).expect("created");
    dirty(&stream, ".synthesis/drafts/note.md");

    assert_eq!(
        get_work_stream_uncommitted_paths(fx.app.clone(), stream.id.clone()).expect("read"),
        Vec::<String>::new()
    );
    delete_work_stream(fx.app.clone(), stream.id, false, false).expect("deleted");
}

// WKS-FR-DBXN, WKS-FR-GQSU
#[test]
fn a_stream_whose_working_copy_is_missing_reports_no_path_and_is_deleted() {
    let fx = Fixture::new();
    let stream = fx.create("vanished", None).expect("created");
    std::fs::remove_dir_all(&stream.worktree_path).unwrap();

    assert_eq!(
        get_work_stream_uncommitted_paths(fx.app.clone(), stream.id.clone()).expect("read"),
        Vec::<String>::new()
    );
    delete_work_stream(fx.app.clone(), stream.id.clone(), false, false).expect("deleted");

    assert!(!branch_names(&fx.repo()).contains(&stream.branch));
    assert!(worktree_names(&fx.repo()).is_empty());
}

// ---------------------------------------------------------------------------
// WKS-FR-GQSU — the read a confirmation names the paths from
// ---------------------------------------------------------------------------

// WKS-FR-GQSU
#[test]
fn the_uncommitted_paths_of_a_stream_are_the_complete_sorted_set_and_nothing_is_written() {
    let fx = Fixture::new();
    let stream = fx.create("reading", None).expect("created");
    assert_eq!(
        get_work_stream_uncommitted_paths(fx.app.clone(), stream.id.clone()).expect("read"),
        Vec::<String>::new(),
        "a clean working copy"
    );
    std::fs::write(Path::new(&stream.worktree_path).join("README.md"), "edited\n").unwrap();
    dirty(&stream, "b.txt");
    dirty(&stream, "dir/a.txt");
    let before = files_under(Path::new(&stream.worktree_path));

    let paths =
        get_work_stream_uncommitted_paths(fx.app.clone(), stream.id.clone()).expect("read");

    assert_eq!(paths, vec!["README.md", "b.txt", "dir/a.txt"]);
    assert_eq!(before, files_under(Path::new(&stream.worktree_path)));
    assert_intact(&fx, &stream);
}

// WKS-FR-GQSU
#[test]
fn the_uncommitted_paths_of_an_unknown_stream_are_the_typed_error() {
    let fx = Fixture::new();

    assert_eq!(
        get_work_stream_uncommitted_paths(fx.app.clone(), "w-nothing".into()).expect_err("refused"),
        ERR_UNKNOWN_STREAM
    );
}

// ---------------------------------------------------------------------------
// WKS-FR-HLYM, WTC-FR-RDVK — the other routes to a stream's branch and tree
// ---------------------------------------------------------------------------

// WKS-FR-HLYM, GTC-FR-JOWX
#[test]
fn a_stream_is_found_by_its_branch_and_by_its_working_copy() {
    let fx = Fixture::new();
    let stream = fx.create("findable", None).expect("created");

    assert_eq!(stream_on_branch(&fx.app, &stream.branch).map(|s| s.id), Some(stream.id.clone()));
    assert_eq!(
        stream_at_path(&fx.app, Path::new(&stream.worktree_path)).map(|s| s.id),
        Some(stream.id.clone())
    );
    assert!(stream_on_branch(&fx.app, "main").is_none());
    assert!(stream_at_path(&fx.app, &fx.root()).is_none());
}

// WTC-FR-RDVK
#[test]
fn a_streams_working_copy_is_not_removable_as_a_linked_worktree() {
    let fx = Fixture::new();
    let stream = fx.create("kept", None).expect("created");
    let before = files_under(Path::new(&stream.worktree_path));

    let refused = crate::worktree::remove_linked_worktree(
        &fx.app,
        &fx.root(),
        Path::new(&stream.worktree_path),
    )
    .expect_err("refused");

    assert_eq!(refused, crate::worktree::ERR_WORKTREE_BELONGS_TO_WORK_STREAM);
    assert_intact(&fx, &stream);
    assert_eq!(before, files_under(Path::new(&stream.worktree_path)));
}
