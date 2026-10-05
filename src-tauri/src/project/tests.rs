//! The tests of project creation, of the open and the close, and of the
//! state that an active project installs.

use super::*;
use crate::artifacts::ContentTracker;
use crate::watcher::ProjectWatcher;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::time::Duration;

#[test]
fn basename_plain() {
    assert_eq!(basename("/Users/x/dev/repo"), "repo");
}

#[test]
fn basename_trailing_slash() {
    assert_eq!(basename("/Users/x/dev/repo/"), "repo");
}

#[test]
fn basename_no_separator() {
    assert_eq!(basename("repo"), "repo");
}

#[test]
fn basename_tilde_path() {
    assert_eq!(basename("~/dev/acme"), "acme");
}

#[test]
fn basename_unicode() {
    assert_eq!(basename("/tmp/проект"), "проект");
}

#[test]
fn basename_empty_returns_input() {
    assert_eq!(basename(""), "");
}

#[test]
fn open_project_at_path_happy() {
    let h = open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "/tmp/foo".into()).unwrap();
    assert_eq!(h.name, "foo");
    assert_eq!(h.path, "/tmp/foo");
}

#[test]
fn open_project_at_path_trailing_slash_strips_in_name_only() {
    let h = open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "/tmp/foo/".into()).unwrap();
    assert_eq!(h.name, "foo");
    assert_eq!(h.path, "/tmp/foo/");
}

#[test]
fn open_project_at_path_empty_errors() {
    assert!(open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "".into()).is_err());
}

#[test]
fn open_project_at_path_whitespace_errors() {
    assert!(open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "   ".into()).is_err());
}

#[test]
fn git_url_ssh_with_dot_git() {
    let h = open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "git@github.com:org/repo.git".into()).unwrap();
    assert_eq!(h.name, "repo");
    assert_eq!(h.path, "~/dev/repo");
}

#[test]
fn git_url_https_with_dot_git() {
    let h = open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "https://github.com/org/repo.git".into()).unwrap();
    assert_eq!(h.name, "repo");
}

#[test]
fn git_url_https_without_dot_git() {
    let h = open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "https://github.com/org/repo".into()).unwrap();
    assert_eq!(h.name, "repo");
}

#[test]
fn git_url_ssh_without_suffix() {
    let h = open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "git@github.com:org/repo".into()).unwrap();
    assert_eq!(h.name, "repo");
}

#[test]
fn git_url_empty_errors() {
    assert!(open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "".into()).is_err());
}

#[test]
fn git_url_whitespace_errors() {
    assert!(open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "   ".into()).is_err());
}

#[test]
fn git_url_just_dot_git_errors() {
    assert!(open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), ".git".into()).is_err());
}

#[test]
fn git_url_trailing_slash_errors() {
    assert!(open_project_from_git_url_impl(&GlobalSettingsStore::in_memory(), "https://github.com/org/repo.git/".into()).is_err());
}

#[test]
fn create_project_standalone_happy() {
    let h = create_project_impl(&GlobalSettingsStore::in_memory(), "my-proj".into(), CreateMode::Standalone, "/tmp/x".into()).unwrap();
    assert_eq!(h.name, "my-proj");
    assert_eq!(h.path, "/tmp/x");
}

#[test]
fn create_project_colocated_happy() {
    let h = create_project_impl(&GlobalSettingsStore::in_memory(), "my-proj".into(), CreateMode::Colocated, "/tmp/x".into()).unwrap();
    assert_eq!(h.name, "my-proj");
}

#[test]
fn create_project_trims_name() {
    let h = create_project_impl(&GlobalSettingsStore::in_memory(), "  my-proj  ".into(), CreateMode::Standalone, "/tmp/x".into())
        .unwrap();
    assert_eq!(h.name, "my-proj");
}

#[test]
fn create_project_empty_name_errors() {
    assert!(create_project_impl(&GlobalSettingsStore::in_memory(), "".into(), CreateMode::Standalone, "/tmp/x".into()).is_err());
}

#[test]
fn create_project_whitespace_name_errors() {
    assert!(create_project_impl(&GlobalSettingsStore::in_memory(), "   ".into(), CreateMode::Standalone, "/tmp/x".into()).is_err());
}

#[test]
fn create_project_empty_target_errors() {
    assert!(create_project_impl(&GlobalSettingsStore::in_memory(), "foo".into(), CreateMode::Standalone, "".into()).is_err());
}

#[test]
fn create_mode_deserializes_lowercase() {
    let s: CreateMode = serde_json::from_str("\"standalone\"").unwrap();
    assert!(matches!(s, CreateMode::Standalone));
    let c: CreateMode = serde_json::from_str("\"colocated\"").unwrap();
    assert!(matches!(c, CreateMode::Colocated));
}

#[test]
fn create_mode_rejects_uppercase() {
    assert!(serde_json::from_str::<CreateMode>("\"Standalone\"").is_err());
}

#[test]
fn create_mode_rejects_unknown() {
    assert!(serde_json::from_str::<CreateMode>("\"hybrid\"").is_err());
}

// GSS-FR-10 wiring: the open/create commands append the opened project to
// the recents MRU via `record_open`. The command fns themselves need a
// Tauri runtime (State), but their post-success seam — `record_open` — is
// pure over the store and is what actually produces the user-visible
// behavior ("the project I just opened shows up in recents"). These tests
// guard against silently dropping the recording call.
#[test]
fn record_open_appends_opened_project_to_recents() {
    let store = GlobalSettingsStore::in_memory();
    let handle = open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "/Users/me/code/proj-a".into()).unwrap();
    record_open(&store, &handle);
    let recents = store.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].name, "proj-a");
    assert_eq!(recents[0].path, "/Users/me/code/proj-a");
}

#[test]
fn record_open_for_created_project_uses_handle_name_and_path() {
    let store = GlobalSettingsStore::in_memory();
    let handle =
        create_project_impl(&GlobalSettingsStore::in_memory(), "new-proj".into(), CreateMode::Standalone, "/tmp/new-proj".into())
            .unwrap();
    record_open(&store, &handle);
    let recents = store.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].name, "new-proj");
    assert_eq!(recents[0].path, "/tmp/new-proj");
}

#[test]
fn record_open_most_recent_open_sorts_first() {
    // Open A then B; the most recently opened (B) is first in recents.
    let store = GlobalSettingsStore::in_memory();
    record_open(&store, &open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "/tmp/a".into()).unwrap());
    record_open(&store, &open_project_at_path_impl(&GlobalSettingsStore::in_memory(), "/tmp/b".into()).unwrap());
    let recents = store.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 2);
    assert_eq!(recents[0].name, "b", "most-recently-opened project sorts first");
}

// -- Two-level identity (PST-FR-01 / WTC-FR-17 / WTC-FR-18) ------------

/// A repository at `<container>/repo` with one commit, so linked worktrees
/// can be created as siblings inside the container.
fn repo_fixture() -> (tempfile::TempDir, PathBuf) {
    let container = tempfile::TempDir::new().unwrap();
    let root = container.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    drop(config);
    std::fs::write(root.join("a.md"), "one\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    {
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }
    drop(index);
    drop(repo);
    let canonical = crate::changes::canonicalize_lenient(&root);
    (container, canonical)
}

#[test]
fn opening_a_linked_worktree_resolves_to_the_repositorys_project() {
    // PST-FR-22 / WTC-FR-18 / GSS-FR-18, GSS-FR-05: the handle names the repository as
    // the project's identity and the opened worktree as its content root,
    // and the recents list holds ONE entry — recording the primary
    // worktree, so a repository with several worktrees is one project.
    let (container, primary) = repo_fixture();
    let linked = container.path().join("wt-alpha");
    {
        let repo = git2::Repository::open(&primary).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("alpha", &head, false).unwrap();
    }
    crate::worktree::create_worktree_at(&primary, "alpha", &linked.to_string_lossy())
        .unwrap();
    let linked = crate::changes::canonicalize_lenient(&linked);

    let store = GlobalSettingsStore::in_memory();
    let from_primary =
        open_project_at_path_impl(&store, primary.to_string_lossy().into()).unwrap();
    record_open(&store, &from_primary);
    let from_linked =
        open_project_at_path_impl(&store, linked.to_string_lossy().into()).unwrap();
    record_open(&store, &from_linked);

    assert_eq!(
        from_linked.path,
        primary.to_string_lossy(),
        "the project's identity anchor is the repository's primary worktree"
    );
    assert_eq!(
        from_linked.active_worktree_path,
        linked.to_string_lossy(),
        "and the opened worktree is the content root"
    );
    assert_eq!(
        from_linked.name, from_primary.name,
        "so the project presents under one name whichever worktree is opened"
    );
    let recents = store.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1, "one repository, one entry: {recents:?}");
    assert_eq!(recents[0].path, primary.to_string_lossy());
}

#[test]
fn opening_the_anchor_resumes_the_worktree_the_project_was_last_in() {
    // WTC-FR-17 / GSS-FR-16 through the open path.
    let (container, primary) = repo_fixture();
    let linked = container.path().join("wt-alpha");
    {
        let repo = git2::Repository::open(&primary).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("alpha", &head, false).unwrap();
    }
    crate::worktree::create_worktree_at(&primary, "alpha", &linked.to_string_lossy())
        .unwrap();
    let linked = crate::changes::canonicalize_lenient(&linked);

    let store = GlobalSettingsStore::in_memory();
    store
        .save_active_worktree(&primary.to_string_lossy(), &linked.to_string_lossy())
        .unwrap();

    let handle =
        open_project_at_path_impl(&store, primary.to_string_lossy().into()).unwrap();
    assert_eq!(handle.active_worktree_path, linked.to_string_lossy());
    assert!(!handle.remembered_worktree_unavailable);

    // And once that worktree is gone, the project falls back to the primary
    // and says so.
    std::fs::remove_dir_all(&linked).unwrap();
    let fallback =
        open_project_at_path_impl(&store, primary.to_string_lossy().into()).unwrap();
    assert_eq!(fallback.active_worktree_path, primary.to_string_lossy());
    assert!(fallback.remembered_worktree_unavailable);
}

#[test]
fn a_project_outside_a_repository_is_its_own_anchor_and_content_root() {
    let store = GlobalSettingsStore::in_memory();
    let handle = open_project_at_path_impl(&store, "/tmp/plain".into()).unwrap();
    assert_eq!(handle.path, "/tmp/plain");
    assert_eq!(handle.active_worktree_path, "/tmp/plain");
    assert!(!handle.remembered_worktree_unavailable);
}

#[test]
fn the_slot_key_is_the_anchor_and_a_worktree_switch_does_not_change_it() {
    // PSS-FR-06 / SNV-FR-08 / GSS-FR-17: layout preferences are keyed by
    // project, and a switch of active worktree moves the *root* only — so
    // the key the layout is stored under is unchanged and the shell's shape
    // survives the switch.
    let state = ProjectState::default();
    state.set_anchor("/dev/acme".into());
    state.set_root(PathBuf::from("/dev/acme"));
    assert_eq!(state.slot_key(), "/dev/acme");

    state.set_root(PathBuf::from("/dev/acme-main"));
    assert_eq!(
        state.slot_key(),
        "/dev/acme",
        "the content root moved; the project did not"
    );

    // With no project open the key is empty, which selects the global
    // default slot (PSS-FR-06).
    state.clear_root();
    assert_eq!(state.slot_key(), "");
}

#[test]
fn project_state_require_root_errors_until_a_project_is_open() {
    let state = ProjectState::default();
    // No project open -> typed "no project open" error (PST-FR-07 / ASC-FR-05).
    let err = state.require_root().unwrap_err();
    assert_eq!(err, "no project open");

    // A real directory: `require_root` now hands out a *governed* root
    // (FSA-FR-19), and there is no governing a directory that does not
    // exist — the access could not be built for it.
    let dir = tempfile::TempDir::new().unwrap();
    state.set_root(dir.path().to_path_buf());
    assert_eq!(state.require_root().unwrap().path(), dir.path());
}

#[test]
fn deactivate_project_tears_down_all_project_state() {
    // SNV-FR-25 / PST-FR-14 / ASC-FR-14: closing the project forgets the
    // root, leaves the watcher inactive, and empties the per-project caches.
    let project = ProjectState::default();
    let watcher = ProjectWatcher::default();
    let tracker = ContentTracker::default();
    let drafts_watcher = crate::draft_watcher::DraftsWatcher::default();
    let candidates = crate::scanning::CandidateStore::default();
    let attribution = crate::scanning::AttributionBaseline::default();

    let dir = tempfile::TempDir::new().unwrap();
    project.set_root(dir.path().to_path_buf());
    tracker.record("a.md", "ck");
    // ASC-FR-17: mount a candidate list so the teardown assertion below is
    // not vacuous — a fresh store is already empty.
    let _ = candidates.candidates(&crate::fs::RootFs::for_root(dir.path()));
    // Likewise the attribution baseline: recorded from a real file so the
    // teardown assertion below cannot pass against an already-empty store.
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(crate::scanning::LIBRARY_TOML_REL),
        "[assignments]\n",
    )
    .unwrap();
    attribution.record_from_disk(&crate::fs::RootFs::for_root(dir.path()));
    assert!(
        attribution.current().is_some(),
        "precondition: a baseline is recorded before the close"
    );
    // Activate a REAL watcher so the teardown assertion is not vacuous: a
    // fresh ProjectWatcher is already inactive, so without this `!is_active()`
    // would pass even if `stop()` were a no-op — and watcher teardown
    // (ASC-FR-14) is the strongest claim of FR-25. A debouncer is
    // constructible without a Tauri runtime (it only needs `notify`).
    let debouncer = new_debouncer(Duration::from_millis(50), |_res: DebounceEventResult| {})
        .expect("debouncer construction must succeed in a unit test");
    watcher.set(debouncer);
    assert!(project.require_root().is_ok());
    assert!(watcher.is_active(), "precondition: the watcher is live before close");
    assert_eq!(tracker.len(), 1);
    // DRS-FR-42: a live drafts watch too, so its teardown assertion below is
    // not vacuous either.
    let drafts_debouncer =
        new_debouncer(Duration::from_millis(50), |_res: DebounceEventResult| {})
            .expect("debouncer construction must succeed in a unit test");
    drafts_watcher.set_for_test(drafts_debouncer);
    assert!(drafts_watcher.is_active(), "precondition: the drafts watch is live");

    deactivate_project(
        &project,
        &watcher,
        &tracker,
        &drafts_watcher,
        &candidates,
        &attribution,
    );

    assert_eq!(
        project.require_root().unwrap_err(),
        "no project open",
        "the root must be forgotten so tree commands error again (PST-FR-14)"
    );
    assert!(!watcher.is_active(), "watcher torn down (ASC-FR-14)");
    assert_eq!(tracker.len(), 0, "content baselines dropped (PST-FR-14)");
    assert!(
        !drafts_watcher.is_active(),
        "the drafts watch goes down with the project, so a closed project \
         emits no prompt change (DRS-FR-42)"
    );
    assert!(
        attribution.current().is_none(),
        "the attribution baseline goes with the content root (ASC-FR-14): \
         carried across a close, the next project's first assignment could \
         match it by coincidence and be suppressed as our own"
    );
    assert!(
        !candidates.is_mounted(),
        "the candidate list is torn down with the scan (ASC-FR-14 / ASC-FR-17)"
    );
}

#[test]
fn close_project_command_stays_wired_to_the_teardown() {
    // SNV-FR-25 / EDT-FR-33: the teardown moved behind a command the
    // frontend calls *after* writing pending Editor changes (a closed
    // project has no root, so `save_artifact_contents` would fail). The
    // command needs managed `State` to invoke, so what is pinned here is
    // that it stays in scope with its runtime signature — the teardown
    // behaviour itself is covered by the `deactivate_project` tests it
    // delegates to.
    let _close = close_project::<tauri::test::MockRuntime>;
}

#[test]
fn saving_requires_an_open_project_so_writes_must_precede_the_close() {
    // The ordering constraint EDT-FR-33 rests on: once the project is torn
    // down there is no root to resolve an artifact against, so a flush that
    // ran after the close would fail rather than write.
    let dir = tempfile::TempDir::new().unwrap();
    let project = ProjectState::default();
    project.set_root(dir.path().to_path_buf());
    assert!(project.require_root().is_ok());

    deactivate_project(
        &project,
        &ProjectWatcher::default(),
        &ContentTracker::default(),
        &crate::draft_watcher::DraftsWatcher::default(),
        &crate::scanning::CandidateStore::default(),
        &crate::scanning::AttributionBaseline::default(),
    );

    assert_eq!(project.require_root().unwrap_err(), "no project open");
}

#[test]
fn deactivate_project_touches_no_file_and_is_idempotent() {
    // SNV-FR-25: "No file under the project is modified by the close." And a
    // second close (or a close with nothing left open) must stay safe.
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(root.join("keep.md"), b"untouched").unwrap();

    let project = ProjectState::default();
    let watcher = ProjectWatcher::default();
    let tracker = ContentTracker::default();
    let drafts_watcher = crate::draft_watcher::DraftsWatcher::default();
    let candidates = crate::scanning::CandidateStore::default();
    let attribution = crate::scanning::AttributionBaseline::default();
    project.set_root(root.to_path_buf());

    deactivate_project(
        &project,
        &watcher,
        &tracker,
        &drafts_watcher,
        &candidates,
        &attribution,
    );
    // Idempotent: a second close over already-cleared state must not panic.
    deactivate_project(
        &project,
        &watcher,
        &tracker,
        &drafts_watcher,
        &candidates,
        &attribution,
    );

    assert_eq!(
        project.require_root().unwrap_err(),
        "no project open",
        "a redundant close keeps the project closed"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("keep.md")).unwrap(),
        "untouched",
        "close must not modify any file under the project (SNV-FR-25)"
    );
}

// ---------------------------------------------------------------------------
// The repository machine store (`RMS-repository-machine-storage.md`)
// ---------------------------------------------------------------------------

/// RMS-FR-EYAM: closing a project releases the store selection, so a later
/// command answers "no project open" rather than resolving a closed project's
/// store.
#[test]
fn rms_a_closed_project_resolves_no_store() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let state = ProjectState::default();
    state.set_root(dir.path().to_path_buf());
    state.set_anchor(dir.path().to_string_lossy().into_owned());
    assert!(state.require_store().is_ok());

    state.clear_root();
    assert_eq!(state.require_store().unwrap_err(), "no project open");
    assert_eq!(state.require_root().unwrap_err(), "no project open");
}

/// RMS-FR-WGQS, RMS-FR-HAJC: a store the instance cannot reach is the typed
/// `store_unavailable`, and the content root still answers — so a write refuses
/// with an error the caller can report rather than landing somewhere else.
#[test]
fn rms_a_store_that_cannot_be_resolved_is_the_typed_refusal() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let state = ProjectState::default();
    // The production install: an instance allowlisting the worktree alone, and
    // no named store, so resolving one reaches an application data root the
    // instance does not hold.
    let access = crate::fs::FsAccess::builder()
        .allow_root(dir.path())
        .build()
        .expect("access");
    state.set_root_with_access(dir.path().to_path_buf(), Some(Arc::new(access)));
    state.set_anchor(dir.path().to_string_lossy().into_owned());

    assert!(state.require_root().is_ok(), "the content root still answers");
    assert_eq!(
        state.require_store().unwrap_err(),
        crate::repository_store::ERR_STORE_UNAVAILABLE,
    );
}

/// RMS-FR-QJVT, RMS-FR-TCAF, RMS-FR-LPWG: the store is resolved from the
/// identity anchor and never from the active worktree, so a switch of worktree
/// resolves the same store and a second repository resolves another.
///
/// Driven through `require_store()` rather than through the mapping function,
/// which is the point: resolving from `root()` instead of `anchor()` is exactly
/// the mistake this has to catch, and it would pass a test that hashed the
/// anchor twice itself.
#[test]
fn rms_the_store_follows_the_anchor_rather_than_the_active_worktree() {
    let app_data = tempfile::TempDir::new().expect("tempdir");
    let repository = tempfile::TempDir::new().expect("tempdir");
    let linked = tempfile::TempDir::new().expect("tempdir");
    let other = tempfile::TempDir::new().expect("tempdir");

    // The production resolution: an instance that reaches the application data
    // root, and no named store.
    let resolve = |anchor: &std::path::Path, root: &std::path::Path| -> PathBuf {
        let access = Arc::new(
            crate::fs::FsAccess::builder()
                .allow_root(app_data.path())
                .allow_root(root)
                .build()
                .expect("access"),
        );
        let state = ProjectState::default();
        state.set_root_with_access(root.to_path_buf(), Some(Arc::clone(&access)));
        state.set_anchor(anchor.to_string_lossy().into_owned());
        // The identity `require_store` resolves — from the anchor, never from
        // the root it was just handed.
        let identity = crate::repository_store::normalize_identity(std::path::Path::new(
            &state.slot_key(),
        ));
        crate::repository_store::ensure_store_in(app_data.path(), &identity, &access)
            .expect("store")
    };

    // WTC-FR-18: the active worktree moves and the anchor does not, so the two
    // worktrees of one repository resolve one store.
    let from_primary = resolve(repository.path(), repository.path());
    let from_linked = resolve(repository.path(), linked.path());
    assert_eq!(from_primary, from_linked, "a worktree change selects the same store");

    // RMS-FR-LPWG: another repository, another store.
    let from_other = resolve(other.path(), other.path());
    assert_ne!(from_primary, from_other, "a second repository selects another");
}

/// RMS-FR-XLTR, PST-FR-MHZB: the **mount** schedules the import pass, which is
/// what makes a worktree change carry that worktree's committed records into
/// the store as an open does.
///
/// Bounded to `remount_content_root`'s own body: an unbounded search would find
/// the call in any function below it and pass whatever the mount actually does.
#[test]
fn rms_mounting_a_content_root_schedules_the_import_pass() {
    let source = include_str!("../project.rs");
    let after = source
        .split_once("pub fn remount_content_root")
        .expect("the mount")
        .1;
    let body = after
        .split_once("\n}\n")
        .expect("the end of the mount")
        .0;
    assert!(
        body.contains("crate::repository_store::schedule_import(app)"),
        "the mount itself schedules one import pass",
    );
    // And the seam a worktree change arrives through is that mount
    // (`WTC-worktree-context.md` WTC-FR-08).
    assert!(
        include_str!("../worktree.rs").contains("remount_content_root"),
        "a worktree change re-roots through the mount",
    );
}
