//! The apply of a pull run (`GRD-graduation.md` GRD-FR-PAPX, GRD-FR-PRBZ,
//! GRD-FR-XHSE).
//!
//! Each test applies a tree it settled itself, which is what a `ready` review
//! of the run's merge worktree hands the apply.

use super::pull_support::*;
use super::*;
use crate::git::PullStrategy;

/// A conflict on `a.md`: the local branch and the upstream both rewrote its
/// middle line. Returns `(local, remote, settled tree)`, where the settled tree
/// is the local tree with `a.md` holding the reconciled text.
fn conflicted(fx: &Fixture) -> (git2::Oid, git2::Oid, git2::Oid) {
    local_commit(fx, &[("a.md", "one\ntwo\nthree\n")], "base");
    track_origin(fx);
    let local = local_commit(fx, &[("a.md", "one\nLOCAL\nthree\n")], "local work");
    let remote = upstream_commit(fx, &[("a.md", "one\nREMOTE\nthree\n")], "upstream work");
    let repo = fx.repo();
    let local_tree = repo.find_commit(local).unwrap().tree_id();
    let settled = tree_with(&repo, local_tree, &[("a.md", "one\nBOTH\nthree\n")]);
    (local, remote, settled)
}

// GRD-FR-PAPX, GRD-FR-AQNW: a merge pull run lands one merge commit of the
// settled tree, whose parents are the pinned local and upstream tips.
#[test]
fn a_merge_pull_run_lands_a_merge_commit_of_the_settled_tree() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);

    let applied = apply_pull_run(&fx.app, &data, settled).expect("applied");

    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(applied.commit, Some(head.id().to_string()));
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![local, remote]);
    assert_eq!(head.tree_id(), settled);
    assert_eq!(read(&fx, "a.md"), "one\nBOTH\nthree\n");
    assert_eq!(applied.merged_paths, vec!["a.md".to_string()]);
}

// GRD-FR-PRBZ: a rebase pull run replays the local commit onto the upstream,
// answers the conflicting step from the settled tree, and lands linear history.
#[test]
fn a_rebase_pull_run_replays_onto_the_upstream_from_the_settled_tree() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Rebase);

    let applied = apply_pull_run(&fx.app, &data, settled).expect("applied");

    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(applied.commit, Some(head.id().to_string()));
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![remote]);
    assert_eq!(head.message().ok(), Some("local work"));
    assert_eq!(read(&fx, "a.md"), "one\nBOTH\nthree\n");
}

// GRD-FR-PRBZ: a settled tree the replay cannot reproduce is refused as
// unsettled, and nothing is written.
#[test]
fn a_settled_tree_the_replay_cannot_reproduce_is_unsettled() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let repo = fx.repo();
    // The run also wrote a path no replay step asks about.
    let settled = tree_with(&repo, settled, &[("extra.md", "the agent's own\n")]);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Rebase);
    let before = refs_of(&fx);

    let failure = apply_pull_run(&fx.app, &data, settled).unwrap_err();

    assert_eq!(failure, MergeApplyFailure::Unsettled(vec!["extra.md".to_string()]));
    assert_eq!(refs_of(&fx), before);
    assert_eq!(read(&fx, "a.md"), "one\nLOCAL\nthree\n");
}

// GRL-FR-NDHW, GRD-FR-PAPX: a marker left in a path Git could not merge is
// never applied.
#[test]
fn a_marker_left_in_an_unresolved_path_is_not_applied() {
    let fx = Fixture::new();
    let (local, remote, _) = conflicted(&fx);
    let repo = fx.repo();
    let local_tree = repo.find_commit(local).unwrap().tree_id();
    let marked = tree_with(
        &repo,
        local_tree,
        &[("a.md", "<<<<<<< ours\nLOCAL\n=======\nREMOTE\n>>>>>>> theirs\n")],
    );
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);

    let failure = apply_pull_run(&fx.app, &data, marked).unwrap_err();

    assert_eq!(failure, MergeApplyFailure::MarkersRemaining(vec!["a.md".to_string()]));
    assert_eq!(head_of(&fx), local);
}

// GRD-FR-PAPX: a worktree that no longer holds the local branch ends the apply
// with the target changed.
#[test]
fn a_worktree_that_left_the_branch_is_a_changed_target() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();

    let failure = apply_pull_run(&fx.app, &data, settled).unwrap_err();

    assert_eq!(failure, MergeApplyFailure::TargetChanged);
}

// GRD-FR-PAPX, GTC-FR-VOXB: the apply lands over uncommitted work it does not
// change, and is refused as a dirty side over work it would overwrite.
#[test]
fn the_apply_keeps_uncommitted_work_it_does_not_change() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);
    std::fs::write(fx.root().join("a.md"), "mine\n").unwrap();

    let failure = apply_pull_run(&fx.app, &data, settled).unwrap_err();
    assert!(
        matches!(&failure, MergeApplyFailure::DirtySide(reason) if reason.starts_with(crate::git::ERR_PULL_WOULD_OVERWRITE)),
        "{failure:?}"
    );
    assert_eq!(head_of(&fx), local);

    std::fs::write(fx.root().join("a.md"), "one\nLOCAL\nthree\n").unwrap();
    std::fs::write(fx.root().join("README.md"), "edited\n").unwrap();
    apply_pull_run(&fx.app, &data, settled).expect("applied");
    assert_eq!(read(&fx, "README.md"), "edited\n");
    assert_eq!(read(&fx, "a.md"), "one\nBOTH\nthree\n");
}

// GRD-FR-XHSE: a pull run pins its local branch alone. A later fetch that moves
// the upstream does not stop it; a local branch that moved does.
#[test]
fn a_pull_run_pins_its_local_branch_alone() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);
    upstream_commit(&fx, &[("z.md", "later\n")], "a later upstream commit");
    assert!(!merge_tips_moved(&fx.app, &data));

    local_commit(&fx, &[("c.md", "more\n")], "more local work");
    assert!(merge_tips_moved(&fx.app, &data));
    let failure = apply_pull_run(&fx.app, &data, settled).unwrap_err();
    assert_eq!(failure, MergeApplyFailure::BranchMoved);
}

// GRD-FR-JSBE, WKS-FR-HLGN: the apply is refused while another reconciliation
// holds the repository update guard.
#[test]
fn the_apply_is_refused_while_the_guard_is_held() {
    let fx = Fixture::new();
    let (local, remote, settled) = conflicted(&fx);
    let data = pull_data(&fx, local, remote, &["a.md"], PullStrategy::Merge);
    let state = fx.app.state::<StreamState>();
    let _held = state.begin_repository_update("other", Reconciliation::Update).unwrap();

    let failure = apply_pull_run(&fx.app, &data, settled).unwrap_err();

    assert_eq!(failure, MergeApplyFailure::GuardHeld(ERR_UPDATE_IN_PROGRESS.to_string()));
}
