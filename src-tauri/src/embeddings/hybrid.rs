//! Hybrid retrieval: BM25 and vector ranks fused by Reciprocal Rank Fusion,
//! with the BM25-only fallback (EMB-FR-BWTI, EMB-FR-NJYA, EMB-FR-LPKD,
//! EMB-FR-IFWD, EMB-FR-HWTF, EMB-FR-CZND, EMB-FR-YLBN).

use crate::bm25_index::{self, Bm25Indexer, ChunkHit, ChunkKey, IndexId, IndexSet};

/// The vector half of a hybrid search.
pub trait VectorRanker {
    /// The chunk IDs of `index` nearest to `query` with their stored content
    /// hashes, nearest first. `Ok(None)` while vector search is not ready.
    fn nearest(
        &self,
        index: IndexId,
        query: &str,
        n: usize,
    ) -> Result<Option<Vec<(String, String)>>, String>;

    /// Report a failure of the vector half (EMB-FR-HWTF).
    fn report_failure(&self, reason: &str);
}

/// EMB-FR-BWTI / EMB-FR-NJYA: the `limit` best chunks of `index` for `query`.
///
/// Without a ranker, or while vector search is not ready, this is exactly
/// `bm25_index::search([index], query, limit)`. It never returns an error: a
/// failure in the vector part gives the BM25 result (EMB-FR-HWTF).
pub fn hybrid_search(
    indexer: &Bm25Indexer,
    ranker: Option<&dyn VectorRanker>,
    index: IndexId,
    query: &str,
    limit: usize,
) -> Vec<ChunkHit> {
    let snapshot = indexer.snapshot();
    let bm25 = bm25_index::search_snapshot(&snapshot, &[index], query, limit);
    if limit == 0 || query.trim().is_empty() || !matches!(index, IndexId::Spec | IndexId::Documents)
    {
        return bm25;
    }
    let Some(ranker) = ranker else {
        return bm25;
    };
    let vector = match ranker.nearest(index, query, limit) {
        Ok(Some(vector)) => vector,
        Ok(None) => return bm25,
        Err(reason) => {
            ranker.report_failure(&reason);
            return bm25;
        }
    };
    fuse_hits(&snapshot, index, bm25, &vector, limit)
}

/// EMB-FR-NJYA, EMB-FR-LPKD, EMB-FR-CZND: fuse a BM25 list with a vector list
/// of (chunk ID, stored hash). A vector hit counts only when its stored hash is
/// the hash of the chunk text the BM25 index holds now under the same ID, so
/// the vector of old content never ranks a chunk.
pub fn fuse_hits(
    snapshot: &IndexSet,
    index: IndexId,
    bm25: Vec<ChunkHit>,
    vector: &[(String, String)],
    limit: usize,
) -> Vec<ChunkHit> {
    let mut hits: std::collections::HashMap<String, ChunkHit> = std::collections::HashMap::new();
    let bm25_ids: Vec<String> = bm25
        .into_iter()
        .map(|hit| {
            let id = bm25_index::chunk_id(&hit.path, hit.chunk_ordinal);
            hits.insert(id.clone(), hit);
            id
        })
        .collect();
    let mut vector_ids: Vec<String> = Vec::new();
    for (chunk_id, hash) in vector {
        let Some((path, ordinal)) = bm25_index::parse_chunk_id(chunk_id) else {
            continue;
        };
        let key = ChunkKey { draft_id: None, path: path.to_string(), ordinal };
        let current = match hits.get(chunk_id) {
            Some(hit) => Some(hit.text.clone()),
            None => snapshot.hit(index, &key, 0.0).map(|hit| {
                let text = hit.text.clone();
                hits.insert(chunk_id.clone(), hit);
                text
            }),
        };
        if current.is_some_and(|text| &super::layers::hash_text(&text) == hash) {
            vector_ids.push(chunk_id.clone());
        }
    }
    super::rrf::fuse(&bm25_ids, &vector_ids)
        .into_iter()
        .take(limit)
        .filter_map(|fused| {
            hits.remove(&fused.chunk_id).map(|mut hit| {
                hit.score = fused.score as f32;
                hit
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25_index::{reconcile, FileRef, PassScope, SourceFile};
    use std::cell::RefCell;
    use tauri::Manager;

    fn snapshot(files: &[(&str, &str)]) -> IndexSet {
        let sources = files
            .iter()
            .map(|(path, text)| SourceFile {
                index: IndexId::Spec,
                file: FileRef { draft_id: None, path: path.to_string() },
                text: Ok(text.to_string()),
                plain_text: false,
            })
            .collect();
        reconcile(&IndexSet::default(), sources, PassScope::ARTIFACTS, |_, _, _| {}, |_| {}).0
    }

    fn hash(text: &str) -> String {
        super::super::layers::hash_text(text)
    }

    struct Ranker {
        answer: Result<Option<Vec<(String, String)>>, String>,
        failures: RefCell<Vec<String>>,
    }

    impl VectorRanker for Ranker {
        fn nearest(
            &self,
            _: IndexId,
            _: &str,
            _: usize,
        ) -> Result<Option<Vec<(String, String)>>, String> {
            self.answer.clone()
        }
        fn report_failure(&self, reason: &str) {
            self.failures.borrow_mut().push(reason.to_string());
        }
    }

    // EMB-FR-NJYA, EMB-FR-LPKD: a chunk only the vector list holds is returned
    // with its BM25 text and fields, and the score is the fused score.
    #[test]
    fn a_vector_only_chunk_is_returned_with_its_bm25_fields() {
        let set = snapshot(&[
            ("specifications/a.md", "# Alpha\n\nworktree teardown"),
            ("specifications/b.md", "# Beta\n\nunrelated words"),
        ]);
        let bm25 = bm25_index::search_snapshot(&set, &[IndexId::Spec], "teardown", 10);
        assert_eq!(bm25.len(), 1);
        let b_text = set
            .chunks(IndexId::Spec)
            .into_iter()
            .find(|(k, _)| k.path.ends_with("b.md"))
            .unwrap()
            .1;
        let vector = vec![("specifications/b.md#0".to_string(), hash(&b_text))];
        let fused = fuse_hits(&set, IndexId::Spec, bm25, &vector, 10);
        assert_eq!(fused.len(), 2);
        let b = fused.iter().find(|h| h.path == "specifications/b.md").unwrap();
        assert_eq!(b.node_id.as_deref(), Some("specifications/b.md"));
        assert_eq!(b.text, b_text);
        assert!((b.score - 1.0 / 61.0).abs() < 1e-6);
    }

    // EMB-FR-CZND, EMB-FR-YLBN: a stored vector of old content is not used, so
    // the chunk keeps its BM25 rank only.
    #[test]
    fn a_vector_of_old_content_is_ignored() {
        let set = snapshot(&[("specifications/a.md", "# Alpha\n\nnew text")]);
        let vector = vec![("specifications/a.md#0".to_string(), hash("# Alpha\n\nold text"))];
        let fused = fuse_hits(&set, IndexId::Spec, Vec::new(), &vector, 10);
        assert!(fused.is_empty());
        // A vector of a chunk the BM25 index no longer holds is dropped too.
        let gone = vec![("specifications/zzz.md#0".to_string(), hash("x"))];
        assert!(fuse_hits(&set, IndexId::Spec, Vec::new(), &gone, 10).is_empty());
    }

    // EMB-FR-BWTI: the drafts index, an empty query, and a zero limit never
    // reach the vector part.
    #[test]
    fn indexes_without_vectors_never_ask_the_ranker() {
        struct Panics;
        impl VectorRanker for Panics {
            fn nearest(
                &self,
                _: IndexId,
                _: &str,
                _: usize,
            ) -> Result<Option<Vec<(String, String)>>, String> {
                panic!("the vector part was asked")
            }
            fn report_failure(&self, _: &str) {}
        }
        let indexer = Bm25Indexer::default();
        assert!(hybrid_search(&indexer, Some(&Panics), IndexId::Drafts, "x", 5).is_empty());
        assert!(hybrid_search(&indexer, Some(&Panics), IndexId::Spec, "  ", 5).is_empty());
        assert!(hybrid_search(&indexer, Some(&Panics), IndexId::Spec, "x", 0).is_empty());
    }

    // EMB-FR-LPKD, EMB-FR-CZND: the fused list stops at `limit`, and a chunk
    // both lists hold whose stored hash is stale keeps only its BM25 share.
    #[test]
    fn the_limit_bounds_the_result_and_a_stale_hash_adds_nothing() {
        let set = snapshot(&[
            ("specifications/a.md", "# A\n\nteardown teardown"),
            ("specifications/b.md", "# B\n\nteardown"),
        ]);
        let bm25 = bm25_index::search_snapshot(&set, &[IndexId::Spec], "teardown", 10);
        assert_eq!(fuse_hits(&set, IndexId::Spec, bm25.clone(), &[], 1).len(), 1);
        let stale = vec![("specifications/a.md#0".to_string(), hash("old text"))];
        let fused = fuse_hits(&set, IndexId::Spec, bm25, &stale, 10);
        assert!((fused[0].score - 1.0 / 61.0).abs() < 1e-6);
    }

    // EMB-FR-BWTI: with no ranker, or one not ready, the BM25 result comes back
    // unchanged; EMB-FR-HWTF: a failing ranker gives BM25 and is reported.
    #[test]
    fn falls_back_to_bm25_when_vectors_are_absent_or_fail() {
        let app = tauri::test::mock_app();
        let indexer = Bm25Indexer::default();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("specifications")).unwrap();
        std::fs::write(
            root.path().join("specifications/a.md"),
            "# Alpha\n\nteardown of a worktree",
        )
        .unwrap();
        let rooted = crate::fs::RootFs::for_root(root.path());
        app.manage(crate::progress::ProgressRegistry::default());
        app.manage(crate::scanning::CandidateStore::default());
        indexer.mount(&rooted);
        indexer.run_pass(app.handle(), &rooted, PassScope::ALL, indexer.generation());
        let plain = bm25_index::search(&indexer, &[IndexId::Spec], "teardown", 5);
        assert!(!plain.is_empty());

        assert_eq!(hybrid_search(&indexer, None, IndexId::Spec, "teardown", 5), plain);
        let not_ready = Ranker { answer: Ok(None), failures: RefCell::new(Vec::new()) };
        assert_eq!(hybrid_search(&indexer, Some(&not_ready), IndexId::Spec, "teardown", 5), plain);
        let failing =
            Ranker { answer: Err("store busy".to_string()), failures: RefCell::new(Vec::new()) };
        assert_eq!(hybrid_search(&indexer, Some(&failing), IndexId::Spec, "teardown", 5), plain);
        assert_eq!(failing.failures.borrow().as_slice(), ["store busy"]);
    }
}
