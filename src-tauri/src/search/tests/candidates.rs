//! Where the candidates come from, and one that goes away mid-run
//! (SCC-FR-03, SCC-FR-04).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-03 — the candidates are the scan's, so the exclusions are inherited
// -----------------------------------------------------------------------

#[test]
fn ts3_excluded_paths_never_produce_a_hit() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, ".gitignore", "node_modules/\n");
    write(root, "src/keep.rs", "needle\n");
    write(root, "node_modules/pkg/index.js", "needle\n");
    write(root, ".git/objects/abc", "needle\n");
    write(root, ".synthesis/cache/db", "needle\n");
    write(root, ".synthesis/drafts/wip.md", "needle\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    assert_eq!(paths, vec!["src/keep.rs".to_string()], "{paths:?}");
}

// -----------------------------------------------------------------------
// SCC-FR-04 — a candidate removed between enumeration and matching
// -----------------------------------------------------------------------

#[test]
fn ts4_a_candidate_deleted_after_enumeration_is_skipped_not_fatal() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "gone.md", "needle\n");
    write(root, "stays.md", "needle\n");

    let candidates = Arc::new(scanning::candidate_files(&scanning::scan(root)));
    assert_eq!(candidates.len(), 2);
    // Enumerated, then removed before any consumer reaches it.
    std::fs::remove_file(root.join("gone.md")).unwrap();

    let sink = Recorder::default();
    let reason = run_search(
        &sink,
        root,
        candidates,
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Full,
        &Stop::new(),
        "s",
        &|_| {},
    );
    assert_eq!(reason, EndReason::Completed, "a missing file is not fatal");
    let paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    assert_eq!(paths, vec!["stays.md".to_string()], "{paths:?}");
}
