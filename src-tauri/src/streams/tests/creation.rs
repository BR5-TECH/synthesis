//! Creating a stream: what the record holds, the base branch, the names, and
//! what a failure leaves behind.

use super::*;

// ---------------------------------------------------------------------------
// WKS-FR-QMTV, WKS-FR-KDXF, WKS-FR-JGCA — the record and what creation makes
// ---------------------------------------------------------------------------

// WKS-FR-QMTV / WKS-FR-KDXF: a created stream records what it is, and the
// branch and the working copy it names both exist.
#[test]
fn a_created_stream_records_its_branch_its_base_and_its_working_copy() {
    let fx = Fixture::new();
    let stream = fx.create("Editor work", None).expect("created");

    assert!(!stream.id.is_empty());
    assert_eq!(stream.name, "Editor work");
    assert_eq!(stream.branch, "synthesis/stream/editor-work");
    assert!(!stream.base_revision.is_empty());
    assert_eq!(stream.busy_run_id, None);
    assert!(!stream.is_missing);

    let repo = fx.repo();
    assert!(branch_names(&repo).contains(&stream.branch));
    assert_eq!(worktree_names(&repo).len(), 1);
    assert!(Path::new(&stream.worktree_path).is_dir());
}

// WKS-FR-JGCA: the working copy is `w/<stream-id>` and no deeper, because the
// container names the path the host names.
#[test]
fn the_working_copy_stands_directly_under_the_store_root() {
    let fx = Fixture::new();
    let stream = fx.create("docs pass", None).expect("created");
    let expected = canonical(fx.store.path()).join("w").join(&stream.id);
    assert_eq!(canonical(Path::new(&stream.worktree_path)), canonical(&expected));
}

// WKS-FR-QMTV: the record round-trips off disk, so a relaunch reads what the
// creation wrote.
#[test]
fn the_record_is_read_back_from_disk_unchanged() {
    let fx = Fixture::new();
    let created = fx.create("round trip", None).expect("created");
    let read = get_work_stream(fx.app.clone(), created.id.clone()).expect("read");
    assert_eq!(read.id, created.id);
    assert_eq!(read.branch, created.branch);
    assert_eq!(read.base_branch, created.base_branch);
    assert_eq!(read.base_revision, created.base_revision);
}

// WKS-FR-QMTV: the base revision is recorded once and never refreshed.
#[test]
fn the_base_revision_is_the_one_captured_at_creation() {
    let fx = Fixture::new();
    let stream = fx.create("pinned", None).expect("created");
    let before = stream.base_revision.clone();

    std::fs::write(fx.root().join("later.txt"), "later\n").unwrap();
    commit_all(&fx.repo(), "later");

    let read = get_work_stream(fx.app.clone(), stream.id).expect("read");
    assert_eq!(read.base_revision, before);
}

// WKS-FR-KDXF: the slug is path-safe and cut to 48 characters.
#[test]
fn a_name_is_reduced_to_a_path_safe_slug() {
    assert_eq!(slug_of("Editor work"), "editor-work");
    assert_eq!(slug_of("UPPER Case"), "upper-case");
    assert_eq!(slug_of("a//b"), "a-b");
    assert_eq!(slug_of("  spaced  "), "spaced");
    assert_eq!(slug_of("keep_under-score"), "keep_under-score");
    assert_eq!(slug_of("tabs\tand\nnewlines"), "tabs-and-newlines");
    assert_eq!(slug_of(""), "");
    assert_eq!(slug_of("!!!"), "");
}

// WKS-FR-KDXF: the cut is applied last, so a slug never ends in a separator.
#[test]
fn a_long_name_is_cut_to_the_slug_limit_without_a_trailing_separator() {
    let long = "a".repeat(200);
    assert_eq!(slug_of(&long).len(), 48);

    let straddling = format!("{} x", "b".repeat(47));
    let slug = slug_of(&straddling);
    assert!(!slug.ends_with('-'), "slug {slug:?} ends in a separator");
    assert!(slug.len() <= 48);
}

// WKS-FR-MFDW: a stream's branch is recognisable as one.
#[test]
fn a_stream_branch_is_told_from_an_ordinary_branch() {
    assert!(is_stream_branch("synthesis/stream/editor-work"));
    assert!(!is_stream_branch("main"));
    assert!(!is_stream_branch("feature/editor-work"));
}

// ---------------------------------------------------------------------------
// WKS-FR-PWNR — the base branch
// ---------------------------------------------------------------------------

// WKS-FR-PWNR: the active worktree's branch where the request names none.
#[test]
fn the_base_branch_defaults_to_the_branch_the_author_is_on() {
    let fx = Fixture::new();
    let expected = git::current_branch(&fx.root()).expect("a branch");
    let stream = fx.create("defaulted", None).expect("created");
    assert_eq!(stream.base_branch, expected);
}

// WKS-FR-PWNR: a detached active worktree with no base branch is refused, and
// the refusal costs no branch and no working copy.
#[test]
fn a_detached_head_with_no_base_branch_is_refused_and_creates_nothing() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(head).unwrap();

    let refusal = fx.create("detached", None).expect_err("refused");
    assert_eq!(refusal, ERR_BASE_BRANCH_REQUIRED);
    assert!(!branch_names(&repo).iter().any(|b| is_stream_branch(b)));
    assert!(worktree_names(&repo).is_empty());
}

// WKS-FR-PWNR: a detached worktree is workable once the request names a base.
#[test]
fn a_named_base_branch_is_used_even_when_the_head_is_detached() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let branch = git::current_branch(&fx.root()).expect("a branch");
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(head).unwrap();

    let stream = fx.create("named", Some(&branch)).expect("created");
    assert_eq!(stream.base_branch, branch);
}

// ---------------------------------------------------------------------------
// WKS-FR-HRUZ — names
// ---------------------------------------------------------------------------

// WKS-FR-HRUZ: a name no live stream holds is accepted; a duplicate is not.
#[test]
fn a_duplicate_name_is_refused_and_creates_nothing() {
    let fx = Fixture::new();
    fx.create("docs", None).expect("created");
    let refusal = fx.create("docs", None).expect_err("refused");
    assert_eq!(refusal, ERR_STREAM_NAME_TAKEN);

    let repo = fx.repo();
    assert_eq!(worktree_names(&repo).len(), 1);
}

// WKS-FR-HRUZ: two names that reduce to one slug are one name, because the
// branch they would own is the same branch.
#[test]
fn two_names_that_reduce_to_one_slug_collide() {
    let fx = Fixture::new();
    fx.create("my work", None).expect("created");
    for other in ["My Work", "my-work", "my/work"] {
        assert_eq!(
            fx.create(other, None).expect_err("refused"),
            ERR_STREAM_NAME_TAKEN,
            "{other:?} should collide with \"my work\"",
        );
    }
}

// WKS-FR-HRUZ: a name that reduces to nothing is refused.
#[test]
fn a_name_that_reduces_to_an_empty_slug_is_refused() {
    let fx = Fixture::new();
    for name in ["", "   ", "!!!", "---", "///"] {
        assert_eq!(
            fx.create(name, None).expect_err("refused"),
            ERR_STREAM_NAME_INVALID,
            "{name:?} should be invalid",
        );
    }
}

// WKS-FR-HRUZ: uniqueness is over live streams, so a deleted name is free.
#[test]
fn a_deleted_stream_frees_its_name() {
    let fx = Fixture::new();
    let stream = fx.create("reusable", None).expect("created");
    delete_work_stream(fx.app.clone(), stream.id, false, false).expect("deleted");
    fx.create("reusable", None).expect("created again");
}

// ---------------------------------------------------------------------------
// WKS-FR-ZBHL — a project that is not a repository
// ---------------------------------------------------------------------------

// WKS-FR-ZBHL: every operation refuses a project outside Git, and none of them
// writes anything.
#[test]
fn every_operation_refuses_a_project_that_is_not_a_repository() {
    let project = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let root = canonical(project.path());

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(crate::fs::FsAccessState::default());
    handle.manage(crate::project::ProjectState::default());
    handle.manage(StreamState::rooted_at(canonical(store.path())));
    handle
        .state::<crate::fs::FsAccessState>()
        .install_for_worktree_and(&root, &canonical(store.path()))
        .expect("two real directories");
    let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
    let state = handle.state::<crate::project::ProjectState>();
    state.set_root_with_access(root.clone(), Some(access));
    state.set_anchor(root.to_string_lossy().into_owned());

    // Every operation of the contract surface, not two of them: a project
    // outside Git is told that, whichever operation it asked for and whatever
    // stream id it named.
    assert_eq!(
        create_work_stream(handle.clone(), "x".into(), None).expect_err("refused"),
        ERR_NOT_A_GIT_REPOSITORY
    );
    assert_eq!(
        list_work_streams(handle.clone()).expect_err("refused"),
        ERR_NOT_A_GIT_REPOSITORY
    );
    assert_eq!(
        get_work_stream(handle.clone(), "w1".into()).expect_err("refused"),
        ERR_NOT_A_GIT_REPOSITORY
    );
    assert_eq!(
        delete_work_stream(handle.clone(), "w1".into(), false, false).expect_err("refused"),
        ERR_NOT_A_GIT_REPOSITORY
    );
    let refused = merge_work_stream_blocking(&handle, "w1", StreamMergePublication::Uncommitted)
        .expect_err("refused");
    assert_eq!(refused, ERR_NOT_A_GIT_REPOSITORY);
    assert!(!canonical(store.path()).join("w").exists());
}

// ---------------------------------------------------------------------------
// WKS-FR-VTEY — all or nothing
// ---------------------------------------------------------------------------

// WKS-FR-VTEY: a creation that cannot make its working copy leaves no branch,
// no registration and no directory behind.
#[test]
fn a_creation_that_fails_leaves_no_branch_and_no_working_copy() {
    let fx = Fixture::new();
    // A file where the working copy must go: the worktree cannot be created,
    // and the branch made just before it has to be reclaimed.
    let store_root = canonical(fx.store.path());
    std::fs::create_dir_all(store_root.join("w")).unwrap();
    let blocked = store_root.join("w");
    // Every stream id is unpredictable, so block the whole parent instead by
    // making it a file the creation cannot write under.
    std::fs::remove_dir_all(&blocked).unwrap();
    std::fs::write(&blocked, "not a directory\n").unwrap();

    let refusal = fx.create("blocked", None).expect_err("refused");
    assert!(
        refusal.starts_with(ERR_STREAM_CREATION_FAILED)
            || refusal.starts_with(ERR_STREAM_CLEANUP_FAILED),
        "unexpected refusal: {refusal}",
    );
    let repo = fx.repo();
    assert!(
        !branch_names(&repo).iter().any(|b| is_stream_branch(b)),
        "a failed creation left its branch behind",
    );
    assert!(
        worktree_names(&repo).is_empty(),
        "a failed creation left a worktree registration behind",
    );
}

// WKS-FR-VTEY: a failed creation leaves an earlier stream untouched.
#[test]
fn a_failed_creation_leaves_an_earlier_stream_alone() {
    let fx = Fixture::new();
    let first = fx.create("first", None).expect("created");

    let store_root = canonical(fx.store.path());
    let marker = store_root.join("w").join("blocker");
    std::fs::write(&marker, "x").unwrap();
    let _ = fx.create("first", None).expect_err("duplicate refused");

    let read = get_work_stream(fx.app.clone(), first.id.clone()).expect("still there");
    assert_eq!(read.branch, first.branch);
    assert!(Path::new(&read.worktree_path).is_dir());
}

// ---------------------------------------------------------------------------
// WKS-FR-BSLO — the shared store recorded relatively
// ---------------------------------------------------------------------------

// WKS-FR-BSLO: the worktree records the store it shares as a relative path, so
// a container that mounts the store elsewhere still resolves it.
#[test]
fn the_working_copy_records_its_store_relatively() {
    let fx = Fixture::new();
    let stream = fx.create("relative", None).expect("created");
    let commondir = fx
        .root()
        .join(".git")
        .join("worktrees")
        .join(&stream.id)
        .join("commondir");
    let recorded = std::fs::read_to_string(&commondir).expect("commondir");
    let recorded = recorded.trim();
    assert!(
        !Path::new(recorded).is_absolute(),
        "commondir must be relative, found {recorded:?}",
    );
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// WKS-FR-SMKU — nothing of a stream reaches the project
// ---------------------------------------------------------------------------

// WKS-FR-SMKU: a creation writes nothing into the project and commits nothing.
#[test]
fn a_creation_writes_nothing_into_the_project() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let before = repo.head().unwrap().peel_to_commit().unwrap().id();

    fx.create("clean", None).expect("created");

    let repo = fx.repo();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), before);
    assert!(!fx.root().join(".synthesis").exists());

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts)).expect("status");
    assert_eq!(statuses.len(), 0, "the project's worktree stayed clean");
}
