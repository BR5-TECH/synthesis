//! Creation of a worktree, and what a creation that cannot finish leaves
//! behind.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-14 / WTC-FR-15: creation ------------------------------------

#[test]
fn creating_a_worktree_for_a_new_branch_branches_from_the_active_head() {
    // WTC-FR-14.
    let f = Fixture::new();
    let target = f.sibling("wt-spike");
    let created = create_worktree_at(&f.root(), "spike", &target.to_string_lossy()).unwrap();
    assert!(created.is_dir());
    assert!(created.join("a.md").exists(), "the branch's content is checked out");

    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "spike");
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        f.repo().head().unwrap().peel_to_commit().unwrap().id(),
        "the new branch starts at the active worktree's HEAD"
    );
    // And the repository now knows about it.
    let ctx = context_for(&f.root()).unwrap();
    assert!(names(&ctx.worktrees).contains(&"wt-spike".to_string()));
}

#[test]
fn a_created_worktree_records_its_store_relative_to_its_own_git_directory() {
    // WTC-FR-QVNL.
    let f = Fixture::new();
    let target = f.sibling("wt-spike");
    create_worktree_at(&f.root(), "spike", &target.to_string_lossy()).unwrap();

    let admin = f.root().join(".git/worktrees/wt-spike");
    let record = fs::read_to_string(admin.join("commondir")).unwrap();
    assert_eq!(record.trim(), "../..");
    assert!(
        !Path::new(record.trim()).is_absolute(),
        "an absolute record names nothing to a reader that reaches the store \
         somewhere else: {record}",
    );

    // And it resolves when the store is reached at another path. The store
    // and the worktree are copied elsewhere and the originals removed, which
    // is what a container mount amounts to.
    let elsewhere = f.sibling("elsewhere");
    let store = elsewhere.join("store");
    let checkout = elsewhere.join("checkout");
    copy_tree(&f.root().join(".git"), &store);
    copy_tree(&target, &checkout);
    fs::write(checkout.join(".git"), format!("gitdir: {}/worktrees/wt-spike\n", store.display()))
        .unwrap();
    fs::remove_dir_all(f.root().join(".git")).unwrap();

    let moved = Repository::open(&checkout).expect("the worktree resolves");
    assert_eq!(moved.head().unwrap().shorthand().unwrap(), "spike");
    moved.statuses(None).expect("a status to read");

    // The control: a record naming somewhere the store is not fails.
    // Without it this half would pass on a Git that never read the record.
    fs::write(store.join("worktrees/wt-spike/commondir"), "../../nowhere\n").unwrap();
    assert!(
        Repository::open(&checkout).and_then(|r| r.head().map(|_| ())).is_err(),
        "the record is what the resolution goes through",
    );
}

#[test]
fn a_record_that_cannot_be_written_fails_the_creation_and_leaves_nothing() {
    // WTC-FR-QVNL: the record is part of creating the worktree, so a
    // creation that cannot write it reclaims what it made.
    let f = Fixture::new();
    let target = f.sibling("wt-spike");
    create_worktree_at(&f.root(), "spike", &target.to_string_lossy()).unwrap();
    let main = Repository::open(f.root()).unwrap();

    // A record that cannot be replaced: an atomic write renames over it, and
    // nothing renames a file over a directory.
    let record = f.root().join(".git/worktrees/wt-spike/commondir");
    fs::remove_file(&record).unwrap();
    fs::create_dir(&record).unwrap();
    assert!(normalize_worktree_commondir(&main, "wt-spike").is_err());

    // And a name that is not one path segment names an administrative
    // directory somewhere else entirely, so it is refused rather than
    // recorded wrongly.
    // And a name that is not one ordinary segment reaches an
    // administrative directory somewhere else — `..` reaching the store
    // itself, which is where the record it would write means something
    // altogether different.
    for name in ["a/b", "..", "."] {
        assert!(normalize_worktree_commondir(&main, name).is_err(), "{name}");
    }
}

#[test]
fn a_creation_that_cannot_finish_reclaims_the_worktree_and_only_a_branch_it_made() {
    // WTC-FR-QVNL: the wiring between the failed record and the reclaim,
    // exercised through `create_worktree_at` itself.
    struct BreakRecord(PathBuf);
    impl CreateSeam for BreakRecord {
        fn after_worktree(&self) {
            // Nothing renames a file over a directory, so the record's
            // atomic write cannot land.
            fs::remove_file(&self.0).unwrap();
            fs::create_dir(&self.0).unwrap();
        }
    }

    let f = Fixture::new();
    let main = Repository::open(f.root()).unwrap();
    let target = f.sibling("wt-spike");
    let seam = BreakRecord(f.root().join(".git/worktrees/wt-spike/commondir"));
    let refusal =
        create_worktree_seamed(&f.root(), "spike", &target.to_string_lossy(), None, &seam)
            .expect_err("a creation that cannot be finished is refused");

    assert!(refusal.starts_with("failed to create worktree"), "{refusal}");
    assert!(!target.exists(), "the working copy is gone");
    assert!(
        !worktree_entries(&main).unwrap().iter().any(|w| w.name == "wt-spike"),
        "and so is the registration",
    );
    assert!(
        main.find_branch("spike", BranchType::Local).is_err(),
        "the branch the creation made goes with it",
    );

    // A branch the author already had is not the creation's to remove.
    f.branch("alpha");
    let second = f.sibling("wt-alpha");
    let seam = BreakRecord(f.root().join(".git/worktrees/wt-alpha/commondir"));
    create_worktree_seamed(&f.root(), "alpha", &second.to_string_lossy(), None, &seam)
        .expect_err("refused on the same terms");
    assert!(!second.exists());
    assert!(
        main.find_branch("alpha", BranchType::Local).is_ok(),
        "a branch the creation found is left where it was",
    );
}

#[test]
fn a_reclaimed_creation_removes_the_worktree_and_only_a_branch_it_made() {
    // WTC-FR-QVNL: what `create_worktree_at` does when the record fails.
    let f = Fixture::new();
    let target = f.sibling("wt-spike");
    create_worktree_at(&f.root(), "spike", &target.to_string_lossy()).unwrap();
    let main = Repository::open(f.root()).unwrap();

    reclaim_partial_worktree(&main, "wt-spike", &target, "spike", true);
    assert!(!target.exists(), "the working copy is gone");
    assert!(
        !worktree_entries(&main).unwrap().iter().any(|w| w.name == "wt-spike"),
        "and so is the registration",
    );
    assert!(
        main.find_branch("spike", BranchType::Local).is_err(),
        "the branch the creation made goes with it",
    );

    // A branch the author already had is not the creation's to remove.
    f.branch("alpha");
    let second = f.sibling("wt-alpha");
    create_worktree_at(&f.root(), "alpha", &second.to_string_lossy()).unwrap();
    reclaim_partial_worktree(&main, "wt-alpha", &second, "alpha", false);
    assert!(!second.exists());
    assert!(
        main.find_branch("alpha", BranchType::Local).is_ok(),
        "a branch the creation found is left where it was",
    );
}

/// Copy a directory tree, for reaching a store at another path.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn creating_a_worktree_for_an_existing_branch_reuses_it() {
    let f = Fixture::new();
    f.branch("alpha");
    let created =
        create_worktree_at(&f.root(), "alpha", &f.sibling("wt-a").to_string_lossy()).unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "alpha");
}

#[test]
fn creating_at_an_existing_path_is_the_typed_error_and_creates_nothing() {
    // WTC-FR-15.
    let f = Fixture::new();
    let occupied = f.sibling("wt-taken");
    fs::create_dir_all(&occupied).unwrap();
    let err = create_worktree_at(&f.root(), "other", &occupied.to_string_lossy()).unwrap_err();
    assert_eq!(err, ERR_WORKTREE_PATH_EXISTS);
    assert!(
        f.repo().find_branch("other", BranchType::Local).is_err(),
        "no branch is created when the path is refused"
    );
    assert_eq!(context_for(&f.root()).unwrap().worktrees.len(), 1, "and no worktree is registered");
    fs::remove_dir_all(&occupied).unwrap();
}

#[test]
fn creating_for_a_branch_that_already_has_a_worktree_is_the_typed_error() {
    // WTC-FR-15, second half.
    let f = Fixture::new();
    let main = f.current_branch();
    let fresh = f.sibling("wt-fresh");
    let err = create_worktree_at(&f.root(), &main, &fresh.to_string_lossy()).unwrap_err();
    assert_eq!(err, ERR_BRANCH_ALREADY_CHECKED_OUT);
    assert!(!fresh.exists(), "no directory is created when the branch is refused");
}

// -- WTC-FR-NQZE: a new branch at the tip of `start` --------------------

/// A fixture whose branch `top` is one commit ahead of the active `HEAD`.
fn fixture_with_top() -> (Fixture, git2::Oid, git2::Oid) {
    let f = Fixture::new();
    let base = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    let main = f.current_branch();
    f.branch("top");
    f.repo().set_head("refs/heads/top").unwrap();
    f.write("top.md", "top");
    f.commit("top");
    let top = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    f.repo().set_head(&format!("refs/heads/{main}")).unwrap();
    f.repo().checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    (f, base, top)
}

// WTC-FR-NQZE: the new branch starts at the tip of `start`, not at `HEAD`.
#[test]
fn a_new_branch_with_a_start_starts_at_the_tip_of_that_branch() {
    let (f, base, top) = fixture_with_top();
    assert_ne!(base, top);
    let target = f.sibling("wt-next");
    let created =
        create_worktree_from(&f.root(), "next", &target.to_string_lossy(), "top").unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "next");
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), top);
    assert!(created.join("top.md").exists(), "the content of the start is checked out");
}

// WTC-FR-NQZE: with no local branch of the name, the remote-tracking branch
// of the primary remote is the start.
#[test]
fn a_start_with_no_local_branch_uses_the_primary_remote() {
    let (f, _, top) = fixture_with_top();
    f.repo().remote("origin", "https://example.invalid/r.git").unwrap();
    f.repo().reference("refs/remotes/origin/far", top, true, "test").unwrap();
    let target = f.sibling("wt-next");
    let created =
        create_worktree_from(&f.root(), "next", &target.to_string_lossy(), "far").unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), top);
}

// WTC-FR-NQZE: a start that resolves to no commit is `branch_missing`, and
// the creation makes no directory and no branch.
#[test]
fn a_start_that_is_no_branch_is_the_typed_error_and_creates_nothing() {
    let f = Fixture::new();
    let target = f.sibling("wt-next");
    let err =
        create_worktree_from(&f.root(), "next", &target.to_string_lossy(), "nowhere").unwrap_err();
    assert_eq!(err, crate::git::objects::ERR_BRANCH_MISSING);
    assert!(!target.exists());
    assert!(f.repo().find_branch("next", BranchType::Local).is_err());
    assert_eq!(context_for(&f.root()).unwrap().worktrees.len(), 1);
}

// WTC-FR-NQZE: an existing branch ignores `start`, and an empty `start` is
// no `start`.
#[test]
fn an_existing_branch_and_an_empty_start_ignore_the_start() {
    let (f, base, _) = fixture_with_top();
    f.branch("alpha");
    let created =
        create_worktree_from(&f.root(), "alpha", &f.sibling("wt-a").to_string_lossy(), "top")
            .unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);

    let created =
        create_worktree_from(&f.root(), "beta", &f.sibling("wt-b").to_string_lossy(), " ").unwrap();
    let repo = Repository::open(&created).unwrap();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
}

// WTC-FR-NQZE: through the path of the command, a start that resolves to no
// commit leaves the content root where it was.
#[test]
fn a_start_that_is_no_branch_leaves_the_content_root_unchanged() {
    use tauri::Manager;
    let f = Fixture::new();
    let app = mock_app_on(&f.root(), &f.root());
    let store = GlobalSettingsStore::in_memory();
    let target = f.sibling("wt-next");
    let err = create_worktree_in(
        app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        "next",
        &target.to_string_lossy(),
        Some("nowhere"),
    )
    .unwrap_err();
    assert_eq!(err, crate::git::objects::ERR_BRANCH_MISSING);
    assert_eq!(app.state::<ProjectState>().root(), Some(f.root()));
    assert!(!target.exists());
}
