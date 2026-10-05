//! What the module reads: the worktrees, the branches with no worktree, and
//! which checkout is the primary one.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-02: not a Git repository ------------------------------------

#[test]
fn a_non_repository_is_the_typed_error_for_every_read() {
    // WTC-FR-02.
    let dir = TempDir::new().unwrap();
    assert_eq!(context_for(dir.path()).unwrap_err(), ERR_NOT_A_REPO);
    assert_eq!(active_entry_for(dir.path()).unwrap_err(), ERR_NOT_A_REPO);
    assert_eq!(
        propose_path_for(dir.path(), "x").unwrap_err(),
        ERR_NOT_A_REPO
    );
    assert_eq!(
        validate_activation_target(dir.path(), "/tmp/anything").unwrap_err(),
        ERR_NOT_A_REPO
    );
    assert_eq!(
        create_worktree_at(dir.path(), "x", "/tmp/anything").unwrap_err(),
        ERR_NOT_A_REPO
    );
}

// -- WTC-FR-04 / WTC-FR-05: worktree enumeration ------------------------

#[test]
fn a_lone_repository_lists_its_primary_worktree_as_active() {
    // WTC-FR-04, degenerate case: one worktree, both primary and active.
    let f = Fixture::new();
    let ctx = context_for(&f.root()).unwrap();
    assert_eq!(ctx.repository_root, to_string_path(&f.root()));
    assert_eq!(ctx.active_worktree_path, to_string_path(&f.root()));
    assert_eq!(ctx.worktrees.len(), 1);
    assert!(ctx.worktrees[0].is_primary);
    assert!(ctx.worktrees[0].is_active);
    assert!(!ctx.worktrees[0].is_missing);
    assert_eq!(ctx.worktrees[0].branch.as_deref(), Some(&*f.current_branch()));
    assert_eq!(ctx.worktrees[0].head_short_hash.len(), 7);
}

#[test]
fn the_primary_comes_first_and_exactly_one_entry_is_active_and_primary() {
    // WTC-FR-04.
    let f = Fixture::new();
    f.branch("alpha");
    f.branch("beta");
    let a = f.add_worktree("wt-alpha", "alpha");
    f.add_worktree("wt-beta", "beta");

    let ctx = context_for(&a).unwrap();
    assert_eq!(ctx.worktrees.len(), 3);
    assert!(ctx.worktrees[0].is_primary, "primary sorts first");
    assert_eq!(ctx.worktrees[0].path, to_string_path(&f.root()));
    assert_eq!(ctx.worktrees.iter().filter(|w| w.is_primary).count(), 1);
    assert_eq!(ctx.worktrees.iter().filter(|w| w.is_active).count(), 1);
    assert_eq!(
        ctx.worktrees.iter().find(|w| w.is_active).unwrap().path,
        to_string_path(&a)
    );
    // Linked worktrees follow, ordered by path.
    let linked: Vec<String> = ctx.worktrees[1..].iter().map(|w| w.path.clone()).collect();
    let mut sorted = linked.clone();
    sorted.sort();
    assert_eq!(linked, sorted);
}

#[test]
fn a_detached_worktree_carries_no_branch_and_a_deleted_one_is_reported_missing() {
    // WTC-FR-05.
    let f = Fixture::new();
    f.branch("alpha");
    f.branch("beta");
    let detached = f.add_worktree("wt-detached", "alpha");
    let gone = f.add_worktree("wt-gone", "beta");

    // Detach the first worktree's HEAD at its current commit.
    let repo = Repository::open(&detached).unwrap();
    let id = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(id).unwrap();
    drop(repo);
    // And delete the second's directory out from under Git.
    fs::remove_dir_all(&gone).unwrap();

    let ctx = context_for(&f.root()).unwrap();
    let d = ctx
        .worktrees
        .iter()
        .find(|w| w.path == to_string_path(&detached))
        .unwrap();
    assert!(d.is_detached);
    assert_eq!(d.branch, None);
    assert_eq!(d.head_short_hash.len(), 7);

    let m = ctx
        .worktrees
        .iter()
        .find(|w| w.path == to_string_path(&gone))
        .expect("a worktree whose directory is gone is reported, not omitted");
    assert!(m.is_missing);
    assert_eq!(
        m.branch.as_deref(),
        Some("beta"),
        "the admin HEAD still names the branch, so the row can label itself"
    );
}

// -- WTC-FR-06: branches with no worktree -------------------------------

#[test]
fn branches_lists_only_the_ones_with_no_worktree() {
    // WTC-FR-06.
    let f = Fixture::new();
    let main = f.current_branch();
    f.branch("chore-deps");
    f.branch("alpha");
    f.add_worktree("wt-alpha", "alpha");
    f.remote_ref("origin/experiment");
    // A remote branch WITH a local counterpart must not double up.
    f.remote_ref(&format!("origin/{main}"));

    let ctx = context_for(&f.root()).unwrap();
    let listed = branch_names(&ctx.branches);
    assert!(listed.contains(&"chore-deps".to_string()), "{listed:?}");
    assert!(
        listed.contains(&"origin/experiment".to_string()),
        "a remote with no local counterpart is offered: {listed:?}"
    );
    assert!(
        !listed.contains(&main),
        "the branch checked out in the primary worktree is not a branch-without-worktree: {listed:?}"
    );
    assert!(
        !listed.contains(&"alpha".to_string()),
        "a branch checked out in a linked worktree is excluded too: {listed:?}"
    );
    assert!(
        !listed.contains(&format!("origin/{main}")),
        "a remote whose local counterpart exists is excluded: {listed:?}"
    );
    let remote = ctx
        .branches
        .iter()
        .find(|b| b.name == "origin/experiment")
        .unwrap();
    assert_eq!(remote.kind, BRANCH_KIND_REMOTE);
    let local = ctx.branches.iter().find(|b| b.name == "chore-deps").unwrap();
    assert_eq!(local.kind, BRANCH_KIND_LOCAL);
}

// -- WTC-FR-21: a checkout belongs to the primary worktree --------------

#[test]
fn a_linked_worktree_offers_no_branches_at_all() {
    // WTC-FR-06, second half: `branches` is what could be checked out, and
    // from a linked worktree nothing can — while the worktree list itself
    // is unchanged, since moving between worktrees stays available.
    let f = Fixture::new();
    f.branch("chore-deps");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    f.remote_ref("origin/experiment");

    let from_primary = context_for(&f.root()).unwrap();
    assert!(
        !from_primary.branches.is_empty(),
        "precondition: the primary worktree does offer branches"
    );

    let from_linked = context_for(&alpha).unwrap();
    assert!(
        from_linked.branches.is_empty(),
        "a linked worktree offers none: {:?}",
        from_linked.branches
    );
    assert_eq!(
        from_linked.worktrees.len(),
        from_primary.worktrees.len(),
        "but every worktree is still listed, so the user can move between them"
    );
}

#[test]
fn only_the_primary_worktree_counts_as_primary() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    assert!(is_in_primary_worktree(&f.root()).unwrap());
    assert!(!is_in_primary_worktree(&alpha).unwrap());
}

#[test]
fn a_co_located_project_inside_the_primary_worktree_is_still_primary() {
    // The question is which checkout the project belongs to, not where
    // inside it the content root sits.
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    std::fs::create_dir_all(alpha.join("sub")).unwrap();

    assert!(is_in_primary_worktree(&f.root().join("docs")).unwrap());
    assert!(!is_in_primary_worktree(&alpha.join("sub")).unwrap());
}

#[test]
fn every_expression_of_primary_agrees_with_every_other() {
    // Three places decide "is this the repository's own checkout": this
    // helper, the `is_primary` flag on the active entry, and `context_for`'s
    // gate on offering branches. They are separate expressions of one rule,
    // so they are pinned against each other.
    let f = Fixture::new();
    f.branch("chore-deps");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    for root in [f.root(), alpha] {
        let helper = is_in_primary_worktree(&root).unwrap();
        let entry = active_entry_for(&root).unwrap();
        let ctx = context_for(&root).unwrap();
        assert_eq!(helper, entry.is_primary, "helper vs active entry at {root:?}");
        assert_eq!(
            helper,
            !ctx.branches.is_empty(),
            "helper vs the branch gate at {root:?}"
        );
        assert_eq!(
            helper,
            ctx.active_worktree_path == ctx.repository_root,
            "helper vs the context's own comparison at {root:?}"
        );
    }
}

#[test]
fn a_co_located_project_inside_a_linked_worktree_offers_no_branches_either() {
    // The gate is about which checkout the project belongs to, so a nested
    // content root inside a linked worktree is still linked.
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("chore-deps");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let nested = alpha.join("docs");

    assert!(nested.is_dir(), "precondition: the subdirectory is checked out");
    assert!(!is_in_primary_worktree(&nested).unwrap());
    assert!(context_for(&nested).unwrap().branches.is_empty());
    // …while the same subdirectory of the primary does offer them.
    assert!(!context_for(&f.root().join("docs")).unwrap().branches.is_empty());
}

#[test]
fn a_linked_worktree_may_still_be_left_and_may_still_create_another() {
    // WTS-FR-28's other half: what a linked worktree loses is moving *this*
    // checkout onto another branch. Changing which checkout the project
    // reads from, and adding a new one, stay available from everywhere.
    let f = Fixture::new();
    f.branch("alpha");
    f.branch("beta");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    // Leaving it for another worktree.
    assert_eq!(
        validate_activation_target(&alpha, &f.root().to_string_lossy()).unwrap(),
        f.root()
    );
    // And creating one from it — including proposing where it should go.
    let proposed = propose_path_for(&alpha, "spike").unwrap();
    assert!(!proposed.is_empty());
    let created =
        create_worktree_at(&alpha, "beta", &f.sibling("wt-beta").to_string_lossy())
            .unwrap();
    assert!(created.is_dir());
    assert_eq!(
        Repository::open(&created).unwrap().head().unwrap().shorthand().unwrap(),
        "beta"
    );
}

#[test]
fn is_in_primary_worktree_on_a_non_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        is_in_primary_worktree(dir.path()).unwrap_err(),
        ERR_NOT_A_REPO
    );
}
