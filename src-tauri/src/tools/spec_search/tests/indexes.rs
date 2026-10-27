//! SPS-FR-05: the spec index alone.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-05 — the spec index alone (SPS-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn no_other_index_is_reachable_through_this_tool() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "specifications/core/a.md", &spec_body("sarcophagus", 1));
    // A skill, a flow, and a prompt each carrying the same term. Every one of
    // them lands in an index this tool must not reach.
    crate::tools::tests::write_skill(
        root,
        ".claude/skills",
        "digger",
        "name: digger\ndescription: a skill about sarcophagus handling",
        "sarcophagus sarcophagus sarcophagus",
    );
    write(root, "flows/x.flow", "{\"note\":\"sarcophagus\"}");
    write(root, "resources/prompts/p.md", "# sarcophagus\n\nsarcophagus\n");
    // SPS-FR-05 names the `notes` index among the ten this tool cannot reach.
    crate::notes::create_note_in(
        &crate::fs::RootFs::for_root(root),
        crate::notes::NoteScope::Project,
        "sarcophagus in a note".to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes");
    let fixture = mounted(dir);

    let output = query(&fixture, "sarcophagus", Some(20));
    let paths: Vec<&str> = output.specifications.iter().map(|m| m.path.as_str()).collect();

    assert_eq!(paths, vec!["specifications/core/a.md"], "only the spec index answers (SPS-FR-05)",);
}

/// A ranker that answers with a fixed vector list, as a ready index would.
struct FixedRanker(Vec<(String, String)>);

impl crate::embeddings::VectorRanker for FixedRanker {
    fn nearest(
        &self,
        _: crate::bm25_index::IndexId,
        _: &str,
        _: usize,
    ) -> Result<Option<Vec<(String, String)>>, String> {
        Ok(Some(self.0.clone()))
    }
    fn report_failure(&self, _: &str) {}
}

// SPS-FR-05, SPS-FR-07, SPS-FR-10: with vectors ready, ranking is the fusion of
// BM25 and vector ranks; a file the vector list ranks first is returned with its
// passage, one match per file, in the unchanged output shape.
#[test]
fn ranking_fuses_the_vector_ranks_when_they_are_ready() {
    let fixture = spec_project();
    let indexer = fixture.app.state::<Bm25Indexer>();
    let args = SpecificationSearchArgs { query: "teardown".to_string(), limit: Some(3) };
    let plain = search(&indexer, &args).unwrap();
    assert_eq!(plain.specifications[0].path, "specifications/core/teardown.md");

    // The vector list puts the lightest file first, with every chunk of it.
    let chunks = indexer.snapshot().chunks(crate::bm25_index::IndexId::Spec);
    let vector: Vec<(String, String)> = chunks
        .iter()
        .filter(|(key, _)| key.path == "specifications/core/indexing.md")
        .map(|(key, text)| (key.chunk_id(), crate::fs::sha256_bytes(text.as_bytes())))
        .collect();
    let fused = search_ranked(&indexer, Some(&FixedRanker(vector)), &args).unwrap();
    let paths: Vec<&str> = fused.specifications.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths.len(), 3);
    let plain_paths: Vec<&str> = plain.specifications.iter().map(|m| m.path.as_str()).collect();
    assert_ne!(paths, plain_paths, "the vector ranks moved the order");
    assert_eq!(paths[0], "specifications/core/indexing.md");
    let unique: std::collections::HashSet<&str> = paths.iter().copied().collect();
    assert_eq!(unique.len(), paths.len(), "one match per specification");
    // A fused score is a sum of reciprocal ranks, far below a BM25 score.
    assert!(fused.specifications.iter().all(|m| m.score < 0.05 && !m.excerpt.is_empty()));
}
