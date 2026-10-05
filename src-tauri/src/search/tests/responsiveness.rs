//! Streaming and the no-project case (SCC-FR-02, SCC-FR-17, SCC-FR-19).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-02, SCC-FR-19 / SCC-FR-17 — responsiveness and the no-project case
// -----------------------------------------------------------------------

#[test]
fn ts19_a_search_over_no_candidates_ends_completed_with_no_hits() {
    // SCC-FR-17's shape: no candidates to match -> an id, no hits, and an
    // immediate `completed`. (The command's no-project branch takes the same
    // path; what it adds is a Tauri runtime, which this asserts without.)
    let dir = tempfile::TempDir::new().unwrap();
    let sink = Recorder::default();
    let reason = run_search(
        &sink,
        &crate::fs::RootFs::for_root(dir.path()),
        Arc::new(Vec::new()),
        Arc::new(Matcher::compile("anything", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Capped,
        &Stop::new(),
        "s",
        &|_| {},
    );
    assert_eq!(reason, EndReason::Completed);
    assert_eq!(sink.end_reason(), EndReason::Completed);
    assert!(sink.hits().is_empty());
    assert_eq!(sink.batch_count(), 0, "no empty batch is emitted");
}

#[test]
fn ts2_hits_stream_rather_than_arriving_all_at_the_end() {
    // SCC-FR-19: a hit found early is delivered while the rest of the tree is
    // still being matched. Many matches spread over many files, so the
    // collector's interval elapses more than once during the sweep.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..300 {
        write(root, &format!("f{n:04}.md"), "needle\n");
    }
    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    assert_eq!(sink.hits().len(), 300);
    assert!(
        sink.batch_count() > 1,
        "300 hits arrived as one batch: hits are being accumulated, not streamed"
    );
    assert!(
        sink.batch_count() < 300,
        "the event bus must not be flooded one event per hit, got {}",
        sink.batch_count()
    );
}
