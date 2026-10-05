//! SPS-FR-16, SPS-FR-17, SPS-FR-18: passes, freshness, and read-only posture.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-16 — never blocks on an index pass (SPS-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn a_call_resolves_without_waiting_on_an_index_pass() {
    let fixture = spec_project();
    let tool = SpecSearchTool::new(fixture.handle());

    // `block_on` panics on `Pending`, so reaching the assertion is the proof.
    let output = block_on(tool.call(SpecificationSearchArgs {
        query: "teardown".to_string(),
        limit: None,
    }))
    .expect("resolves on the first poll (SPS-FR-16)");
    assert!(!output.specifications.is_empty());
}

// ---------------------------------------------------------------------------
// SPS-FR-17 — the next pass is followed in every direction (SPS-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn an_edit_a_deletion_and_a_new_specification_all_reach_the_next_call() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    write(&root, "specifications/a.md", &spec_body("teardown", 2));
    write(&root, "specifications/b.md", &spec_body("teardown", 2));
    let fixture = mounted(dir);
    let indexed_root = fixture.app.state::<Bm25Indexer>().root().expect("mounted");

    assert_eq!(query(&fixture, "teardown", Some(20)).specifications.len(), 2);

    // Edited to drop the term.
    write(&indexed_root, "specifications/a.md", &spec_body("palette", 2));
    fixture.reindex();
    let paths: Vec<String> = query(&fixture, "teardown", Some(20))
        .specifications
        .iter()
        .map(|m| m.path.clone())
        .collect();
    assert_eq!(paths, vec!["specifications/b.md"], "the edit is followed");

    // Deleted.
    std::fs::remove_file(indexed_root.join("specifications/b.md")).unwrap();
    fixture.reindex();
    assert!(
        query(&fixture, "teardown", Some(20)).specifications.is_empty(),
        "a deleted specification stops being returned (SPS-FR-17)",
    );

    // A file that becomes a specification.
    write(&indexed_root, "specifications/c.md", &spec_body("teardown", 2));
    fixture.reindex();
    let paths: Vec<String> = query(&fixture, "teardown", Some(20))
        .specifications
        .iter()
        .map(|m| m.path.clone())
        .collect();
    assert_eq!(paths, vec!["specifications/c.md"], "a new one is findable");
}

// ---------------------------------------------------------------------------
// SPS-FR-18 — read-only (SPS-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn searching_changes_nothing_on_disk() {
    let fixture = spec_project();
    let root = fixture.app.state::<Bm25Indexer>().root().expect("mounted");
    let before = crate::tools::tests::tree_snapshot(&root);

    for text in ["teardown", "palette", "indexing", "nothing at all"] {
        for limit in [None, Some(1), Some(20)] {
            let _ = search(
                &fixture.app.state::<Bm25Indexer>(),
                &SpecificationSearchArgs {
                    query: text.to_string(),
                    limit,
                },
            );
        }
    }

    assert_eq!(
        before,
        crate::tools::tests::tree_snapshot(&root),
        "searching leaves the project exactly as it was (SPS-FR-18)",
    );
}

