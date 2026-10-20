//! The names a new worktree and its branch take.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-XLXY / WTC-FR-AGQQ: the rule --------------------------------

#[test]
fn a_branch_name_is_lower_case_and_keeps_its_namespace() {
    // WTC-FR-XLXY.
    assert_eq!(normalize_branch_name("More fixes"), "more-fixes");
    assert_eq!(normalize_branch_name("feature/PROJ 12"), "feature/proj-12");
    assert_eq!(normalize_branch_name("keep_under-score"), "keep_under-score");
    assert_eq!(normalize_branch_name("release/2.1"), "release/2-1");
    assert_eq!(normalize_branch_name("feat:a b"), "feat-a-b");
    assert_eq!(normalize_branch_name("tabs\tand\nnewlines"), "tabs-and-newlines");
    assert_eq!(normalize_branch_name("café"), "caf");
    assert_eq!(normalize_branch_name("ÉCOLE"), "cole");
}

#[test]
fn a_branch_name_has_no_empty_segment_and_no_hyphen_at_a_segment_edge() {
    // WTC-FR-XLXY: what makes the result a reference name Git accepts.
    assert_eq!(normalize_branch_name("a//b"), "a/b");
    assert_eq!(normalize_branch_name("/a/"), "a");
    assert_eq!(normalize_branch_name("  -x-  "), "x");
    assert_eq!(normalize_branch_name("a - b"), "a-b");
    assert_eq!(normalize_branch_name("a--b"), "a-b");
    assert_eq!(normalize_branch_name("a-/-b"), "a/b");
    assert_eq!(normalize_branch_name("/-a"), "a");
    assert_eq!(normalize_branch_name("a/ /b"), "a/b");
    assert_eq!(normalize_branch_name("x/.lock"), "x/lock");
    assert_eq!(normalize_branch_name("a..b"), "a-b");
    assert_eq!(normalize_branch_name("_a_"), "_a_");
}

#[test]
fn a_branch_name_with_nothing_to_keep_is_empty() {
    // WTC-FR-XLXY.
    for raw in ["", "   ", "!!!", "---", "///", "-/-", "日本"] {
        assert_eq!(normalize_branch_name(raw), "", "raw {raw:?}");
    }
}

#[test]
fn every_normalized_branch_name_is_a_valid_reference_name() {
    // WTC-FR-XLXY: the contract the rule exists for.
    let inputs = [
        "More fixes",
        "feature/PROJ 12",
        "a/-/b",
        "a/--/b",
        "foo.lock",
        ".hidden",
        "@{",
        "a@{b",
        "fix \u{1F41B} bug",
        "a\0b",
        "_",
        "/_/",
        "HEAD",
        "\u{130}stanbul",
        ".tmpAbC",
        "x/.lock",
        "a..b",
        "-/-a-/-",
    ];
    for raw in inputs {
        let name = normalize_branch_name(raw);
        if name.is_empty() {
            continue;
        }
        assert!(
            git2::Reference::is_valid_name(&format!("refs/heads/{name}")),
            "{raw:?} gave {name:?}"
        );
        assert_eq!(normalize_branch_name(&name), name, "stable on {raw:?}");
        let tree = normalize_worktree_name(raw);
        assert!(!tree.contains('/') && !tree.contains("--"), "{raw:?} gave {tree:?}");
        assert!(!tree.starts_with('-') && !tree.ends_with('-'), "{raw:?} gave {tree:?}");
    }
    assert_eq!(normalize_branch_name("a@{b"), "a-b");
    assert_eq!(normalize_branch_name("HEAD"), "head");
    assert_eq!(normalize_branch_name(".hidden"), "hidden");
    assert_eq!(normalize_branch_name(".."), "");
}

#[test]
fn a_worktree_name_is_one_segment() {
    // WTC-FR-AGQQ.
    assert_eq!(normalize_worktree_name("More fixes"), "more-fixes");
    assert_eq!(normalize_worktree_name("feature/PROJ 12"), "feature-proj-12");
    assert_eq!(normalize_worktree_name("a/-/b"), "a-b");
    assert_eq!(normalize_worktree_name("synthesis-more-fixes"), "synthesis-more-fixes");
    assert_eq!(normalize_worktree_name("/"), "");
}

#[test]
fn the_rule_is_stable_on_its_own_output() {
    // WTC-FR-XLXY, WTC-FR-AGQQ: a name that is already normal is not changed.
    for raw in ["More  fixes", "feature/PROJ 12", "-a-/-b-", "x.y/z"] {
        let branch = normalize_branch_name(raw);
        assert_eq!(normalize_branch_name(&branch), branch);
        let name = normalize_worktree_name(raw);
        assert_eq!(normalize_worktree_name(&name), name);
    }
}

// -- WTC-FR-13: the proposal takes the rule -----------------------------

#[test]
fn the_proposal_for_a_typed_name_is_normalized() {
    // WTC-FR-13, WTC-FR-AGQQ.
    let f = Fixture::new();
    let base = project::basename(&f.root().to_string_lossy());
    assert_eq!(
        propose_path_for(&f.root(), "More Fixes").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-more-fixes"))),
    );
    assert_eq!(
        propose_path_for(&f.root(), "Feature/PROJ-12345/Editor scroll").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-editor-scroll"))),
    );
    for branch in ["!!!", "-", "..", "/-/", "\u{65E5}\u{672C}"] {
        assert_eq!(
            propose_path_for(&f.root(), branch).unwrap(),
            to_string_path(&f.sibling(&base)),
            "{branch:?}: a name with nothing to keep proposes the basename alone"
        );
    }
    assert_eq!(
        propose_path_for(&f.root(), "feature/\u{65E5}\u{672C}").unwrap(),
        to_string_path(&f.sibling(&format!("{base}-feature"))),
        "the leaf is the last segment that is left"
    );
}

#[test]
fn the_proposal_for_an_existing_branch_uses_its_own_leaf() {
    // WTC-FR-13: the branch WTC-FR-14 resolves is the one named. The exact
    // branch has the leaf `日本`, which leaves nothing, so the proposal is the
    // basename alone; the normalized `team` would have given `<base>-team`.
    let f = Fixture::new();
    f.branch("team/\u{65E5}\u{672C}");
    let base = project::basename(&f.root().to_string_lossy());
    assert_eq!(
        propose_path_for(&f.root(), "team/\u{65E5}\u{672C}").unwrap(),
        to_string_path(&f.sibling(&base)),
    );
}

// -- WTC-FR-14 / WTC-FR-15 / WTC-FR-AGQQ: creation takes the rule --------

#[test]
fn creating_with_a_typed_name_creates_the_normalized_branch() {
    // WTC-FR-14, WTC-FR-XLXY: `refs/heads/more fixes` is no reference name.
    let f = Fixture::new();
    let target = f.sibling("wt-more");
    let created = create_worktree_at(&f.root(), "More fixes", &target.to_string_lossy()).unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "more-fixes");
    assert!(f.repo().find_branch("more-fixes", BranchType::Local).is_ok());
}

#[test]
fn creating_for_an_existing_branch_keeps_its_name_exactly() {
    // WTC-FR-14: an exact local branch is not normalized.
    let f = Fixture::new();
    f.branch("Feature/X");
    let created =
        create_worktree_at(&f.root(), " Feature/X ", &f.sibling("wt-x").to_string_lossy()).unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "Feature/X");
}

#[test]
fn creating_with_a_name_that_normalizes_to_nothing_is_the_typed_error() {
    // WTC-FR-XLXY: nothing is created for a name with nothing to keep. `-`,
    // `/` and `-/-` are what the Branch field can send (WTS-FR-UKSX).
    let f = Fixture::new();
    let before = f.repo().branches(Some(BranchType::Local)).unwrap().count();
    let target = f.sibling("wt-none");
    for raw in ["!!!", "-", "/", "-/-"] {
        let err = create_worktree_at(&f.root(), raw, &target.to_string_lossy()).unwrap_err();
        assert_eq!(err, ERR_UNKNOWN_BRANCH, "{raw:?}");
    }
    assert!(!target.exists());
    assert_eq!(context_for(&f.root()).unwrap().worktrees.len(), 1);
    assert_eq!(f.repo().branches(Some(BranchType::Local)).unwrap().count(), before);
}

#[test]
fn a_typed_name_whose_branch_already_has_a_worktree_is_the_typed_error() {
    // WTC-FR-15: the check is on the branch WTC-FR-14 resolves.
    let f = Fixture::new();
    f.add_worktree("wt-first", "more-fixes");
    let second = f.sibling("wt-second");
    let err = create_worktree_at(&f.root(), "More fixes", &second.to_string_lossy()).unwrap_err();
    assert_eq!(err, ERR_BRANCH_ALREADY_CHECKED_OUT);
    assert!(!second.exists());
}

#[test]
fn the_worktree_is_registered_under_a_normalized_name() {
    // WTC-FR-AGQQ: the directory the user chose is created as it is.
    let f = Fixture::new();
    let target = f.sibling("My Tree");
    let created = create_worktree_at(&f.root(), "spike", &target.to_string_lossy()).unwrap();
    assert_eq!(project::basename(&created.to_string_lossy()), "My Tree");
    assert!(f.repo().find_worktree("my-tree").is_ok(), "registered as `my-tree`");
}

#[test]
fn a_name_that_differs_from_a_checked_out_branch_only_in_case_is_that_branch() {
    // WTC-FR-14, WTC-FR-15: `MAIN` is no exact branch, so it resolves to
    // `main`, which has a worktree; nothing is created.
    let f = Fixture::new();
    let main = f.current_branch();
    let before = f.repo().branches(Some(BranchType::Local)).unwrap().count();
    let fresh = f.sibling("wt-fresh");
    let err =
        create_worktree_at(&f.root(), &main.to_uppercase(), &fresh.to_string_lossy()).unwrap_err();
    assert_eq!(err, ERR_BRANCH_ALREADY_CHECKED_OUT);
    assert!(!fresh.exists());
    assert_eq!(f.repo().branches(Some(BranchType::Local)).unwrap().count(), before);
}

#[test]
fn a_name_that_differs_from_a_branch_with_a_worktree_only_in_case_is_refused() {
    // WTC-FR-14, WTC-FR-15: `feature/x` resolves to `Feature/X` without
    // regard to case, so the refusal holds on every platform.
    let f = Fixture::new();
    f.add_worktree("wt-x", "Feature/X");
    let fresh = f.sibling("wt-lower");
    let err = create_worktree_at(&f.root(), "feature/x", &fresh.to_string_lossy()).unwrap_err();
    assert_eq!(err, ERR_BRANCH_ALREADY_CHECKED_OUT);
    assert!(!fresh.exists());
}

#[test]
fn a_typed_name_opens_an_existing_branch_that_differs_only_in_case() {
    // WTC-FR-14: one result on every platform, also where the file system
    // ignores case and `feature/x` already is `Feature/X`.
    let f = Fixture::new();
    f.branch("Feature/X");
    let before = f.repo().branches(Some(BranchType::Local)).unwrap().count();
    let created =
        create_worktree_at(&f.root(), "feature/x", &f.sibling("wt-x").to_string_lossy()).unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "Feature/X");
    assert_eq!(f.repo().branches(Some(BranchType::Local)).unwrap().count(), before);
}

#[test]
fn a_typed_name_reuses_the_normalized_branch_where_it_exists() {
    // WTC-FR-14: an existing branch is checked out, not reset to the HEAD.
    let f = Fixture::new();
    f.branch("more-fixes");
    let older = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    f.write("b.md", "two\n");
    f.commit("second");
    let created =
        create_worktree_at(&f.root(), "More fixes", &f.sibling("wt-m").to_string_lossy()).unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "more-fixes");
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), older);
}

#[test]
fn a_refused_registration_leaves_no_branch_it_made() {
    // WTC-FR-PKRW, WTC-FR-AGQQ: `My Tree` and `my-tree` take one registration.
    let f = Fixture::new();
    f.add_worktree("my-tree", "first");
    let second = f.sibling("My Tree");
    let err = create_worktree_at(&f.root(), "second", &second.to_string_lossy()).unwrap_err();
    assert!(err.starts_with("failed to create worktree"), "{err}");
    assert!(f.repo().find_branch("second", BranchType::Local).is_err());
    assert!(!second.exists());
    assert!(f.repo().find_worktree("my-tree").is_ok(), "the first worktree is untouched");
}

#[test]
fn a_refused_registration_keeps_a_branch_that_existed_before() {
    // WTC-FR-PKRW: only a branch the call made is deleted.
    let f = Fixture::new();
    f.add_worktree("my-tree", "first");
    f.branch("second");
    let second = f.sibling("My Tree");
    create_worktree_at(&f.root(), "second", &second.to_string_lossy()).unwrap_err();
    assert!(f.repo().find_branch("second", BranchType::Local).is_ok());
}

#[test]
fn a_location_with_nothing_to_keep_is_registered_under_the_branch() {
    // WTC-FR-AGQQ.
    let f = Fixture::new();
    let target = f.sibling("!!!");
    let created =
        create_worktree_at(&f.root(), "Team/Spike two", &target.to_string_lossy()).unwrap();
    assert_eq!(project::basename(&created.to_string_lossy()), "!!!", "created as it is");
    assert!(f.repo().find_worktree("team-spike-two").is_ok());
}

#[test]
fn the_primary_basename_is_normalized_in_the_proposal() {
    // WTC-FR-13, WTC-FR-AGQQ: the whole directory name is a worktree name. A
    // basename with nothing to keep is used as it is, so the proposal is never
    // the parent directory itself.
    for (dir, expected_spike, expected_alone) in
        [("My Repo", "my-repo-spike", "my-repo"), ("\u{65E5}\u{672C}", "spike", "\u{65E5}\u{672C}")]
    {
        let container = TempDir::new().unwrap();
        let root = container.path().join(dir);
        fs::create_dir_all(&root).unwrap();
        Repository::init(&root).unwrap();
        let root = canonicalize_lenient(&root);
        let parent = root.parent().unwrap().to_path_buf();
        assert_eq!(
            propose_path_for(&root, "Spike").unwrap(),
            to_string_path(&parent.join(expected_spike)),
            "{dir:?}"
        );
        assert_eq!(
            propose_path_for(&root, "").unwrap(),
            to_string_path(&parent.join(expected_alone)),
            "{dir:?}"
        );
    }
}
