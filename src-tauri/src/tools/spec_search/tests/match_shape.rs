//! SPS-FR-10, SPS-FR-11, SPS-FR-12: the match's fields, its path, and its excerpt.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-10 — the match's fields (SPS-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn a_match_carries_exactly_path_excerpt_and_score() {
    let fixture = spec_project();
    let output = query(&fixture, "teardown", Some(1));
    let value = serde_json::to_value(&output.specifications[0]).unwrap();
    let object = value.as_object().expect("an object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["excerpt", "path", "score"]);

    for absent in ["index", "nodeId", "node_id", "chunkOrdinal", "chunk_ordinal", "draftId"] {
        assert!(
            !object.contains_key(absent),
            "`{absent}` is the index's business, not the model's (SPS-FR-10)",
        );
    }
}

// ---------------------------------------------------------------------------
// SPS-FR-11 — the path is read_file's argument unchanged (SPS-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn a_matchs_path_is_accepted_by_the_read_tool_unchanged() {
    let fixture = spec_project();
    let output = query(&fixture, "teardown", Some(1));
    let path = &output.specifications[0].path;

    let root = fixture
        .app
        .state::<Bm25Indexer>()
        .root()
        .expect("mounted");
    let session = crate::tools::tests::agent_session(&fixture.app, &root, "sps-reader");
    let access = fixture
        .app
        .state::<crate::fs::FsAccessState>()
        .agent_session(&session)
        .expect("just opened");
    let text = crate::tools::file_read::read(
        &access,
        &root,
        &crate::tools::file_read::ReadFileArgs {
            path: path.clone(),
            offset: None,
            limit: None,
        },
    )
    .expect("the path a match reported is one read_file accepts (SPS-FR-11)");

    assert!(text.contains("teardown"), "the file behind the match");
}

// ---------------------------------------------------------------------------
// SPS-FR-12 — the excerpt is the passage as supplied (SPS-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn the_excerpt_is_a_section_of_the_file_verbatim() {
    let fixture = spec_project();
    let output = query(&fixture, "teardown", Some(1));
    let matched = &output.specifications[0];

    let root = fixture.app.state::<Bm25Indexer>().root().expect("mounted");
    let on_disk = std::fs::read_to_string(root.join(&matched.path)).unwrap();

    // A contiguous span of the file, located exactly.
    let start = on_disk
        .find(matched.excerpt.as_str())
        .expect("the excerpt is a span of the file itself, not something composed (SPS-FR-12)");

    // And a *complete* section, which is what rules out truncation. `contains`
    // alone would accept any prefix, so a silently shortened excerpt would pass
    // — and shortening it makes the "smaller than the file" check more true, not
    // less. The boundaries are the claim: a section runs from its own heading to
    // the next heading of any level (BMI-FR-05).
    assert!(
        matched.excerpt.trim_start().starts_with('#'),
        "an excerpt begins at its heading (SPS-FR-12): {:?}",
        matched.excerpt,
    );
    let rest = &on_disk[start + matched.excerpt.len()..];
    assert!(
        rest.is_empty() || rest.trim_start().starts_with('#'),
        "an excerpt runs to the next heading rather than to an arbitrary cut \
         (SPS-FR-12); it was followed by {:?}",
        &rest[..rest.len().min(60)],
    );
    assert!(
        matched.excerpt.len() < on_disk.len(),
        "an excerpt is one section rather than the whole file (SPS-FR-12)",
    );
}

