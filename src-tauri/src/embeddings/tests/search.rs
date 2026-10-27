//! Hybrid search end to end: the real ranker, the store, and `sqlite-vec`.

use super::*;
use crate::bm25_index::IndexId;

// EMB-FR-BWTI, EMB-FR-NJYA, EMB-FR-LPKD: when vector search is ready, the
// search fuses the BM25 and vector ranks — the scores are RRF scores — and a
// chunk only the vector list holds is returned with its BM25 fields.
#[test]
fn a_ready_index_fuses_bm25_and_vector_ranks() {
    let h = Harness::new();
    h.ready();
    let indexer = h.app.state::<Bm25Indexer>();
    let fused = search_open_project(h.handle(), &indexer, IndexId::Spec, "worktree", 3);
    let plain = crate::bm25_index::search(&indexer, &[IndexId::Spec], "worktree", 3);
    assert_ne!(fused, plain);
    assert_eq!(fused.len(), 3, "the vector list adds chunks BM25 did not find");
    let best = 1.0 / 61.0 + 1.0 / 61.0;
    assert!((fused[0].score as f64 - best).abs() < 1e-4, "{}", fused[0].score);
    assert_eq!(fused[0].path, "specifications/a.md");
    assert!(fused.iter().all(|hit| hit.node_id.as_deref() == Some(hit.path.as_str())));
}

// EMB-FR-BWTI, EMB-FR-HWTF: with the store gone, search returns the BM25
// result and raises no error.
#[test]
fn a_store_that_fails_gives_the_bm25_result() {
    let h = Harness::new();
    h.ready();
    std::fs::remove_file(h.store_file()).unwrap();
    std::fs::create_dir(h.store_file()).unwrap();
    let indexer = h.app.state::<Bm25Indexer>();
    assert_eq!(
        search_open_project(h.handle(), &indexer, IndexId::Spec, "worktree", 5),
        crate::bm25_index::search(&indexer, &[IndexId::Spec], "worktree", 5)
    );
}

// EMB-FR-JLRR: a query waits for the running batch and for no other: it is
// embedded before the job's next batch.
#[test]
fn a_query_is_embedded_between_two_batches() {
    let files: Vec<(String, String)> = (0..20)
        .map(|n| (format!("specifications/s{n:02}.md"), format!("# Topic {n}\n\nunique words {n}")))
        .collect();
    let refs: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let h = Harness::with_files(&refs);
    h.ready();
    h.service().request_reindex(ReindexScope::WholeTrace).unwrap();
    let app = h.handle().clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    h.during_next_embed(move || {
        // Inside the first batch: a query starts on another thread and must
        // wait until this batch ends.
        let querying = app.clone();
        std::thread::spawn(move || {
            let service = querying.state::<EmbeddingsService>();
            let _ = service.embed_query("a query");
            let _ = done_tx.send(());
        });
        while *app.state::<EmbeddingsService>().queries.lock().unwrap() == 0 {
            std::thread::yield_now();
        }
    });
    h.settle(Instant::now());
    done_rx.recv_timeout(Duration::from_secs(30)).expect("the query was answered");
    let batches = h.log.batches.lock().unwrap().clone();
    let reindex: Vec<usize> = batches[batches.len() - 3..].to_vec();
    assert_eq!(reindex, [16, 1, 4], "batch, then the query, then the next batch: {batches:?}");
}
