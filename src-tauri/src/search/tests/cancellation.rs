//! Supersession and cancellation (SCC-FR-12, SCC-FR-13, SCC-FR-14).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-12, SCC-FR-13 / SCC-FR-14 — supersession and cancellation
// -----------------------------------------------------------------------

#[test]
fn ts15_starting_a_search_supersedes_the_running_one() {
    // SCC-FR-12: the registry is where "at most one at a time" lives, and
    // the superseded search learns WHY it is stopping so its one terminal
    // event carries `superseded` rather than `cancelled` (SCC-FR-13).
    let registry = SearchRegistry::default();
    let first_id = registry.next_id();
    let first = registry.install(&first_id);
    assert_eq!(first.reason(), None, "still running");

    let second_id = registry.next_id();
    let second = registry.install(&second_id);
    assert_ne!(first_id, second_id, "ids are never reused");
    assert_eq!(first.reason(), Some(EndReason::Superseded));
    assert_eq!(second.reason(), None, "the newcomer proceeds normally");

    // And the superseded search retiring must not evict its successor.
    registry.retire(&first_id);
    registry.cancel(&second_id);
    assert_eq!(second.reason(), Some(EndReason::Cancelled));
}

#[test]
fn ts16_cancellation_ends_the_search_once_and_a_second_cancel_is_a_noop() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..200 {
        write(root, &format!("f{n:04}.md"), "needle\n");
    }
    let candidates = Arc::new(scanning::candidate_files(&scanning::scan(root)));

    let stop = Stop::new();
    // Cancelled before it starts, so no hit can be emitted after the stop —
    // which is the invariant, and the only one a test can pin without racing.
    stop.cancel();
    let sink = Recorder::default();
    let reason = run_search(
        &sink,
        root,
        candidates,
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Full,
        &stop,
        "s",
        &|_| {},
    );
    assert_eq!(reason, EndReason::Cancelled);
    assert_eq!(sink.end_reason(), EndReason::Cancelled);
    assert!(sink.hits().is_empty(), "{:?}", sink.hits());

    // A second cancel for an id that has already ended is a no-op.
    let registry = SearchRegistry::default();
    registry.cancel("search-does-not-exist");
    let id = registry.next_id();
    let running = registry.install(&id);
    registry.cancel(&id);
    registry.cancel(&id);
    assert_eq!(running.reason(), Some(EndReason::Cancelled));
}

#[test]
fn a_search_cancelled_mid_sweep_still_ends_exactly_once() {
    // SCC-FR-13 / SCC-FR-14: whatever moment the cancel lands in, the search
    // emits its one terminal event and stops.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..400 {
        write(root, &format!("f{n:04}.md"), "needle\n");
    }
    let candidates = Arc::new(scanning::candidate_files(&scanning::scan(root)));
    let stop = Stop::new();
    let canceller = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            stop.cancel();
        })
    };
    let sink = Recorder::default();
    run_search(
        &sink,
        root,
        candidates,
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Full,
        &stop,
        "s",
        &|_| {},
    );
    canceller.join().unwrap();
    // Exactly one terminal event; the reason depends on whether the sweep
    // finished before the cancel landed, and both are legitimate.
    let reason = sink.end_reason();
    assert!(
        matches!(reason, EndReason::Cancelled | EndReason::Completed),
        "{reason:?}"
    );
}
