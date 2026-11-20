//! The mounted project index (ASC-FR-HPUY, ASC-FR-TPJO, ASC-FR-17).
//!
//! One part of `../tests/mod.rs`.

use super::*;
use std::sync::Arc;

// ASC-FR-TPJO: reads that arrive together share one build.
#[test]
fn concurrent_reads_share_one_build() {
    let tmp = project(&[("a.md", ""), ("b/c.md", "")]);
    let store = Arc::new(CandidateStore::default());
    let root = crate::fs::RootFs::for_root(tmp.path());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let root = root.clone();
            std::thread::spawn(move || store.index(&root))
        })
        .collect();
    let all: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(all.windows(2).all(|w| Arc::ptr_eq(&w[0], &w[1])));
}

// ASC-FR-17: a reader holds the index it started from while an update
// changes the mounted one.
#[test]
fn a_held_index_outlives_an_update() {
    let tmp = project(&[("a.md", "")]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    let store = CandidateStore::default();
    let held = store.index(&root);
    fs::write(tmp.path().join("b.md"), "").unwrap();
    let changed = store.update(&root, |i| i.apply(&root, &["b.md".to_string()], false, &[]));
    assert_eq!(changed, Some(vec![String::new()]));
    assert_eq!(candidate_paths(&held), vec!["a.md"]);
    assert_eq!(candidate_paths(&store.mounted(&root).unwrap()), vec!["a.md", "b.md"]);
}

// ASC-FR-HPUY: an update with nothing mounted, or for another root, does
// nothing and builds nothing.
#[test]
fn an_update_needs_a_mounted_index_of_that_root() {
    let tmp = project(&[("a.md", "")]);
    let other = project(&[("z.md", "")]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    let store = CandidateStore::default();
    assert!(store.update(&root, |_| ()).is_none());
    assert!(!store.is_mounted());
    let _ = store.index(&root);
    let other_root = crate::fs::RootFs::for_root(other.path());
    assert!(store.update(&other_root, |_| ()).is_none());
    assert!(store.mounted(&root).is_some());
}

// ASC-FR-HPUY: the full tree of the index is the tree `scan` returns, and the
// candidates are in tree order with ordinals from 0.
#[test]
fn the_index_and_scan_agree() {
    let tmp = project(&[("b/x.md", ""), ("a.md", ""), ("b/a/y.md", "")]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    let index = ProjectIndex::build(&root);
    let name = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(index.full_tree(&name), scan(&root));
    assert_eq!(candidate_paths(&index), vec!["b/a/y.md", "b/x.md", "a.md"]);
    let ordinals: Vec<u64> = index.candidates().iter().map(|c| c.ordinal).collect();
    assert_eq!(ordinals, vec![0, 1, 2]);
}

// ASC-FR-09, ASC-FR-23: a one-node read refuses what the tree does not admit,
// and reports a folder with its roll-up.
#[test]
fn node_at_admits_what_the_tree_admits() {
    let tmp = project(&[
        (".gitignore", "dist/\n"),
        ("dist/x", ""),
        ("docs/a.spec.md", ""),
        (".synthesis/cache/db", ""),
    ]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    assert!(node_at(&root, "dist").is_none());
    assert!(node_at(&root, ".synthesis/cache/db").is_none());
    assert!(node_at(&root, "missing").is_none());
    assert!(node_at(&root, LIBRARY_TOML_REL).is_none());
    let docs = node_at(&root, "docs").unwrap();
    assert_eq!(docs.subtree_types, Some(vec![ArtifactType::Spec]));
    assert_eq!(docs.has_children, Some(true));
}

// ASC-FR-TPJO: a read that arrives while an update runs waits for it and
// answers from the changed index.
#[test]
fn a_read_during_an_update_waits_and_reads_the_new_index() {
    let tmp = project(&[("a.md", "")]);
    let root = crate::fs::RootFs::for_root(tmp.path());
    let store = Arc::new(CandidateStore::default());
    let _ = store.index(&root);
    fs::write(tmp.path().join("b.md"), "").unwrap();
    let (inside_tx, inside_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let writer = {
        let store = Arc::clone(&store);
        let root = root.clone();
        std::thread::spawn(move || {
            store.update(&root, |i| {
                inside_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                i.apply(&root, &["b.md".to_string()], false, &[])
            })
        })
    };
    inside_rx.recv().unwrap();
    let reader = {
        let store = Arc::clone(&store);
        let root = root.clone();
        std::thread::spawn(move || store.index(&root))
    };
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    let read = reader.join().unwrap();
    assert_eq!(candidate_paths(&read), vec!["a.md", "b.md"]);
    assert!(Arc::ptr_eq(&read, &store.mounted(&root).unwrap()));
}
