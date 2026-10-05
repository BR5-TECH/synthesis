//! The apply step of a merge run (`GRB-graduation-rebase.md` GRB-FR-CWLD,
//! GRB-FR-ZEPB, GRB-FR-HUTP, GRB-FR-NSAY, GRB-FR-ASWC, `WKS-work-streams.md`
//! WKS-FR-HLGN).

use super::merge_support::*;
use super::*;

const RUN: &str = "gapply1";

/// A conflicting stream, its snapshot, the data a run would carry and the tree
/// a settled merge worktree holds.
struct Case {
    stream: WorkStream,
    data: crate::graduation::GraduationMergeData,
    tree: git2::Oid,
}

fn case(fx: &Fixture, publication: StreamMergePublication) -> Case {
    let stream = stream_with_three_conflicts(fx);
    let snapshot = capture_snapshot(fx, &stream, RUN);
    let data = merge_data_for(fx, &stream, &snapshot, publication);
    let tree = settled_tree(
        &fx.repo(),
        snapshot.commit,
        &[
            ("README.md", Some("settled readme\n")),
            ("both-new.txt", Some("settled new\n")),
            // The base deleted it and the settled merge keeps it deleted.
            ("gone.txt", None),
        ],
    );
    Case { stream, data, tree }
}

fn apply(fx: &Fixture, case: &Case) -> Result<MergeApplied, MergeApplyFailure> {
    apply_merge_run(&fx.app, &case.stream.id, &case.data, case.tree)
}

fn refused_and_unchanged(fx: &Fixture, case: &Case) -> MergeApplyFailure {
    let before = observe(fx, &case.stream);
    let failure = apply(fx, case).expect_err("refused");
    assert_eq!(observe(fx, &case.stream), before, "the refusal wrote something");
    failure
}

// GRB-FR-CWLD, GRB-FR-ZEPB, GRB-FR-ASWC, GRB-FR-ITCJ, WKS-FR-HLGN: the `uncommitted`
// publication writes the settled tree unstaged into the base worktree and moves
// no branch.
#[test]
fn an_uncommitted_apply_writes_the_settled_tree_unstaged() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    let base_before = tip_text(&fx.repo(), &case.stream.base_branch);
    let stream_before = tip_text(&fx.repo(), &case.stream.branch);

    let applied = apply(&fx, &case).expect("applied");

    assert_eq!(applied.commit, None);
    assert_eq!(
        applied.merged_paths,
        vec!["README.md", "both-new.txt", "stream-only.txt"],
        "the paths the result changes against the base tip"
    );
    let repo = fx.repo();
    assert_eq!(tip_text(&repo, &case.stream.base_branch), base_before);
    assert_eq!(tip_text(&repo, &case.stream.branch), stream_before);
    let read = |path: &str| std::fs::read_to_string(fx.root().join(path)).unwrap();
    assert_eq!(read("README.md"), "settled readme\n");
    assert_eq!(read("both-new.txt"), "settled new\n");
    assert_eq!(read("stream-only.txt"), "kept\n");
    assert_eq!(read("base-only.txt"), "from the base\n");
    assert!(!fx.root().join("gone.txt").exists());
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    let status_of = |path: &str| {
        statuses
            .iter()
            .find(|entry| entry.path() == Ok(path))
            .unwrap_or_else(|| panic!("{path} has no status"))
            .status()
    };
    assert!(status_of("README.md").is_wt_modified());
    assert!(!status_of("README.md").is_index_modified(), "nothing is staged");
    assert!(status_of("stream-only.txt").is_wt_new());
    assert!(!status_of("stream-only.txt").is_index_new());
}

// GRB-FR-CWLD, GRB-FR-ZEPB, GRB-FR-ASWC, GRD-FR-AQNW: the `commit` publication
// makes one commit that holds the settled tree, has the pinned base tip and the
// pinned stream tip as parents, and carries the author's message.
#[test]
fn a_committed_apply_makes_one_merge_commit_of_the_settled_tree() {
    let fx = Fixture::new();
    let case = case(
        &fx,
        StreamMergePublication::Commit {
            message: "Land the settled merge".into(),
        },
    );
    let base_before = tip_of(&fx.repo(), &case.stream.base_branch);
    let stream_before = tip_of(&fx.repo(), &case.stream.branch);

    let applied = apply(&fx, &case).expect("applied");

    let repo = fx.repo();
    let head = repo
        .find_branch(&case.stream.base_branch, git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    assert_eq!(applied.commit, Some(head.id().to_string()));
    assert_eq!(head.message().unwrap(), "Land the settled merge");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![base_before, stream_before]);
    assert_eq!(head.tree_id(), case.tree);
    assert_eq!(tip_of(&repo, &case.stream.branch), stream_before);
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    assert_eq!(
        repo.statuses(Some(&mut options)).unwrap().len(),
        0,
        "the base worktree is clean after the commit"
    );
    assert_eq!(
        std::fs::read_to_string(fx.root().join("README.md")).unwrap(),
        "settled readme\n"
    );
}

// GRB-FR-CWLD, GRB-FR-ZEPB, GRB-FR-BXEJ, WKS-FR-HLGN: a base tip that moved
// since handoff ends the apply with `BranchMoved` and writes nothing.
#[test]
fn an_apply_over_a_moved_base_branch_writes_nothing() {
    for publication in [
        StreamMergePublication::Uncommitted,
        StreamMergePublication::Commit {
            message: "never lands".into(),
        },
    ] {
        let fx = Fixture::new();
        let case = case(&fx, publication);
        commit_file(&fx.repo(), "later.txt", "later\n", "the base moves");

        assert_eq!(refused_and_unchanged(&fx, &case), MergeApplyFailure::BranchMoved);
    }
}

// GRB-FR-CWLD, GRB-FR-BXEJ: a stream tip that moved ends the apply too.
#[test]
fn an_apply_over_a_moved_stream_branch_writes_nothing() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    commit_file(&stream_repo(&case.stream), "later.txt", "later\n", "the stream moves");

    assert_eq!(refused_and_unchanged(&fx, &case), MergeApplyFailure::BranchMoved);
}

// GRB-FR-NSAY, GRL-FR-NDHW: a conflict marker left in an unresolved path stops
// the apply with the paths named, and nothing is written.
#[test]
fn a_marker_left_in_an_unresolved_path_stops_the_apply() {
    let fx = Fixture::new();
    let mut case = case(&fx, StreamMergePublication::Uncommitted);
    case.tree = settled_tree(
        &fx.repo(),
        git2::Oid::from_str(&case.data.snapshot_commit).unwrap(),
        &[
            ("README.md", Some("settled\n<<<<<<< the base branch\nleft behind\n")),
            ("both-new.txt", Some("fine\n")),
            ("gone.txt", Some("kept\n>>>>>>> the stream\n")),
        ],
    );

    assert_eq!(
        refused_and_unchanged(&fx, &case),
        MergeApplyFailure::MarkersRemaining(vec!["README.md".to_string(), "gone.txt".to_string()])
    );
}

// GRB-FR-NSAY: the marker check reads the unresolved paths alone, so a file
// that merely quotes a marker elsewhere does not stop the apply.
#[test]
fn a_marker_in_a_path_git_merged_cleanly_does_not_stop_the_apply() {
    let fx = Fixture::new();
    let mut case = case(&fx, StreamMergePublication::Uncommitted);
    case.tree = settled_tree(
        &fx.repo(),
        git2::Oid::from_str(&case.data.snapshot_commit).unwrap(),
        &[
            ("README.md", Some("settled\n")),
            ("both-new.txt", Some("settled\n")),
            ("gone.txt", None),
            ("stream-only.txt", Some("<<<<<<< quoted in a document\n")),
        ],
    );

    apply(&fx, &case).expect("applied");
}

// GRB-FR-HUTP, GRB-FR-CWLD, WKS-FR-HLGN, WKS-FR-TSOA: the apply takes the
// repository update guard, and a held guard refuses it with the holder's reason
// and writes nothing. The guard is free again once the apply returns.
#[test]
fn an_apply_while_the_guard_is_held_is_refused_and_the_guard_is_released_after() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    let state = fx.app.state::<StreamState>();
    let hold = state
        .begin_repository_update(&case.stream.id, Reconciliation::Update)
        .expect("the guard is free");

    assert_eq!(
        refused_and_unchanged(&fx, &case),
        MergeApplyFailure::GuardHeld(ERR_UPDATE_IN_PROGRESS.to_string())
    );
    drop(hold);

    apply(&fx, &case).expect("applied once the guard is free");
    assert!(
        state
            .begin_repository_update(&case.stream.id, Reconciliation::Merge)
            .is_ok(),
        "the apply released the guard"
    );
}

// GRB-FR-CWLD, GRB-FR-HUTP, WKS-FR-UZHT: a dirty base worktree or a dirty stream
// refuses the apply as a dirty side, with the paths, and writes nothing.
#[test]
fn an_apply_over_a_dirty_side_is_refused_with_the_paths() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    std::fs::write(fx.root().join("mine.txt"), "mine\n").unwrap();
    match refused_and_unchanged(&fx, &case) {
        MergeApplyFailure::DirtySide(reason) => {
            assert!(reason.starts_with(ERR_BASE_DIRTY), "{reason}");
            assert!(reason.contains("mine.txt"), "{reason}");
        }
        other => panic!("expected a dirty side, got {other:?}"),
    }
    std::fs::remove_file(fx.root().join("mine.txt")).unwrap();

    std::fs::write(Path::new(&case.stream.worktree_path).join("scratch.txt"), "s\n").unwrap();
    match refused_and_unchanged(&fx, &case) {
        MergeApplyFailure::DirtySide(reason) => {
            assert!(reason.starts_with(ERR_STREAM_DIRTY), "{reason}");
            assert!(reason.contains("scratch.txt"), "{reason}");
        }
        other => panic!("expected a dirty side, got {other:?}"),
    }
}

// GRB-FR-HUTP, GRB-FR-CWLD: a stream whose working copy is gone is a side the
// apply cannot trust, and writes nothing.
#[test]
fn an_apply_for_a_stream_whose_working_copy_is_gone_is_refused() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    std::fs::remove_dir_all(&case.stream.worktree_path).unwrap();

    assert_eq!(
        refused_and_unchanged(&fx, &case),
        MergeApplyFailure::DirtySide(ERR_STREAM_MISSING.to_string())
    );
}

// GRB-FR-HUTP, GRB-FR-ASWC: an `uncommitted` apply with no worktree on the base
// branch fails as an apply failure and writes nothing, rather than reporting a
// merge that did not happen.
#[test]
fn an_uncommitted_apply_with_no_base_worktree_fails_and_writes_nothing() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();

    assert_eq!(
        refused_and_unchanged(&fx, &case),
        MergeApplyFailure::Failed(ERR_BASE_NOT_CHECKED_OUT.to_string())
    );
}

// GRB-FR-HUTP: a stream the store does not hold is a failed apply with the
// typed reason, and writes nothing.
#[test]
fn an_apply_for_an_unknown_stream_fails() {
    let fx = Fixture::new();
    let case = case(&fx, StreamMergePublication::Uncommitted);
    let before = observe(&fx, &case.stream);

    let failure =
        apply_merge_run(&fx.app, "w-no-such-stream", &case.data, case.tree).expect_err("refused");

    assert_eq!(failure, MergeApplyFailure::Failed(ERR_UNKNOWN_STREAM.to_string()));
    assert_eq!(observe(&fx, &case.stream), before);
}

// GRB-FR-ASWC, GRB-FR-HUTP: a `commit` apply with no worktree on the base branch
// moves the branch ref alone. The commit holds the settled tree with both pinned
// tips as parents, and the worktree that stands on another branch is not written.
#[test]
fn a_commit_apply_with_no_base_worktree_moves_the_ref_and_writes_no_worktree() {
    let fx = Fixture::new();
    let case = case(
        &fx,
        StreamMergePublication::Commit {
            message: "Land it".into(),
        },
    );
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    let elsewhere_before = tip_text(&repo, "elsewhere");
    let observed_before = observe(&fx, &case.stream);
    let base_tip = tip_of(&repo, &case.stream.base_branch);
    let stream_tip = tip_of(&repo, &case.stream.branch);

    let applied = apply(&fx, &case).expect("applied");

    let repo = fx.repo();
    let landed = repo
        .find_branch(&case.stream.base_branch, git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    assert_eq!(applied.commit, Some(landed.id().to_string()));
    assert_eq!(landed.message().unwrap(), "Land it");
    assert_eq!(landed.parent_ids().collect::<Vec<_>>(), vec![base_tip, stream_tip]);
    assert_eq!(landed.tree_id(), case.tree, "the commit holds the settled tree");
    assert_eq!(tip_of(&repo, &case.stream.branch), stream_tip, "the stream branch stands");
    assert_eq!(tip_text(&repo, "elsewhere"), elsewhere_before);
    let after = observe(&fx, &case.stream);
    assert_eq!(after.base_files, observed_before.base_files, "the other worktree was written");
    assert_eq!(after.base_status, observed_before.base_status);
    assert_eq!(after.stream_files, observed_before.stream_files);
}
