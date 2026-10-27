//! Failures: the model download, the store, a failed pass, and what the log
//! may and may not carry.

use super::*;

// EMB-FR-LYKW, EMB-FR-KEGS: a failed download leaves embeddings not ready for
// the open project and is not tried again until the user asks.
#[test]
fn a_failed_download_waits_for_a_retry() {
    let (factory, log) = FakeFactory::new();
    *factory.fail_download.lock().unwrap() = true;
    let h = Harness::with_factory(&[("specifications/a.md", SPEC_A)], factory, log);
    h.enable();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    let status = h.service().status();
    assert_eq!(status.model_state, ModelState::Failed);
    assert_eq!(status.model_error.as_deref(), Some("network unreachable"));
    assert!(!h.ready_for_spec());
    h.settle(Instant::now() + LATER);
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 1, "no automatic retry");

    h.service().retry_download();
    h.settle(Instant::now());
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 2);
}

// EMB-FR-RFBJ: with no repository store, the index is unavailable and search
// stays BM25-only.
#[test]
fn a_missing_store_leaves_search_bm25_only() {
    let h = Harness::new();
    h.enable();
    h.mount_with_store(ProjectEmbeddingsMode::UseGlobal, None);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.phase(), Some(Phase::Unavailable));
    assert!(!h.ready_for_spec());
}

// EMB-FR-RFBJ: a store file that is not a database is unavailable; once it
// is readable again, the next trigger brings the index back.
#[test]
fn a_corrupt_store_is_unavailable_until_it_can_be_opened() {
    let h = Harness::new();
    std::fs::write(h.store_file(), b"this is not a database, it is plain bytes").unwrap();
    h.enable();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.phase(), Some(Phase::Unavailable));
    assert!(h.service().status().project.unwrap().last_error.is_some());

    std::fs::remove_file(h.store_file()).unwrap();
    h.write("specifications/c.md", "# Third\n\nA trigger.");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-RFBJ, RMS-FR-QCVB: a store that another process holds busy is
// unavailable for that attempt; the next trigger after it is free works.
#[test]
fn a_busy_store_is_unavailable_for_that_attempt() {
    let h = Harness::new();
    h.ready();
    let other = rusqlite::Connection::open(h.store_file()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE;").unwrap();

    h.write("specifications/c.md", "# Third\n\nWritten while the store is busy.");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.phase(), Some(Phase::Unavailable));
    assert!(!h.ready_for_spec());

    other.execute_batch("ROLLBACK;").unwrap();
    drop(other);
    h.write("specifications/d.md", "# Fourth\n\nWritten once it is free.");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.phase(), Some(Phase::Ready));
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 5);
}

// EMB-FR-EAEA: a Reindex whose embedding fails leaves the store as it was —
// the layers it would drop are still there, and search keeps using them.
#[test]
fn a_failed_reindex_leaves_the_store_as_it_was() {
    let h = Harness::new();
    h.ready();
    let layers_before = store::Store::open_existing(&h.store_file()).unwrap().layers().unwrap();
    h.service().request_reindex(ReindexScope::WholeTrace).unwrap();
    h.log.fail_embed.store(true, Ordering::SeqCst);
    h.settle(Instant::now());
    let layers_after = store::Store::open_existing(&h.store_file()).unwrap().layers().unwrap();
    assert_eq!(layers_after, layers_before);
    assert_eq!(h.phase(), Some(Phase::Ready));
    assert!(h.ready_for_spec());
    assert_eq!(h.service().status().project.unwrap().last_error.as_deref(), Some("embed failed"));
}

// EMB-FR-OLDK: a specification of the root tip that gives no chunks is named
// in a WARN record by its project-relative path, with the reason.
#[test]
fn a_skipped_root_file_is_logged() {
    let sentinel = format!("skipped-{}", std::process::id());
    let h = Harness::new();
    let path = format!("specifications/{sentinel}.md");
    std::fs::write(h.repo_dir.path().join(&path), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let mut index = h.repo.index().unwrap();
    index.add_path(std::path::Path::new(&path)).unwrap();
    index.write().unwrap();
    let tree = h.repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let parent = h.repo.head().unwrap().peel_to_commit().unwrap();
    h.repo.commit(Some("HEAD"), &sig, &sig, "binary", &tree, &[&parent]).unwrap();
    h.ready();
    let log = rendered_log();
    assert!(log.contains(&sentinel), "a WARN names the skipped file");
    assert!(log.contains("embeddings skipped a file"));
}

// EMB-FR-EYGO, EMB-FR-HWTF: no record carries a chunk text or a query, also
// when the vector part fails and search falls back to BM25.
#[test]
fn no_record_carries_chunk_text_or_a_query() {
    let text_sentinel = format!("chunksentinel{}", std::process::id());
    let query_sentinel = format!("querysentinel{}", std::process::id());
    let h = Harness::with_files(&[(
        "specifications/a.md",
        &format!("# Secret\n\nThe {text_sentinel} passage."),
    )]);
    h.ready();
    std::fs::remove_file(h.store_file()).unwrap();
    std::fs::create_dir(h.store_file()).unwrap();
    let indexer = h.app.state::<Bm25Indexer>();
    let index = crate::bm25_index::IndexId::Spec;
    let query = format!("{text_sentinel} {query_sentinel}");
    assert_eq!(
        search_open_project(h.handle(), &indexer, index, &query, 5),
        crate::bm25_index::search(&indexer, &[index], &query, 5)
    );
    let log = rendered_log();
    assert!(log.contains("vector search failed"), "the fallback is logged");
    assert!(!log.contains(&text_sentinel));
    assert!(!log.contains(&query_sentinel));
}

// EMB-FR-JFOV, EMB-FR-ZFGT: turning embeddings off while the model downloads
// leaves the status at not downloaded, not at a download that was abandoned,
// and loads no model.
#[test]
fn turning_off_during_a_download_leaves_no_stale_model_state() {
    let h = Harness::new();
    let app = h.handle().clone();
    *h.log.on_download.lock().unwrap() = Some(Box::new(move || {
        app.state::<EmbeddingsService>()
            .apply_settings(EmbeddingsRecord { enabled: false, model: None });
    }));
    h.enable();
    h.settle(Instant::now());
    let status = h.service().status();
    assert!(!status.enabled);
    assert_eq!(status.model_state, ModelState::NotDownloaded);
    assert!(h.service().embedder.lock().unwrap().is_none());
}

// EMB-FR-EAEA: a failed incremental pass keeps the last good view and writes
// an ERROR record. The next trigger, with no new change, tries the same pass
// again.
#[test]
fn a_failed_incremental_pass_is_logged_and_tried_again() {
    let sentinel = format!("failedpass{}", std::process::id());
    let reason = format!("embed failed {sentinel}reason");
    let h = Harness::new();
    h.ready();
    h.log.fail_embed.store(true, Ordering::SeqCst);
    *h.log.fail_reason.lock().unwrap() = Some(reason.clone());
    h.write("specifications/c.md", &format!("# Third\n\nA new {sentinel} specification."));
    h.pass();
    h.settle(Instant::now() + LATER);
    let project = h.service().status().project.unwrap();
    assert_eq!(project.last_error.as_deref(), Some(reason.as_str()));
    assert_eq!(project.state, Phase::Ready);
    assert!(h.ready_for_spec());
    assert_eq!(project.spec_chunks, 3);
    // The one ERROR record of this pass, found by its unique reason.
    let record = failed_record(&reason).expect("an ERROR record of the failed pass");
    assert_eq!(record["message"], "embedding job failed");
    assert_eq!(record["fields"]["job"], "incremental");
    assert!(!rendered_log().contains(&format!("A new {sentinel}")), "no chunk text in a record");

    h.log.fail_embed.store(false, Ordering::SeqCst);
    let generation = h.app.state::<Bm25Indexer>().generation();
    h.service().index_published(true, generation, Instant::now());
    h.settle(Instant::now() + LATER);
    let project = h.service().status().project.unwrap();
    assert_eq!(project.spec_chunks, 4);
    assert_eq!(project.last_error, None);
}

/// The ERROR record whose `reason` field is `reason`, rendered.
fn failed_record(reason: &str) -> Option<serde_json::Value> {
    use crate::logging::{LogFilter, LogLevel, BUFFER};
    let records = BUFFER
        .query(&LogFilter { min_level: LogLevel::Error, ..LogFilter::default() }, None, 100_000)
        .unwrap()
        .records;
    let records = serde_json::to_value(records).unwrap();
    records.as_array()?.iter().find(|r| r["fields"]["reason"] == reason).cloned()
}
