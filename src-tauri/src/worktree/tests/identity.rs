//! Project identity: the anchor, the worktree the project resumes into, and
//! the guarantee that a read changes nothing.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- WTC-FR-19: nothing destructive -------------------------------------

#[test]
fn the_read_operations_leave_the_repository_byte_identical() {
    // WTC-FR-19, WTC-FR-04, WTC-FR-06: every command in the contract surface has been invoked and
    // no worktree registration was removed, no branch deleted, no commit
    // rewritten. The three mutating operations are additive by
    // construction; this pins the four read paths.
    let f = Fixture::new();
    f.branch("alpha");
    f.add_worktree("wt-alpha", "alpha");
    f.remote_ref("origin/experiment");

    let before = crate::changes::tests_support::snapshot(f.root());
    let ctx = context_for(&f.root()).unwrap();
    active_entry_for(&f.root()).unwrap();
    propose_path_for(&f.root(), "anything").unwrap();
    for w in &ctx.worktrees {
        let _ = validate_activation_target(&f.root(), &w.path);
    }
    assert_eq!(
        before,
        crate::changes::tests_support::snapshot(f.root()),
        "reading the worktree context must not write to refs, the index, or the working tree"
    );
    let after = context_for(&f.root()).unwrap();
    assert_eq!(after.worktrees.len(), ctx.worktrees.len());
    assert_eq!(after.branches.len(), ctx.branches.len());
}

// -- WTC-FR-18 / GSS-FR-18: project identity ----------------------------

#[test]
fn the_project_anchor_is_the_primary_worktree_from_any_of_its_worktrees() {
    // WTC-FR-18: opening a linked worktree resolves to the same project.
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");
    assert_eq!(project_anchor(&f.root()), f.root());
    assert_eq!(
        project_anchor(&a),
        f.root(),
        "a linked worktree anchors on the repository's primary worktree"
    );
}

#[test]
fn a_project_outside_a_repository_anchors_on_itself() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        project_anchor(dir.path()),
        dir.path(),
        "with no repository to reconcile against, the opened path is the anchor verbatim"
    );
}

// -- WTC-FR-17: resuming the remembered worktree ------------------------

#[test]
fn a_project_resumes_the_worktree_it_was_last_working_in() {
    // WTC-FR-17.
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");
    let store = GlobalSettingsStore::in_memory();
    store
        .save_active_worktree(&to_string_path(&f.root()), &to_string_path(&a))
        .unwrap();

    let (resumed, fell_back) = resume_active_worktree(&store, &f.root(), &f.root());
    assert_eq!(resumed, a);
    assert!(!fell_back);
}

#[test]
fn a_remembered_worktree_that_is_gone_falls_back_to_the_primary_and_says_so() {
    // WTC-FR-17, second half.
    let f = Fixture::new();
    f.branch("alpha");
    let a = f.add_worktree("wt-alpha", "alpha");
    let store = GlobalSettingsStore::in_memory();
    store
        .save_active_worktree(&to_string_path(&f.root()), &to_string_path(&a))
        .unwrap();
    fs::remove_dir_all(&a).unwrap();

    let (resumed, fell_back) = resume_active_worktree(&store, &f.root(), &f.root());
    assert_eq!(resumed, f.root());
    assert!(fell_back, "the UI is told the remembered worktree was unavailable");
}

#[test]
fn opening_a_linked_worktree_supersedes_the_remembered_one() {
    // WTC-FR-18: "the path opened becomes the active worktree".
    let f = Fixture::new();
    f.branch("alpha");
    f.branch("beta");
    let a = f.add_worktree("wt-alpha", "alpha");
    let b = f.add_worktree("wt-beta", "beta");
    let store = GlobalSettingsStore::in_memory();
    store
        .save_active_worktree(&to_string_path(&f.root()), &to_string_path(&a))
        .unwrap();

    let (resumed, fell_back) = resume_active_worktree(&store, &f.root(), &b);
    assert_eq!(resumed, b);
    assert!(!fell_back);
}

#[test]
fn a_project_with_nothing_remembered_stays_where_it_was_opened() {
    let f = Fixture::new();
    let store = GlobalSettingsStore::in_memory();
    let (resumed, fell_back) = resume_active_worktree(&store, &f.root(), &f.root());
    assert_eq!(resumed, f.root());
    assert!(!fell_back);
}

#[test]
fn the_event_name_is_one_tauri_will_actually_deliver() {
    // Tauri accepts only alphanumerics, `-`, `/`, `:` and `_` in an event
    // name. A name with spaces is rejected by `emit` — which returns an
    // error rather than panicking, so the failure is silent and the
    // frontend simply never hears anything. This pins the wire name against
    // that, since nothing else would catch it.
    use tauri::Emitter;
    let app = tauri::test::mock_app();
    assert!(
        app.emit(WORKTREE_CONTEXT_CHANGED, ()).is_ok(),
        "{WORKTREE_CONTEXT_CHANGED:?} is not a deliverable Tauri event name"
    );
}
