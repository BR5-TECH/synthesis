//! A search modifies nothing (SCC-FR-16).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-16 — read-only
// -----------------------------------------------------------------------

#[test]
fn ts18_a_search_modifies_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "a.md", "needle\n");
    write(root, "b.md", "nothing\n");

    let before = tree_snapshot(root);
    for _ in 0..3 {
        search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
        search(root, "n.*e", SearchMode::Regex, SearchScope::Capped);
    }
    assert_eq!(before, tree_snapshot(root), "a search writes nothing");
}

/// Every file under `root` with its bytes — enough to catch a created,
/// deleted or rewritten file.
fn tree_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in ignore::WalkBuilder::new(root).hidden(false).build().flatten() {
        if entry.file_type().is_some_and(|t| t.is_file()) {
            let rel = entry.path().strip_prefix(root).unwrap().to_string_lossy().to_string();
            out.insert(rel, std::fs::read(entry.path()).unwrap_or_default());
        }
    }
    out
}
