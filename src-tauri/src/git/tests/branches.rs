//! The branch listing and the checkout primitive (GTC-FR-07 / GTC-FR-08),
//! including the refusals a linked worktree causes.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Branches + checkout (GTC-FR-07 / GTC-FR-08)
// -----------------------------------------------------------------------

#[test]
fn list_branches_marks_the_active_worktrees_branch_current_and_carries_no_worktree_field() {
    // GTC-FR-07, GTC-FR-06.
    let f = WorktreeFixture::new();
    let main = f.current();
    f.branch("develop");
    f.remote_ref("origin/experiment");

    let listed = branches_for(&f.root()).unwrap();
    let current: Vec<&GitBranch> = listed.iter().filter(|b| b.is_current).collect();
    assert_eq!(current.len(), 1, "exactly one branch is current: {listed:?}");
    assert_eq!(current[0].name, main);
    assert!(listed.iter().any(|b| b.name == "develop" && b.kind == "local"));
    assert!(listed
        .iter()
        .any(|b| b.name == "origin/experiment" && b.kind == "remote"));

    // The wire shape carries no worktree association — that belongs to
    // WTC's richer listing.
    let json = serde_json::to_value(&listed[0]).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["isCurrent", "kind", "name"],
        "no worktree field leaks into the Git panel's branch listing"
    );
}

#[test]
fn list_branches_marks_nothing_current_on_a_detached_head() {
    let f = WorktreeFixture::new();
    f.branch("develop");
    let repo = f.repo();
    let id = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(id).unwrap();
    drop(repo);
    let listed = branches_for(&f.root()).unwrap();
    assert!(!listed.is_empty());
    assert!(listed.iter().all(|b| !b.is_current));
}

#[test]
fn list_branches_on_a_non_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    assert_eq!(branches_for(dir.path()).unwrap_err(), ERR_NOT_A_REPO);
}

#[test]
fn checkout_moves_the_active_worktree_onto_the_branch() {
    let f = WorktreeFixture::new();
    f.branch("develop");
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "develop").unwrap();
    assert_eq!(f.current(), "develop");
}

#[test]
fn checkout_of_a_remote_branch_creates_a_local_tracking_branch_first() {
    // WTC-FR-11, served through this primitive so the Git panel's remote
    // entries work the same way as the selector's.
    let f = WorktreeFixture::new();
    f.remote_ref("origin/experiment");
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "origin/experiment").unwrap();
    assert_eq!(f.current(), "experiment");
    let repo = f.repo();
    let local = repo.find_branch("experiment", BranchType::Local).unwrap();
    assert_eq!(
        local.upstream().unwrap().name().unwrap(),
        Some("origin/experiment"),
        "the local branch tracks the remote it came from"
    );
}

#[test]
fn checkout_of_an_unknown_branch_is_the_typed_error() {
    let f = WorktreeFixture::new();
    let before = f.current();
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "nope").unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "   ").unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );
    assert_eq!(f.current(), before);
}

#[test]
fn checkout_of_a_branch_held_by_another_worktree_is_refused_and_changes_nothing() {
    // GTC-FR-08 / WTC-FR-12: Git cannot have one branch in two worktrees.
    let f = WorktreeFixture::new();
    f.branch("develop");
    crate::worktree::create_worktree_at(
        &f.root(),
        "develop",
        &f.sibling("wt-develop").to_string_lossy(),
    )
    .unwrap();

    let before = crate::changes::tests_support::snapshot(f.root());
    let on = f.current();
    let err = checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "develop").unwrap_err();
    assert_eq!(err, crate::worktree::ERR_BRANCH_ALREADY_CHECKED_OUT);
    assert_eq!(f.current(), on, "the active worktree is still on its branch");
    assert_eq!(
        before,
        crate::changes::tests_support::snapshot(f.root()),
        "a refused checkout leaves the index and working tree untouched"
    );
}

#[test]
fn checkout_blocked_by_local_changes_is_refused_and_leaves_head_where_it_was() {
    // GTC-FR-08 / WTC-FR-12, second variant: Git refuses rather than
    // overwriting a modification, and HEAD does not move — which is why the
    // tree is checked out before HEAD is.
    let f = WorktreeFixture::new();
    let main = f.current();
    f.branch("develop");
    // Diverge `develop`'s copy of `a.md` from `main`'s…
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "develop").unwrap();
    f.write("a.md", "from develop\n");
    f.commit();
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), &main).unwrap();
    // …then leave an uncommitted edit on `main` that the checkout would
    // have to clobber.
    f.write("a.md", "uncommitted local work\n");

    let err = checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "develop").unwrap_err();
    assert_eq!(err, ERR_CHECKOUT_BLOCKED);
    assert_eq!(f.current(), main, "HEAD stays on the branch it was on");
    assert_eq!(
        std::fs::read_to_string(f.root().join("a.md")).unwrap(),
        "uncommitted local work\n",
        "the working tree is byte-identical to its prior state"
    );
}

#[test]
fn checking_out_the_branch_already_active_is_a_no_op_rather_than_a_conflict() {
    // The active worktree holding the branch is not "another worktree".
    let f = WorktreeFixture::new();
    let main = f.current();
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), &main).unwrap();
    assert_eq!(f.current(), main);
}

#[test]
fn a_checkout_from_a_linked_worktree_is_refused_and_changes_nothing() {
    // GTC-FR-07, GTC-FR-08 / WTC-FR-21: a branch is checked out only in the
    // repository's primary worktree. The refusal lives in this primitive,
    // so every route to a checkout is bound by it.
    let f = WorktreeFixture::new();
    f.branch("alpha");
    f.branch("develop");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    let before = f.snapshot_all();
    let err = checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, "develop").unwrap_err();

    assert_eq!(err, crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE);
    assert_eq!(
        Repository::open(&alpha)
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap(),
        "alpha",
        "the linked worktree is still on the branch it exists to hold"
    );
    assert_eq!(
        before,
        f.snapshot_all(),
        "and its index, HEAD, and the repository's refs are untouched"
    );
}

#[test]
fn a_remote_target_from_a_linked_worktree_leaves_no_branch_behind() {
    // The remote-tracking path creates a local branch on its way to a
    // checkout (WTC-FR-11). Refused from a linked worktree, it must not
    // have created one — which only the container-wide snapshot can see,
    // since refs live under the primary worktree's `.git`.
    let f = WorktreeFixture::new();
    f.branch("alpha");
    f.remote_ref("origin/experiment");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    let before = f.snapshot_all();
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, "origin/experiment").unwrap_err(),
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE
    );
    assert!(
        f.repo().find_branch("experiment", BranchType::Local).is_err(),
        "no local tracking branch is created for a refused checkout"
    );
    assert_eq!(before, f.snapshot_all());
}

#[test]
fn the_linked_worktree_refusal_takes_precedence_over_every_other() {
    // WTC-FR-21 / GTC-FR-07, GTC-FR-08 say "for any branch". That only holds while the
    // gate sits above the other checks — move it below `resolve_checkout_
    // target` or `branch_checked_out_elsewhere` and each case below starts
    // reporting a different error, with the rule quietly becoming
    // conditional on the argument. Nothing else pins the ordering.
    let f = WorktreeFixture::new();
    let main = f.current();
    f.branch("alpha");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    // (i) a branch that does not exist at all
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, "no-such-branch").unwrap_err(),
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE,
        "not \"unknown branch\""
    );
    // (ii) a branch checked out in the primary worktree
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, &main).unwrap_err(),
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE,
        "not \"branch already checked out\""
    );
    // (iii) local modifications a checkout would have to clobber
    f.branch("develop");
    std::fs::write(alpha.join("a.md"), "uncommitted local work\n").unwrap();
    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, "develop").unwrap_err(),
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE,
        "not \"checkout blocked by local changes\""
    );
}

#[test]
fn a_detached_primary_worktree_may_still_check_a_branch_out() {
    // The rule is about *which checkout* you are in, not about what its
    // HEAD points at — and from a detached primary, checkout is the only
    // route back onto a branch.
    let f = WorktreeFixture::new();
    f.branch("develop");
    let repo = f.repo();
    let id = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(id).unwrap();
    drop(repo);

    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "develop").unwrap();
    assert_eq!(f.current(), "develop");
}

#[test]
fn a_linked_worktree_refuses_a_checkout_of_the_branch_it_is_already_on() {
    // Not a special case: nothing is checked out from a linked worktree,
    // including a no-op. Otherwise the rule would depend on the argument.
    let f = WorktreeFixture::new();
    f.branch("alpha");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    assert_eq!(
        checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &alpha, "alpha").unwrap_err(),
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE
    );
}

#[test]
fn the_branch_listing_is_the_same_from_every_worktree() {
    // GTC-FR-07: describing the repository's branches is not offering to
    // check one out, so the listing does not narrow inside a worktree —
    // the Git panel stays a read-only view of the repository there.
    let f = WorktreeFixture::new();
    f.branch("alpha");
    f.branch("develop");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    let names = |list: Vec<GitBranch>| {
        let mut n: Vec<String> = list.into_iter().map(|b| b.name).collect();
        n.sort();
        n
    };
    assert_eq!(
        names(branches_for(&f.root()).unwrap()),
        names(branches_for(&alpha).unwrap()),
    );
    // …and `current` follows the worktree it was asked from.
    let from_linked = branches_for(&alpha).unwrap();
    let current: Vec<&GitBranch> = from_linked.iter().filter(|b| b.is_current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].name, "alpha");
}

#[test]
fn a_branch_with_a_worktree_is_still_a_valid_comparison_target() {
    // CHC-FR-20: the Changes panel chooses what to diff
    // *against*, so a branch having a worktree neither qualifies nor
    // disqualifies it.
    let f = WorktreeFixture::new();
    let main = f.current();
    f.branch("feature");
    checkout_branch_at(&NullSink, &SCRATCH_BUFFER, &f.root(), "feature").unwrap();
    f.write("a.md", "one\nfrom feature\n");
    f.commit();
    // Give `main` a worktree of its own.
    crate::worktree::create_worktree_at(
        &f.root(),
        &main,
        &f.sibling("wt-main").to_string_lossy(),
    )
    .unwrap();

    let options = crate::changes::comparison_branches(&f.root()).unwrap();
    assert!(
        options.iter().any(|b| b.name == main),
        "a branch with a worktree is still offered as a target: {options:?}"
    );
    let set = crate::changes::branch_change_set(&crate::fs::RootFs::for_root(f.root()), &main).unwrap();
    assert!(
        set.entries.iter().any(|e| e.path == "a.md"),
        "and comparing against it returns a normal change set: {set:?}"
    );
}
