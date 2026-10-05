//! One hit per file, at the first matching line (SCC-FR-07).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-07 — one hit per file, first matching line
// -----------------------------------------------------------------------

#[test]
fn ts8_a_file_matching_twice_produces_one_hit_at_the_first_line() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "twice.md", "one\ntwo\nneedle A\nfour\nfive\nsix\nneedle B\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "at most one hit per file: {hits:?}");
    assert_eq!(hits[0].match_kind, MatchKind::Content);
    assert_eq!(hits[0].line, Some(3));
    assert_eq!(hits[0].snippet.as_deref(), Some("needle A"));
}

#[test]
fn ts9_a_path_only_match_carries_no_line_or_snippet() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "search-notes.md", "this body mentions nothing relevant\n");

    let (sink, _) = search(root, "search", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].match_kind, MatchKind::Name);
    assert_eq!(hits[0].line, None);
    assert_eq!(hits[0].snippet, None);
    // And the omitted fields really are absent from the wire shape.
    let json = serde_json::to_value(&hits[0]).unwrap();
    assert!(json.get("line").is_none());
    assert!(json.get("snippet").is_none());
}

#[test]
fn a_file_matching_both_path_and_content_reports_the_content_match() {
    // SCC-FR-07: content outranks path, so the user sees where in the file
    // the query is rather than only that the name matched.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "needle.md", "line one\nthe needle is here\n");

    let (sink, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].match_kind, MatchKind::Content);
    assert_eq!(hits[0].line, Some(2));
}
