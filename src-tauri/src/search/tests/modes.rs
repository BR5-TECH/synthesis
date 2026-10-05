//! The three modes and an uncompilable expression (SCC-FR-05, SCC-FR-06).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-05 — the three modes
// -----------------------------------------------------------------------

#[test]
fn ts5_smart_case_respects_case_only_once_the_query_has_some() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "upper.md", "a Widget here\n");
    write(root, "lower.md", "a widget here\n");

    for mode in [SearchMode::LiteralInsensitive, SearchMode::SmartCase] {
        let (sink, _) = search(root, "widget", mode, SearchScope::Full);
        assert_eq!(sink.hits().len(), 2, "lowercase query matches both in {mode:?}");
    }

    let (sink, _) = search(root, "Widget", SearchMode::SmartCase, SearchScope::Full);
    let paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    assert_eq!(paths, vec!["upper.md".to_string()], "{paths:?}");
}

#[test]
fn ts6_literal_modes_assign_no_meaning_to_regex_metacharacters() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "meta.md", "a.c literally\n");
    write(root, "plain.md", "abc plainly\n");

    let (sink, _) = search(root, "a.c", SearchMode::LiteralInsensitive, SearchScope::Full);
    let paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    assert_eq!(paths, vec!["meta.md".to_string()], "literal: only the literal");

    let (sink, _) = search(root, "a.c", SearchMode::Regex, SearchScope::Full);
    let mut paths: Vec<String> = sink.hits().into_iter().map(|h| h.path).collect();
    paths.sort();
    assert_eq!(paths, vec!["meta.md".to_string(), "plain.md".to_string()]);
}

// -----------------------------------------------------------------------
// SCC-FR-06 — an uncompilable regular expression
// -----------------------------------------------------------------------

#[test]
fn ts7_an_uncompilable_regex_is_the_typed_invalid_query_error() {
    assert_eq!(
        Matcher::compile("foo(", SearchMode::Regex).unwrap_err(),
        INVALID_QUERY
    );
    // The same text is a perfectly ordinary literal in the other two modes.
    assert!(Matcher::compile("foo(", SearchMode::LiteralInsensitive).is_ok());
    assert!(Matcher::compile("foo(", SearchMode::SmartCase).is_ok());
}
