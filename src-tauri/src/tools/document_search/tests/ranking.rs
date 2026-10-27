//! SDT-FR-BKAW: the ranking is the fusion of BM25 and vector ranks when the
//! vectors are ready, and BM25 alone when they are not.

use super::*;

struct FixedRanker(Option<Vec<(String, String)>>);

impl crate::embeddings::VectorRanker for FixedRanker {
    fn nearest(
        &self,
        _: crate::bm25_index::IndexId,
        _: &str,
        _: usize,
    ) -> Result<Option<Vec<(String, String)>>, String> {
        Ok(self.0.clone())
    }
    fn report_failure(&self, _: &str) {}
}

fn args(query: &str) -> SearchDocumentsArgs {
    SearchDocumentsArgs { query: query.to_string(), limit: Some(5) }
}

// SDT-FR-BKAW, SDT-FR-EMCC: a document the vector list ranks first moves up,
// one match per document; not ready gives the BM25 result; a vector of old
// content is dropped.
#[test]
fn ranking_fuses_vector_ranks_when_they_are_ready() {
    let fixture = DocFixture::new();
    fixture.write("refs/strong.md", "# Lemurs\n\nlemur lemur lemur lives on Madagascar");
    fixture.write("refs/weak.md", "# Island\n\none lemur, and a long text about islands");
    fixture.select(SourceKind::Folder, "refs");
    let app = &fixture.fixture.app;
    let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
    let collection = app.state::<crate::documents::DocumentsCollection>();

    let plain = search_ranked(&indexer, None, &collection, &args("lemur")).unwrap();
    assert_eq!(plain.documents[0].name, "strong.md");
    let not_ready = FixedRanker(None);
    assert_eq!(
        search_ranked(&indexer, Some(&not_ready), &collection, &args("lemur")).unwrap(),
        plain
    );

    let weak: Vec<(String, String)> = indexer
        .snapshot()
        .chunks(crate::bm25_index::IndexId::Documents)
        .into_iter()
        .filter(|(key, _)| plain.documents.iter().any(|d| d.name == "weak.md" && d.id == key.path))
        .map(|(key, text)| (key.chunk_id(), crate::fs::sha256_bytes(text.as_bytes())))
        .collect();
    assert!(!weak.is_empty());
    let fused = search_ranked(
        &indexer,
        Some(&FixedRanker(Some(weak.clone()))),
        &collection,
        &args("lemur"),
    )
    .unwrap();
    let names: Vec<&str> = fused.documents.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["weak.md", "strong.md"]);

    let stale: Vec<(String, String)> =
        weak.into_iter().map(|(id, _)| (id, "stale".to_string())).collect();
    let dropped =
        search_ranked(&indexer, Some(&FixedRanker(Some(stale))), &collection, &args("lemur"))
            .unwrap();
    assert_eq!(dropped.documents[0].name, "strong.md");
}
