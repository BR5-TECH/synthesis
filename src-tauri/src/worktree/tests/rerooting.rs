//! Re-rooting: what a switch tears down, what it remounts, and what it emits.
//!
//! One part of `mod.rs`, which holds the fixture and the helpers these use.

use super::*;

// -- Re-rooting (WTC-FR-08 / WTC-FR-16) ---------------------------------

#[test]
fn rerooting_tears_the_previous_root_down_and_remounts_on_the_new_one() {
    // WTC-FR-08 / PST-FR-22, PST-FR-15 / ASC-FR-16 / CHC-FR-19: the in-memory state and
    // the watcher are unmounted and remounted, and no file in either
    // worktree is touched.
    use tauri::Manager;
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    let app = mock_app_on(&f.root(), &f.root());
    let tracker = app.state::<crate::artifacts::ContentTracker>();
    tracker.record("a.md", "ck");
    // ASC-FR-16: a baseline recorded against the OUTGOING root, so the
    // assertion after the reroot is not vacuous. The same relative path
    // names different bytes in a different checkout, so carrying it across
    // could suppress the incoming worktree's first genuine change.
    let attribution = app.state::<crate::scanning::AttributionBaseline>();
    attribution.adopt(Some("outgoing-root-checksum".to_string()));
    let store = GlobalSettingsStore::in_memory();

    let before_primary = crate::changes::tests_support::snapshot(f.root());
    let before_alpha = crate::changes::tests_support::snapshot(alpha.clone());

    let context = reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        &alpha,
    )
    .unwrap();

    assert_eq!(context.active_worktree_path, to_string_path(&alpha));
    assert_eq!(
        app.state::<ProjectState>().require_root().unwrap().path(),
        alpha,
        "the content root moved"
    );
    assert_eq!(
        tracker.len(),
        0,
        "the previous root's checksum baselines are dropped, not carried over"
    );
    assert!(
        attribution.current().is_none(),
        "nor the outgoing root's attribution baseline (ASC-FR-16)"
    );
    // FSA-FR-21 / FSA-FR-17: the shared filesystem instance was rebuilt on
    // the incoming worktree, and the outgoing one is no longer reachable
    // through it — the allowlist did not grow, it was replaced.
    let access = app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("a reroot installs a filesystem instance");
    let canonical_alpha = std::fs::canonicalize(&alpha).unwrap();
    access
        .write_text_atomic(canonical_alpha.join("fsa-probe.md"), "reachable")
        .expect("the incoming worktree is writable through the new instance");
    let outgoing = std::fs::canonicalize(f.root()).unwrap();
    let err = access
        .write_text_atomic(outgoing.join("fsa-probe.md"), "should refuse")
        .expect_err("the outgoing worktree must not stay reachable");
    assert!(
        matches!(err, crate::fs::FsError::EscapesAllowedRoots { .. }),
        "expected EscapesAllowedRoots for the outgoing root, got {err:?}"
    );
    assert!(
        !outgoing.join("fsa-probe.md").exists(),
        "and the refused write created nothing there"
    );
    std::fs::remove_file(canonical_alpha.join("fsa-probe.md")).unwrap();

    assert!(
        app.state::<ProjectWatcher>().is_active(),
        "a fresh watcher is mounted on the new content root"
    );
    assert_eq!(
        before_primary,
        crate::changes::tests_support::snapshot(f.root()),
        "no file under the outgoing worktree is modified"
    );
    assert_eq!(
        before_alpha,
        crate::changes::tests_support::snapshot(alpha),
        "nor under the incoming one"
    );
}

#[test]
fn rerooting_discards_the_outgoing_worktrees_skill_registry_and_builds_the_incomings() {
    // WTC-FR-08 / DSL-FR-22: the skill registry belongs to the
    // content root. Switching worktrees discards it and enumerates the new
    // root fresh, so a skill that exists only in A cannot answer a query
    // after the switch to B.
    //
    // Deliberately end to end through `reroot` rather than through
    // `Bm25Indexer::clear`: the registry rides the published snapshot, so
    // the *mechanism* is BMI-FR-25's teardown — but nothing except this
    // proves the reroot actually reaches it. Verified against two
    // mutations, each of which leaves every other test in this file green
    // while A's skills go on answering B's queries: dropping the
    // `indexer.mount` call from `remount_content_root`, and leaving
    // `Bm25Indexer::reset` to bump the generation without emptying the
    // published set.
    use crate::skills::{list_skills, search_skills};
    use tauri::Manager;
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    // A skill in each worktree, sharing no vocabulary. A's is untracked, so
    // the linked worktree checked out from `alpha` does not carry it —
    // which is what makes "unique to A" true rather than asserted.
    let write_skill = |root: &Path, name: &str, description: &str| {
        let dir = root.join(".claude/skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# Body\n"),
        )
        .unwrap();
    };
    write_skill(&f.root(), "only-in-a", "concerns cartography");
    write_skill(&alpha, "only-in-b", "concerns bathymetry");

    let app = mock_app_on(&f.root(), &f.root());
    app.manage(crate::bm25_index::Bm25Indexer::default());
    let handle = app.handle().clone();
    let indexer = app.state::<crate::bm25_index::Bm25Indexer>();

    /// Wait for a background pass to publish. Bounded rather than a fixed
    /// sleep: a pass waits out the coalescing window before it runs, so the
    /// interesting moment is "whenever it lands", not a duration.
    fn settle(
        indexer: &crate::bm25_index::Bm25Indexer,
        want: &str,
    ) -> Vec<crate::skills::SkillDescriptor> {
        for _ in 0..100 {
            let listed = list_skills(indexer);
            if listed.iter().any(|s| s.folder_name == want) {
                return listed;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        list_skills(indexer)
    }

    // Mount A the way opening a project does, and let its build land.
    project::remount_content_root(
        &handle,
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &f.root(),
        &crate::logging::BUFFER,
        project::Diagnostics::Discard,
    );
    let on_a = settle(&indexer, "only-in-a");
    assert_eq!(
        on_a.iter().map(|s| s.folder_name.as_str()).collect::<Vec<_>>(),
        vec!["only-in-a"],
        "precondition: A's skill is registered"
    );
    assert_eq!(search_skills(&indexer, "cartography", 10).len(), 1);

    let store = GlobalSettingsStore::in_memory();
    reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        &alpha,
    )
    .unwrap();

    // The discard is synchronous with the switch — it does not wait on the
    // incoming root's build. A query landing in that window must see an
    // empty registry rather than the outgoing worktree's skills.
    assert!(
        list_skills(&indexer).is_empty(),
        "the outgoing root's registry is discarded at the moment of the switch, \
         not whenever its replacement happens to finish: {:?}",
        list_skills(&indexer)
            .iter()
            .map(|s| s.folder_name.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        search_skills(&indexer, "cartography", 10).is_empty(),
        "and A's skill cannot be ranked from B"
    );

    // Then the new root builds fresh.
    let on_b = settle(&indexer, "only-in-b");
    assert_eq!(
        on_b.iter().map(|s| s.folder_name.as_str()).collect::<Vec<_>>(),
        vec!["only-in-b"],
        "B's skills alone — nothing from A is carried over or merged"
    );
    assert_eq!(
        on_b[0].path, ".claude/skills/only-in-b/SKILL.md",
        "and it is enumerated under the new content root"
    );
    assert_eq!(search_skills(&indexer, "bathymetry", 10).len(), 1);
    assert!(
        search_skills(&indexer, "cartography", 10).is_empty(),
        "A's skill stays gone once B's build has landed"
    );
}

#[test]
fn rerooting_terminates_operations_scoped_to_the_outgoing_worktree() {
    // PRG-FR-13: a scan in flight against worktree A must reach
    // a terminal state when the active worktree becomes B, so nothing is
    // left running against a root the application has stopped reading.
    //
    // This is the wiring, not the registry — `progress.rs` proves
    // `terminate_scope` picks the right operations, but only this proves the
    // reroot calls it, and calls it while the OUTGOING root is still
    // knowable. Moving the call after `set_root` would cancel the incoming
    // root's operations instead and ship perfectly green.
    use tauri::Manager;
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");

    let app = mock_app_on(&f.root(), &f.root());
    let registry = app.state::<crate::progress::ProgressRegistry>();
    let (_, on_a) = registry.register_at(
        "scan",
        "Indexing…",
        Some(f.root()),
        None,
        std::time::Instant::now(),
    );
    // An operation scoped to the INCOMING root, and one scoped to nothing at
    // all (a plugin install, PRG-FR-15) — neither may be swept up.
    let (_, on_b) = registry.register_at(
        "scan",
        "Indexing…",
        Some(alpha.clone()),
        None,
        std::time::Instant::now(),
    );
    let (_, unscoped) = registry.register_at(
        "install",
        "Installing…",
        None,
        None,
        std::time::Instant::now(),
    );
    let store = GlobalSettingsStore::in_memory();

    reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        &alpha,
    )
    .unwrap();

    let remaining: Vec<String> = app
        .state::<crate::progress::ProgressRegistry>()
        .in_flight()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(
        !remaining.contains(&on_a.id),
        "the outgoing worktree's scan must be terminated: {remaining:?}"
    );
    assert!(
        remaining.contains(&on_b.id),
        "the incoming worktree's own operation must NOT be: {remaining:?}"
    );
    assert!(
        remaining.contains(&unscoped.id),
        "and an unscoped operation survives a re-root entirely: {remaining:?}"
    );
}

#[test]
fn rerooting_remembers_the_worktree_against_the_projects_anchor() {
    // WTC-FR-17 / GSS-FR-16 / GSS-FR-18: keyed by project, so the memory
    // survives and is found again from any of the repository's worktrees.
    use tauri::Manager;
    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = mock_app_on(&f.root(), &f.root());
    let store = GlobalSettingsStore::in_memory();

    reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &store,
        &alpha,
    )
    .unwrap();

    assert_eq!(
        store
            .load_active_worktree(&to_string_path(&f.root()))
            .unwrap(),
        Some(to_string_path(&alpha)),
    );
}

#[test]
fn a_successful_switch_emits_exactly_one_context_changed_event() {
    // WTC-FR-16, first half. The payload carries the new active worktree's
    // path, branch, and detached state.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, Manager};

    let f = Fixture::new();
    f.branch("alpha");
    let alpha = f.add_worktree("wt-alpha", "alpha");
    let app = mock_app_on(&f.root(), &f.root());

    let count = Arc::new(AtomicUsize::new(0));
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let c = Arc::clone(&count);
    let s = Arc::clone(&seen);
    app.listen(WORKTREE_CONTEXT_CHANGED, move |event| {
        c.fetch_add(1, Ordering::SeqCst);
        s.lock().unwrap().push(event.payload().to_string());
    });

    reroot(
        &app.handle(),
        &app.state::<ProjectState>(),
        &app.state::<ProjectWatcher>(),
        &GlobalSettingsStore::in_memory(),
        &alpha,
    )
    .unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 1, "exactly one event per switch");
    let payload = seen.lock().unwrap().join("");
    assert!(payload.contains("\"branch\":\"alpha\""), "{payload}");
    assert!(payload.contains("\"isDetached\":false"), "{payload}");
    assert!(payload.contains(&to_string_path(&alpha)), "{payload}");
}

#[test]
fn a_refused_activation_emits_nothing_and_unmounts_nothing() {
    // WTC-FR-16, second half + WTC-FR-09: validation runs to completion
    // before the current content root is touched, so a rejected activation
    // leaves the project exactly where it was and announces nothing.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::{Listener, Manager};

    let f = Fixture::new();
    let app = mock_app_on(&f.root(), &f.root());
    let count = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&count);
    app.listen(WORKTREE_CONTEXT_CHANGED, move |_| {
        c.fetch_add(1, Ordering::SeqCst);
    });

    let stranger = TempDir::new().unwrap();
    let err = validate_activation_target(
        &f.root(),
        &stranger.path().to_string_lossy(),
    )
    .unwrap_err();

    assert_eq!(err, ERR_NOT_A_WORKTREE);
    assert_eq!(count.load(Ordering::SeqCst), 0, "a failed operation emits nothing");
    assert_eq!(
        app.state::<ProjectState>().require_root().unwrap().path(),
        f.root(),
        "and the content root is unchanged"
    );
}
