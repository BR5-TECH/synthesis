//! The candidate list is the scan's file nodes (ASC-FR-17).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// The candidate list (ASC-FR-17)
// -----------------------------------------------------------------------

#[test]
fn asc_ts19_the_candidate_list_is_the_scans_file_nodes_and_is_torn_down() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, ".gitignore", "node_modules/\n");
    write(root, ".claude/skills/s.md", "x");
    write(root, "src/main.rs", "x");
    write(root, "node_modules/pkg/i.js", "x");
    write(root, ".git/objects/a", "x");

    let store = CandidateStore::default();
    assert!(!store.is_mounted(), "nothing is mounted before the first use");
    let first = store.candidates(root);
    assert!(store.is_mounted());
    let paths: Vec<&str> = first.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&".claude/skills/s.md"), "{paths:?}");
    assert!(paths.contains(&"src/main.rs"), "unclassified files are candidates too");
    assert!(!paths.iter().any(|p| p.starts_with("node_modules/")));
    assert!(!paths.iter().any(|p| p.starts_with(".git/")));
    // Deterministic: ordinals are dense and in enumeration order.
    for (index, candidate) in first.iter().enumerate() {
        assert_eq!(candidate.ordinal, index as u64);
    }
    // Served from memory while nothing has changed.
    assert!(Arc::ptr_eq(&first, &store.candidates(root)));

    // The watcher invalidates it, and the next enumeration sees the new file.
    write(root, "src/added.rs", "x");
    store.invalidate();
    let second = store.candidates(root);
    assert!(second.iter().any(|c| c.path == "src/added.rs"));

    // A different root is never served the previous root's list.
    let other = tempfile::TempDir::new().unwrap();
    write(other.path(), "only-here.md", "x");
    let switched = store.candidates(&crate::fs::RootFs::for_root(other.path()));
    assert_eq!(switched.len(), 1);
    assert_eq!(switched[0].path, "only-here.md");

    // And the project close tears it down (ASC-FR-14).
    store.clear();
    assert!(!store.is_mounted());
}
