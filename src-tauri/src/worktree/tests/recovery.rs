//! The content root that is gone (WTC-FR-CPRM, WTC-FR-DZLO, WTC-FR-NQLS,
//! WTC-FR-ZGYJ, WTC-FR-ZXRF).

use super::*;
use tauri::Manager;

fn none() -> PathBuf {
    PathBuf::new()
}

/// A project rooted on `root`, with the offset a mount would have recorded.
fn project_on(root: &Path, anchor: &Path, offset: PathBuf) -> tauri::App<tauri::test::MockRuntime> {
    let app = mock_app_on(root, anchor);
    app.state::<ProjectState>().set_root_offset(offset);
    app
}

fn leave(app: &tauri::App<tauri::test::MockRuntime>) -> Result<WorktreeContext, String> {
    let handle = app.handle();
    leave_gone_worktree_at(
        handle,
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &GlobalSettingsStore::in_memory(),
    )
}

// WTC-FR-CPRM: the primary worktree is healthy, and its branch is named.
#[test]
fn the_primary_worktree_is_healthy_and_names_its_branch() {
    let f = Fixture::new();

    let state = root_state_of(&f.root(), &f.root(), &none());

    assert_eq!(state.state, ActiveRootHealth::Healthy);
    assert_eq!(state.primary_path, to_string_path(&f.root()));
    assert_eq!(state.primary_branch.as_deref(), Some(&*f.current_branch()));
}

// WTC-FR-CPRM: a detached primary worktree names no branch.
#[test]
fn a_detached_primary_worktree_names_no_branch() {
    let f = Fixture::new();
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(head).unwrap();

    assert_eq!(root_state_of(&f.root(), &f.root(), &none()).primary_branch, None);
}

// WTC-FR-DZLO: a live linked worktree is healthy.
#[test]
fn a_live_linked_worktree_is_healthy() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    assert_eq!(root_state_of(&alpha, &f.root(), &none()).state, ActiveRootHealth::Healthy);
}

// WTC-FR-DZLO: `rm -rf` leaves the registration, and the root is gone. The
// answer still names the way back.
#[test]
fn a_removed_linked_worktree_is_gone() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    fs::remove_dir_all(&alpha).unwrap();

    let state = root_state_of(&alpha, &f.root(), &none());

    assert_eq!(state.state, ActiveRootHealth::Gone);
    assert_eq!(state.primary_path, to_string_path(&f.root()));
    assert_eq!(state.primary_branch.as_deref(), Some(&*f.current_branch()));
}

// WTC-FR-DZLO: a directory with no Git files is gone, although Git's own
// registration still validates it.
#[test]
fn a_directory_recreated_with_no_git_files_is_gone() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    fs::remove_dir_all(&alpha).unwrap();
    fs::create_dir_all(&alpha).unwrap();

    assert_eq!(root_state_of(&alpha, &f.root(), &none()).state, ActiveRootHealth::Gone);
}

// WTC-FR-DZLO: a Git read that fails with anything but "not found" is no proof.
// The answer `gone` discards unsaved changes.
#[test]
fn a_worktree_git_cannot_read_is_unknown_and_not_gone() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    fs::write(alpha.join(".git"), "this is not a gitfile").unwrap();

    assert_eq!(root_state_of(&alpha, &f.root(), &none()).state, ActiveRootHealth::Unknown);
}

// WTC-FR-DZLO: a live co-located root is healthy whatever offset was recorded.
#[test]
fn a_live_linked_root_below_its_worktree_is_healthy_with_no_offset() {
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    assert_eq!(
        root_state_of(&alpha.join("docs"), &f.root(), &none()).state,
        ActiveRootHealth::Healthy,
    );
}

// WTC-FR-DZLO: `git worktree remove` takes the registration too.
#[test]
fn a_worktree_with_no_registration_is_gone() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let repo = f.repo();
    prune_worktree_registration(&repo, &alpha, None).unwrap();

    assert_eq!(root_state_of(&alpha, &f.root(), &none()).state, ActiveRootHealth::Gone);
}

// WTC-FR-DZLO: a linked worktree can stand inside the primary worktree's
// directory, and being below that directory does not make it the primary one.
#[test]
fn a_gone_worktree_inside_the_primary_directory_is_gone() {
    let f = Fixture::new();
    f.branch("alpha");
    let inside = f.root().join(".worktrees").join("alpha");
    fs::create_dir_all(inside.parent().unwrap()).unwrap();
    let inside =
        create_worktree_at(&f.root(), "alpha", &inside.to_string_lossy()).expect("a worktree");
    assert_eq!(root_state_of(&inside, &f.root(), &none()).state, ActiveRootHealth::Healthy);
    fs::remove_dir_all(&inside).unwrap();

    assert_eq!(root_state_of(&inside, &f.root(), &none()).state, ActiveRootHealth::Gone);
}

// WTC-FR-DZLO: a primary worktree that cannot be opened reports `unknown`.
#[test]
fn an_anchor_that_does_not_open_is_unknown() {
    let f = Fixture::new();
    let missing = f.sibling("no-such-repository");

    assert_eq!(root_state_of(&missing, &missing, &none()).state, ActiveRootHealth::Unknown);
}

// WTC-FR-DZLO, WTC-FR-ZXRF: a co-located project is read at its offset, in the
// primary worktree and in a linked one.
#[test]
fn a_co_located_root_is_read_at_its_offset() {
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let offset = PathBuf::from("docs");

    let primary = root_state_of(&f.root().join("docs"), &f.root(), &offset);
    assert_eq!(primary.state, ActiveRootHealth::Healthy);
    let linked = root_state_of(&alpha.join("docs"), &f.root(), &offset);
    assert_eq!(linked.state, ActiveRootHealth::Healthy);

    fs::remove_dir_all(&alpha).unwrap();
    let gone = root_state_of(&alpha.join("docs"), &f.root(), &offset);
    assert_eq!(gone.state, ActiveRootHealth::Gone);
}

// WTC-FR-ZXRF: the offset is read from a root that is alive.
#[test]
fn the_offset_of_a_root_is_where_it_sits_in_its_worktree() {
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");

    assert_eq!(offset_of(&f.root().join("docs")), PathBuf::from("docs"));
    assert_eq!(offset_of(&f.root()), PathBuf::new());
    assert_eq!(offset_of(&f.sibling("nothing-here")), PathBuf::new());
}

// WTC-FR-NQLS: a root that is not gone refuses, and nothing moves.
#[test]
fn leaving_a_root_that_is_not_gone_is_refused() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = project_on(&alpha, &f.root(), none());

    assert_eq!(leave(&app).unwrap_err(), ERR_WORKTREE_NOT_GONE);
    assert_eq!(app.state::<ProjectState>().root(), Some(alpha));
}

// WTC-FR-NQLS: `unknown` refuses too, because the root is not known to be gone.
#[test]
fn leaving_a_root_of_unknown_state_is_refused() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    fs::write(alpha.join(".git"), "this is not a gitfile").unwrap();
    let app = project_on(&alpha, &f.root(), none());

    assert_eq!(leave(&app).unwrap_err(), ERR_WORKTREE_NOT_GONE);
    assert_eq!(app.state::<ProjectState>().root(), Some(alpha));
}

// WTC-FR-ZXRF: each mount records the offset, while the root can still be read.
#[test]
fn a_mount_records_the_offset_of_its_root() {
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = mock_app_on(&f.root(), &f.root());
    let project = app.state::<ProjectState>();

    reroot(
        app.handle(),
        &project,
        &app.state::<ProjectWatcher>(),
        &GlobalSettingsStore::in_memory(),
        &alpha.join("docs"),
    )
    .expect("re-rooted");

    assert_eq!(project.root_offset(), PathBuf::from("docs"));
}

// WTC-FR-NQLS, WTC-FR-08: leaving moves the content root to the primary worktree
// and removes no file of it.
#[test]
fn leaving_a_gone_root_moves_to_the_primary_worktree() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = project_on(&alpha, &f.root(), none());
    fs::remove_dir_all(&alpha).unwrap();

    let context = leave(&app).expect("left");

    assert_eq!(context.active_worktree_path, to_string_path(&f.root()));
    assert_eq!(app.state::<ProjectState>().root(), Some(f.root()));
    assert!(f.root().join("a.md").exists());
}

// WTC-FR-ZXRF: a co-located project keeps its position.
#[test]
fn leaving_keeps_a_co_located_position() {
    let f = Fixture::new();
    f.write("docs/keep.md", "x\n");
    f.commit("docs");
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = project_on(&alpha.join("docs"), &f.root(), PathBuf::from("docs"));
    fs::remove_dir_all(&alpha).unwrap();

    leave(&app).expect("left");

    assert_eq!(app.state::<ProjectState>().root(), Some(f.root().join("docs")));
}

// WTC-FR-NQLS, WTC-FR-17: the primary worktree is what the project resumes in.
#[test]
fn leaving_remembers_the_primary_worktree() {
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = project_on(&alpha, &f.root(), none());
    let store = GlobalSettingsStore::in_memory();
    store.save_active_worktree(&to_string_path(&f.root()), &to_string_path(&alpha)).unwrap();
    fs::remove_dir_all(&alpha).unwrap();

    let handle = app.handle();
    leave_gone_worktree_at(
        handle,
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
    )
    .expect("left");

    assert_eq!(
        store.load_active_worktree(&to_string_path(&f.root())).unwrap().as_deref(),
        Some(&*to_string_path(&f.root())),
    );
}
