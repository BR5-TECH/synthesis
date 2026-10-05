//! The tests of filesystem watching and of the change routing behind it
//! (ASC-FR-10/14/15, PST-FR-16).

use super::*;

#[test]
fn external_divergence_only_fires_for_changed_tracked_files() {
    // PST-FR-16: emit only when a TRACKED file's checksum actually changed.
    assert_eq!(
        external_divergence(Some("a"), Some("b")),
        Some("b".to_string()),
        "tracked file with a new checksum -> external divergence"
    );
    assert_eq!(
        external_divergence(Some("a"), Some("a")),
        None,
        "unchanged checksum is the Editor's own save (self-write) -> quiet"
    );
    assert_eq!(
        external_divergence(None, Some("b")),
        None,
        "untracked path is not a served artifact -> quiet"
    );
    assert_eq!(
        external_divergence(Some("a"), None),
        None,
        "removed file is a structural delete, not a divergence -> quiet"
    );
    assert_eq!(external_divergence(None, None), None);
}

#[test]
fn route_watch_change_splits_content_quiet_and_structural() {
    // PST-FR-16 / ASC-FR-15: the routing the watcher composes over a batch.
    // A tracked file whose checksum changed -> the content channel.
    assert_eq!(
        route_watch_change(Some("a"), Some("b")),
        WatchRoute::Content("b".to_string())
    );
    // A tracked file unchanged (the Editor's own save) -> quiet, NOT a tree
    // reload (PST-FR-16: a content path never cross-fires to structural).
    assert_eq!(route_watch_change(Some("a"), Some("a")), WatchRoute::Quiet);
    // An untracked path -> structural (tree reload; its content may have
    // just become classifiable).
    assert_eq!(route_watch_change(None, Some("b")), WatchRoute::Structural);
    // A tracked file that was deleted -> structural (the tree must drop it),
    // never the content channel.
    assert_eq!(route_watch_change(Some("a"), None), WatchRoute::Structural);
    assert_eq!(route_watch_change(None, None), WatchRoute::Structural);
}

// -----------------------------------------------------------------------
// CHC-FR-16 — the Git-directory watch surface
// -----------------------------------------------------------------------

/// A repository whose project root is reached through a symlink, which is
/// the ordinary case on macOS (`/tmp` -> `/private/tmp`, and `TempDir`
/// under `/var` -> `/private/var`).
fn repo_at(dir: &Path) -> git2::Repository {
    let repo = git2::Repository::init(dir).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    drop(config);
    repo
}

#[test]
fn the_watched_git_dir_and_the_attributed_git_dir_are_the_same_path() {
    // The bug this pins: notify reports event paths under the path it was
    // handed, so if the watch registered a canonical path while events were
    // stripped against a raw one, every strip would fail and the Git channel
    // would go silent — on a symlinked root, i.e. every macOS temp dir, and
    // any repository reached through a symlink.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    drop(repo_at(root));

    let (attributed, extra) = git_watch_targets(root);
    let attributed = attributed.expect("the repository resolves");
    assert_eq!(
        attributed,
        crate::changes::canonicalize_lenient(&attributed),
        "the attributed Git directory must already be canonical"
    );
    if let Some(watched) = extra {
        assert_eq!(
            watched, attributed,
            "an event seen under the watched path must strip against the attributed one"
        );
    }

    // And the strip actually works for a real event path under it.
    let event = attributed.join("refs/heads/main");
    assert!(
        event.strip_prefix(&attributed).is_ok(),
        "a Git-directory event must be attributable"
    );
}

#[test]
fn a_co_located_projects_git_dir_is_watched_and_its_events_are_counted() {
    // CHC-FR-16 wiring: the host repository is above the project
    // root, so the Git directory needs its own watch AND events under it
    // must reach `changes_change_count`.
    let dir = tempfile::TempDir::new().unwrap();
    let repo_root = dir.path();
    drop(repo_at(repo_root));
    let project_root = repo_root.join("apps/web");
    std::fs::create_dir_all(&project_root).unwrap();

    let (attributed, extra) = git_watch_targets(&project_root);
    let attributed = attributed.expect("discover walks up to the host repo");
    let watched = extra.expect("a co-located Git directory needs its own watch");
    assert_eq!(watched, attributed);

    // A commit moves HEAD and a ref, touching no working-tree file at all.
    let events = vec![
        notify_debouncer_mini::DebouncedEvent {
            path: attributed.join("HEAD"),
            kind: notify_debouncer_mini::DebouncedEventKind::Any,
        },
        notify_debouncer_mini::DebouncedEvent {
            path: attributed.join("refs/heads/main"),
            kind: notify_debouncer_mini::DebouncedEventKind::Any,
        },
    ];
    assert_eq!(
        crate::changes::changes_change_count(
            &Ok(events),
            &project_root,
            Some(&attributed),
        ),
        2,
        "a commit in a co-located repository must be a change (CHC-FR-16)"
    );
}

#[test]
fn a_standalone_project_needs_no_second_watch() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    drop(repo_at(root));

    let (attributed, extra) = git_watch_targets(root);
    assert!(attributed.is_some());
    assert_eq!(
        extra, None,
        "`.git` is already inside the recursive watch on the project root; \
             a second watch would only duplicate every event"
    );
}

#[test]
fn a_project_that_is_not_a_repository_has_nothing_to_attribute_or_watch() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_eq!(git_watch_targets(dir.path()), (None, None));
}

#[test]
fn a_watched_change_invalidates_the_candidate_list() {
    // ASC-FR-10 / ASC-FR-15 / ASC-FR-17: the watcher is what keeps the
    // candidate list current, so a search never matches against a tree that
    // has moved on. Without this the list is only ever built once per
    // content root and a newly-created file stays unsearchable for the life
    // of the project.
    //
    // The watcher's callback needs a real (headless) app, since it reaches
    // the store through managed state — the same route production takes.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    // Canonical, for the reason `git_watch_targets` documents above: notify
    // reports event paths under the path it was handed, and a macOS temp dir
    // is reached through a symlink (`/var` -> `/private/var`). Watching the
    // uncanonical path would make every `strip_prefix` in the callback fail,
    // and the test would be asserting against a channel that is silent for
    // reasons unrelated to what it is testing.
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::write(root.join("a.md"), b"one").unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(watcher.is_active(), "precondition: the watcher is live");

    // Mount a list, then change the tree underneath it.
    let store = app.state::<scanning::CandidateStore>();
    let _ = store.candidates(&crate::fs::RootFs::for_root(root));
    assert!(store.is_mounted());
    std::fs::write(root.join("b.md"), b"two").unwrap();

    // The debouncer coalesces over 300ms; give it a bounded window rather
    // than a fixed sleep.
    let mut invalidated = false;
    for _ in 0..100 {
        if !store.is_mounted() {
            invalidated = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        invalidated,
        "a filesystem change must invalidate the mounted candidate list"
    );

    // And the rebuilt list holds the new file.
    let rebuilt = store.candidates(&crate::fs::RootFs::for_root(root));
    assert!(
        rebuilt.iter().any(|c| c.path == "b.md"),
        "the rebuilt list must see the new file: {:?}",
        rebuilt.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
}

#[test]
fn attribution_is_self_write_only_for_an_exact_checksum_match() {
    // The asymmetry is the point: only a recorded baseline matching what is
    // on disk is our own echo. Everything else reads as EXTERNAL, because
    // being wrong that way costs one redundant pass while being wrong the
    // other way costs a classification stale for the life of the project.
    assert!(
        attribution_is_self_write(Some("a"), Some("a")),
        "recorded baseline matching disk -> our own write"
    );
    assert!(
        !attribution_is_self_write(Some("a"), Some("b")),
        "disk moved away from the baseline -> an external assignment"
    );
    assert!(
        !attribution_is_self_write(None, Some("a")),
        "no baseline (we never wrote it) -> external"
    );
    assert!(
        !attribution_is_self_write(Some("a"), None),
        "file gone or unreadable -> external, not a self-write"
    );
    assert!(!attribution_is_self_write(None, None));
}

#[test]
fn the_attribution_file_never_reaches_the_per_path_routing_loop() {
    // ASC-FR-09: it rides the raw channel (ASC-FR-20) but is not a tree
    // node, so the split must happen before the routing loop — otherwise it
    // would be handed to `route_watch_change`, come back `Structural` for
    // want of a tracker entry, and be emitted as a removed path that
    // TAB-FR-19 would try to close a tab for.
    use notify_debouncer_mini::{DebouncedEvent, DebouncedEventKind};
    let root = Path::new("/p");
    let ev = |p: &str| DebouncedEvent {
        path: PathBuf::from(p),
        kind: DebouncedEventKind::Any,
    };
    let batch = Ok(vec![
        ev("/p/a.md"),
        ev("/p/.synthesis/library.toml"),
        ev("/p/sub/b.md"),
    ]);

    let mut changed = scanning::changed_rel_paths(&batch, root);
    assert!(
        changed.contains(&scanning::LIBRARY_TOML_REL.to_string()),
        "precondition: the channel carries it"
    );

    // `split_attribution` itself — the same function the callback calls, not
    // a copy of it, so re-merging the two fails here rather than silently
    // closing a tab for a deleted attribution file.
    assert!(
        split_attribution(&mut changed),
        "the split must report that it observed the attribution file"
    );
    assert_eq!(changed, vec!["a.md".to_string(), "sub/b.md".to_string()]);

    // And it is absent from what `removed_rel_paths` could ever be handed,
    // which is the TAB-FR-19 hazard the split exists for.
    assert!(!changed.contains(&scanning::LIBRARY_TOML_REL.to_string()));

    // A batch without it reports false and is left alone.
    let mut ordinary = vec!["a.md".to_string()];
    assert!(!split_attribution(&mut ordinary));
    assert_eq!(ordinary, vec!["a.md".to_string()]);
}

#[test]
fn an_external_attribution_write_invalidates_the_candidate_list() {
    // BMI-FR-17 / BMI-FR-18, the regression this fix exists for: a `git
    // pull` carrying a teammate's folder-scope curation, or an agentic CLI
    // editing the file, rewrites `.synthesis/library.toml` and NOTHING else.
    // Before the fix that batch reached no consumer, so every affected
    // file's resolved type stayed stale for the life of the project.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    // Canonical, for the reason `git_watch_targets` documents.
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/x.md"), b"# X").unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(watcher.is_active(), "precondition: the watcher is live");

    let store = app.state::<scanning::CandidateStore>();
    let _ = store.candidates(&crate::fs::RootFs::for_root(root));
    assert!(store.is_mounted());

    // No baseline is recorded, so this is external — exactly the shape a
    // pull produces.
    std::fs::write(
        root.join(scanning::LIBRARY_TOML_REL),
        "[assignments]\n[assignments.\"notes\"]\ntype = \"scenario\"\nscope = \"folder\"\n",
    )
    .unwrap();

    let mut invalidated = false;
    for _ in 0..100 {
        if !store.is_mounted() {
            invalidated = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        invalidated,
        "an external attribution write must invalidate the mounted list"
    );

    // And the rebuilt list carries the inherited type, which is what a pass
    // reads to decide index membership (BMI-FR-03).
    let rebuilt = store.candidates(&crate::fs::RootFs::for_root(root));
    assert_eq!(
        rebuilt
            .iter()
            .find(|c| c.path == "notes/x.md")
            .and_then(|c| c.artifact_type),
        Some(scanning::ArtifactType::Scenario),
        "the pulled folder-scope assignment must resolve onto the file"
    );
}

/// The `"project tree changed"` payloads a test observed, as parsed JSON.
///
/// Asserting on the payload rather than only on the candidate store is what
/// pins `change_count` and keeps `.synthesis/library.toml` out of
/// `removed_paths` (TAB-FR-19).
fn collect_tree_events(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> {
    use tauri::Listener;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = std::sync::Arc::clone(&seen);
    app.listen(PROJECT_TREE_CHANGED, move |event| {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            s.lock().unwrap().push(v);
        }
    });
    seen
}

/// Poll a condition over a bounded window rather than sleeping a fixed one.
fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn an_applications_own_attribution_write_is_suppressed_while_the_batch_still_lands() {
    // The other half: `assign_artifact_type` publishes the reclassification
    // itself and records the baseline, so the watcher's echo of that same
    // write must not count as a change.
    //
    // Proved with a POSITIVE barrier rather than a sleep. A fixed sleep
    // asserting an absence passes green for every reason the event could
    // fail to arrive — a watcher that never started, a debouncer that failed
    // to init (both of which `start_watching` only `eprintln!`s), or an
    // FSEvents cold start slower than the guess. Writing an ordinary file in
    // the same burst forces the pipeline to prove it is alive: the event
    // MUST arrive, and what it carries is what the suppression is read off.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    let contents = "[assignments]\n[assignments.\"notes\"]\ntype = \"scenario\"\nscope = \"folder\"\n";
    std::fs::write(root.join(scanning::LIBRARY_TOML_REL), contents).unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());
    // What `republish_classification` does after writing.
    app.state::<scanning::AttributionBaseline>()
        .record_from_disk(&crate::fs::RootFs::for_root(root));

    // One burst: the echo of our own attribution write, plus one ordinary
    // path that has to be reported. Asserted **per event**, never summed.
    //
    // Summing `changeCount` across the burst used to be the assertion, and
    // it was wrong: `change_count` is `structural_paths.len() +
    // attribution_changed`, and a structural path — unlike a content change,
    // which the tracker adopts a checksum for — is re-reported if the
    // watcher hands it to a second debounce window. One `std::fs::write`
    // routinely produces several filesystem events, so on a loaded machine
    // the ordinary path alone lands as two events of one each, which summed
    // is indistinguishable from a suppression regression.
    //
    // The ordinary half is a **removal** so that `removedPaths` names it,
    // which is what makes a split batch tell its own story rather than
    // needing to be timed around:
    //
    //   - one batch, suppression broken -> `changeCount: 2`.
    //   - split batch, suppression broken -> an extra event carrying
    //     `changeCount: 1` with an EMPTY `removedPaths`, since the
    //     attribution file is never a removed path (it backs no tab).
    //   - the same removal re-reported in a later window -> another
    //     `{1, ["b.md"]}`, which passes, because it is not a regression.
    //
    // So every timing arrangement is either caught or correctly ignored, and
    // no assertion here rests on a guess about delivery latency.
    std::fs::write(root.join("b.md"), b"two").unwrap();
    let seen = collect_tree_events(&app);
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(
        watcher.is_active(),
        "precondition: a dead watcher would make every assertion below vacuous"
    );

    std::fs::write(root.join(scanning::LIBRARY_TOML_REL), contents).unwrap();
    std::fs::remove_file(root.join("b.md")).unwrap();

    assert!(
        wait_until(|| !seen.lock().unwrap().is_empty()),
        "the removed file must produce an event — the barrier proving the \
             watcher is alive"
    );
    // Let the burst finish arriving, however many windows it took. A
    // straggler is not a hazard here: the assertion below holds for every
    // event individually, so one more of them changes nothing.
    std::thread::sleep(Duration::from_millis(400));

    for e in seen.lock().unwrap().iter() {
        let removed: Vec<&str> = e
            .get("removedPaths")
            .and_then(|r| r.as_array())
            .map(|a| a.iter().filter_map(|p| p.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            (e.get("changeCount").and_then(|c| c.as_u64()), removed),
            (Some(1), vec!["b.md"]),
            "every event in this burst is the removal and nothing else — \
                 our own attribution write must contribute nothing: {e:?}"
        );
    }
}

/// Every `"dashboard recently edited changed"` this app handle sees.
fn collect_dashboard_events(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
    use tauri::Listener;
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let s = std::sync::Arc::clone(&seen);
    app.listen(crate::dashboard::DASHBOARD_RECENTLY_EDITED_CHANGED, move |_| {
        s.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    seen
}

/// PST-FR-35: a write to an artifact's source file moves the Recently
/// edited widget — and a write **this application performed** moves it too.
///
/// The self-write half is the one worth a test of its own. Every other
/// channel here suppresses our own writes (PST-FR-16), and this one
/// deliberately does not: the widget is ordered by the file's modification
/// time, which our own write moved, so suppressing it would leave the
/// artifact sitting where it was in the widget that exists to say it just
/// changed. Nothing else in the codebase states that, so a well-meaning
/// "make it consistent with the other channel" edit would break it silently.
#[test]
fn an_artifact_write_moves_the_recently_edited_widget_ours_included() {
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::write(root.join("a.spec.md"), b"one").unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());
    let rooted = crate::fs::RootFs::for_root(root);
    // What a save leaves behind: the checksum baseline that routes the
    // watcher's echo of it to `WatchRoute::Quiet` (PST-FR-16).
    let checksum = rooted.sha256_file("a.spec.md").unwrap();
    app.state::<ContentTracker>().record("a.spec.md", &checksum);

    let seen = collect_dashboard_events(&app);
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &rooted);
    assert!(
        watcher.is_active(),
        "precondition: a dead watcher would make every assertion below vacuous"
    );

    // An external editor rewrites it: the content channel, and the widget
    // must move.
    std::fs::write(root.join("a.spec.md"), b"two").unwrap();
    assert!(
        wait_until(|| seen.load(std::sync::atomic::Ordering::SeqCst) > 0),
        "an external write to an artifact moves the widget (PST-FR-35)"
    );

    // Now our own. The checksum is recorded BEFORE the bytes reach disk,
    // exactly as `save_artifact_contents` records it — so the watcher sees
    // one write, routes it `Quiet` as our own, and the count below rises
    // only if the quiet route reaches the widget. Writing first and
    // checksumming after would put an external change in front of it and
    // make this half pass for the wrong reason.
    let before = seen.load(std::sync::atomic::Ordering::SeqCst);
    app.state::<ContentTracker>()
        .record("a.spec.md", &crate::fs::sha256_bytes(b"three"));
    std::fs::write(root.join("a.spec.md"), b"three").unwrap();
    assert!(
        wait_until(|| seen.load(std::sync::atomic::Ordering::SeqCst) > before),
        "a write this application performed moves the widget too — the \
             modification time it gave the file is exactly what the widget orders by"
    );
}

/// PST-FR-35: a content change to a path carrying **no** artifact type
/// leaves the widget where it is, that file being no artifact (PST-FR-23).
///
/// A *structural* change to it still moves the widget, because it may have
/// changed what the enumeration holds — so the negative half is stated over
/// a tracked file whose content alone changed.
#[test]
fn a_content_edit_to_an_unclassified_file_moves_the_recently_edited_widget_not_at_all() {
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());
    let rooted = crate::fs::RootFs::for_root(root);
    let checksum = rooted.sha256_file("src/main.rs").unwrap();
    app.state::<ContentTracker>().record("src/main.rs", &checksum);

    let seen = collect_dashboard_events(&app);
    let tree = collect_tree_events(&app);
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &rooted);
    assert!(watcher.is_active(), "precondition");

    // A POSITIVE barrier, for the reason the attribution suite states one:
    // a fixed sleep asserting an absence passes for every reason the event
    // could fail to arrive. The artifact's removal MUST produce a tree
    // event, and by the time it has, the content edit beside it has been
    // seen too.
    std::fs::write(root.join("b.spec.md"), b"artifact").unwrap();
    assert!(
        wait_until(|| !tree.lock().unwrap().is_empty()),
        "the new artifact must produce a tree event — the barrier proving \
             the watcher is alive"
    );
    let after_structural = seen.load(std::sync::atomic::Ordering::SeqCst);

    std::fs::write(root.join("src/main.rs"), b"fn main() { /* edited */ }").unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        seen.load(std::sync::atomic::Ordering::SeqCst),
        after_structural,
        "a file carrying no artifact type is not an artifact, so editing it \
             moves nothing in a widget that enumerates artifacts (PST-FR-23)"
    );
}

#[test]
fn an_external_attribution_write_is_counted_and_never_reported_as_removed() {
    // ASC-FR-22 / TAB-FR-19: the payload of an attribution-only burst. The
    // count must be 1 — never 0, which is a shape no consumer has been given
    // and which a truthiness gate would drop — and `removedPaths` must stay
    // empty even when the attribution file is DELETED, because no tab is
    // backed by it.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::write(
        root.join(scanning::LIBRARY_TOML_REL),
        "[assignments]\n[assignments.\"notes\"]\ntype = \"scenario\"\nscope = \"folder\"\n",
    )
    .unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());

    let seen = collect_tree_events(&app);
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(watcher.is_active(), "precondition");

    // Deleted, not rewritten: the case where `sha256_file` fails and the
    // path would otherwise look like a structural removal.
    std::fs::remove_file(root.join(scanning::LIBRARY_TOML_REL)).unwrap();

    assert!(
        wait_until(|| !seen.lock().unwrap().is_empty()),
        "deleting the attribution file must still reach the tree channel"
    );
    let events = seen.lock().unwrap().clone();
    for e in &events {
        assert_eq!(
            e.get("removedPaths").and_then(|r| r.as_array()).map(|a| a.len()),
            Some(0),
            "the attribution file backs no tab, so it is never a removed \
                 path: {e:?}"
        );
        assert!(
            e.get("changeCount").and_then(|c| c.as_u64()).unwrap_or(0) >= 1,
            "an attribution change counts as one, never zero: {e:?}"
        );
    }
}

#[test]
fn a_pull_rewriting_the_attribution_file_and_ordinary_files_lands_as_one_burst() {
    // BMI-FR-18 / ASC-FR-10: the realistic shape of a pull — it rewrites
    // `.synthesis/library.toml` AND files, in one debounce window. Both
    // halves must be accounted for in one event, and the attribution file
    // must not leak into `removedPaths` alongside the ordinary ones.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());

    let seen = collect_tree_events(&app);
    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(watcher.is_active(), "precondition");

    std::fs::write(
        root.join(scanning::LIBRARY_TOML_REL),
        "[assignments]\n[assignments.\"notes\"]\ntype = \"spec\"\nscope = \"folder\"\n",
    )
    .unwrap();
    std::fs::write(root.join("notes/a.md"), b"# A").unwrap();
    std::fs::write(root.join("notes/b.md"), b"# B").unwrap();

    assert!(
        wait_until(|| {
            let total: u64 = seen
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| e.get("changeCount").and_then(|c| c.as_u64()))
                .sum();
            total >= 3
        }),
        "both ordinary files and the attribution change must be counted: {:?}",
        seen.lock().unwrap()
    );
    for e in seen.lock().unwrap().iter() {
        let removed = e
            .get("removedPaths")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            !removed.iter().any(|p| p.as_str() == Some(scanning::LIBRARY_TOML_REL)),
            "the attribution file is never a removed path: {e:?}"
        );
    }
}

#[test]
fn reverting_the_attribution_file_to_bytes_this_application_wrote_is_still_external() {
    // The baseline names what consumers last SAW, not what we last WROTE.
    //
    // A baseline that only moved on our own writes goes permanently stale
    // here: we write A; a teammate's B arrives and is reported; then a `git
    // checkout` back to the previous branch restores A — which still matches
    // the baseline and would be suppressed, leaving every consumer on B's
    // classification for the life of the project. BMI-FR-18 names checkout
    // explicitly, so this is the most reachable path to the bug, not a
    // corner of one.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    let a = "[assignments]\n[assignments.\"notes\"]\ntype = \"scenario\"\nscope = \"folder\"\n";
    let b = "[assignments]\n[assignments.\"notes\"]\ntype = \"prompt\"\nscope = \"folder\"\n";
    std::fs::write(root.join(scanning::LIBRARY_TOML_REL), a).unwrap();

    let app = tauri::test::mock_app();
    app.manage(scanning::CandidateStore::default());
    app.manage(ContentTracker::default());
    app.manage(scanning::AttributionBaseline::default());
    // As if this application had just written `a`.
    app.state::<scanning::AttributionBaseline>()
        .record_from_disk(&crate::fs::RootFs::for_root(root));

    let watcher = ProjectWatcher::default();
    start_watching(&app.handle().clone(), &watcher, &crate::fs::RootFs::for_root(root));
    assert!(watcher.is_active(), "precondition");
    let store = app.state::<scanning::CandidateStore>();

    // A teammate's `b` arrives and is reported.
    let _ = store.candidates(&crate::fs::RootFs::for_root(root));
    std::fs::write(root.join(scanning::LIBRARY_TOML_REL), b).unwrap();
    assert!(
        wait_until(|| !store.is_mounted()),
        "the external write to `b` must invalidate"
    );

    // A checkout restores `a` — bytes this application once wrote.
    let _ = store.candidates(&crate::fs::RootFs::for_root(root));
    assert!(store.is_mounted(), "precondition: remounted before the revert");
    std::fs::write(root.join(scanning::LIBRARY_TOML_REL), a).unwrap();
    assert!(
        wait_until(|| !store.is_mounted()),
        "a revert to bytes we once wrote is still an external change; \
             suppressing it strands every consumer on the superseded \
             classification"
    );
}

#[test]
fn project_watcher_stop_is_idempotent_and_clears_active() {
    // ASC-FR-14: tearing down the watcher leaves no active watcher; calling
    // stop again is a harmless no-op. (A fresh watcher starts inactive.)
    let watcher = ProjectWatcher::default();
    assert!(!watcher.is_active(), "fresh watcher must be inactive");
    watcher.stop();
    assert!(!watcher.is_active(), "stop on an inactive watcher is a no-op");
}
