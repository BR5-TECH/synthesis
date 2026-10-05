//! Oversized and non-text candidates (SCC-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-15 — oversized and non-text candidates
// -----------------------------------------------------------------------

#[test]
fn ts17_binary_content_is_never_scanned_but_stays_findable_by_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    // Invalid UTF-8 whose bytes contain the query text.
    std::fs::write(root.join("asset.bin"), b"\xff\xfe needle \xff").unwrap();
    std::fs::write(root.join("needle-asset.bin"), b"\xff\xfe nothing \xff").unwrap();

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "only the path match: {hits:?}");
    assert_eq!(hits[0].path, "needle-asset.bin");
    assert_eq!(hits[0].match_kind, MatchKind::Name);
}

#[test]
fn an_oversized_file_is_not_read_line_by_line() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let mut body = String::with_capacity(MAX_CONTENT_BYTES as usize + 64);
    while body.len() <= MAX_CONTENT_BYTES as usize {
        body.push_str("padding padding padding padding\n");
    }
    body.push_str("needle\n");
    write(root, "huge.log", &body);
    write(root, "small.log", "needle\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    assert_eq!(paths, vec!["small.log".to_string()], "{paths:?}");
}
