//! Reads that cannot hang or run without a bound, and the lifecycle edges
//! (DCL-FR-RXMB, DCL-FR-DLPX, DCL-FR-VWSG, DCL-FR-MSHW, DCL-FR-XHSJ,
//! DCL-FR-KMHY).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::documents::manager::Limits;
use crate::documents::model::{Availability, DocumentsError};
use crate::documents::watcher::{DocumentsWatcher, WatchTarget};

fn source(kind: SourceKind, path: &str) -> StoredSource {
    StoredSource {
        kind,
        path: path.to_string(),
    }
}

fn small_limits() -> Limits {
    Limits {
        max_read_bytes: 10,
        max_extract_bytes: 1024,
        extract_deadline: Duration::from_millis(200),
    }
}

/// An extractor that never returns.
fn extract_never_returns(_bytes: &[u8]) -> Result<String, String> {
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
fn make_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid, NUL-terminated string that lives for the call.
    let status = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    assert_eq!(status, 0, "the FIFO is created");
}

fn warns(docs: &Docs, needle: &str) -> usize {
    docs.log
        .records()
        .iter()
        .filter(|r| r.0 == LogLevel::Warn && r.1.contains(needle))
        .count()
}

// DCL-FR-RXMB, DCL-FR-KMHY: a FIFO that carries a supported name, in a selected
// folder, is skipped with a WARN. The refresh does not block on it, and the
// regular files beside it are listed.
#[cfg(unix)]
#[test]
fn a_fifo_in_a_selected_folder_is_skipped_and_never_blocks_the_refresh() {
    let docs = Docs::new();
    docs.write("refs/real.md", "real");
    make_fifo(&docs.root.join("refs/pipe.txt"));
    make_fifo(&docs.root.join("refs/pipe.pdf"));
    docs.add_folder("refs");

    let (send, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            docs.refresh();
            let _ = send.send(());
        });
        receive
            .recv_timeout(Duration::from_secs(10))
            .expect("the refresh ended");
    });
    assert_eq!(docs.names(), ["real.md"]);
    assert_eq!(warns(&docs, "not a regular file"), 2);
}

// DCL-FR-RXMB, DCL-FR-EWPO: a FIFO that is a file source is unavailable, and
// nothing reads it.
#[cfg(unix)]
#[test]
fn a_fifo_that_is_a_file_source_is_unavailable() {
    let docs = Docs::new();
    make_fifo(&docs.root.join("pipe.md"));
    docs.add_file("pipe.md");
    docs.refresh();
    let snapshot = docs.snapshot();
    assert_eq!(snapshot.sources[0].status, Availability::Unavailable);
    assert!(snapshot.documents.iter().all(|d| d.status == Availability::Unavailable));
}

// DCL-FR-RXMB: a file over the size limit is unavailable, a WARN records it, no
// content is returned, and the file at the limit stays available.
#[test]
fn a_file_over_the_size_limit_is_unavailable() {
    let docs = Docs::with_limits(extract_as_text, small_limits());
    docs.write("refs/at-limit.md", "0123456789");
    docs.write("refs/over.md", "0123456789a");
    docs.write("refs/over.pdf", "0123456789a");
    docs.add_folder("refs");
    docs.refresh();

    let snapshot = docs.snapshot();
    let status = |name: &str| {
        snapshot
            .documents
            .iter()
            .find(|d| d.name == name)
            .unwrap()
            .status
    };
    assert_eq!(status("at-limit.md"), Availability::Available);
    assert_eq!(status("over.md"), Availability::Unavailable);
    assert_eq!(status("over.pdf"), Availability::Unavailable);
    assert_eq!(warns(&docs, "cannot be read"), 2);
    let over = docs.id_of("refs/over.md");
    assert_eq!(
        docs.collection.read_document(&docs.fs, &over).unwrap_err(),
        DocumentsError::Unavailable
    );
    let at_limit = docs.id_of("refs/at-limit.md");
    assert_eq!(
        docs.collection.read_document(&docs.fs, &at_limit).unwrap().text,
        "0123456789"
    );
    let over_pdf = docs.id_of("refs/over.pdf");
    assert_eq!(
        docs.collection.read_document_pdf(&docs.fs, &over_pdf).unwrap_err(),
        DocumentsError::Unavailable
    );
}

// DCL-FR-RXMB: a file that grows past the limit after a refresh is refused at
// the read, because the read itself is bounded.
#[test]
fn a_file_that_grows_past_the_limit_is_refused_at_the_read() {
    let docs = Docs::with_limits(extract_as_text, small_limits());
    docs.write("a.md", "small");
    docs.add_file("a.md");
    docs.refresh();
    docs.write("a.md", "grown past the limit");
    let id = docs.id_of("a.md");
    assert_eq!(
        docs.collection.read_document(&docs.fs, &id).unwrap_err(),
        DocumentsError::Unavailable
    );
}

// DCL-FR-DLPX, DCL-FR-VRTS: an extractor that never returns is abandoned at the
// deadline. The caller gets `no_text` within a bounded time, a WARN names the
// document, and the next refresh is not held.
#[test]
fn an_extractor_that_never_returns_is_abandoned_at_the_deadline() {
    let docs = Docs::with_limits(extract_never_returns, small_limits());
    docs.write("stuck.pdf", "bytes");
    docs.add_file("stuck.pdf");
    docs.refresh();
    let id = docs.id_of("stuck.pdf");

    let started = Instant::now();
    let error = docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap_err();
    assert_eq!(error, DocumentsError::NoText);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the caller waited {:?}",
        started.elapsed()
    );
    assert_eq!(warns(&docs, "no extractable text"), 1);
    let records = docs.log.records();
    let warn = records.iter().find(|r| r.1.contains("no extractable text")).unwrap();
    assert_eq!(warn.2.get("document").and_then(|v| v.as_str()), Some(id.as_str()));
    assert_eq!(
        warn.2.get("reason").and_then(|v| v.as_str()),
        Some("extraction ran past its deadline")
    );

    // The outcome is cached for the revision, and the index feed is not held.
    assert_eq!(docs.collection.extraction_count(), 1);
    let started = Instant::now();
    let indexed = docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(indexed.len(), 1);
    assert!(indexed[0].text.is_err());
    assert_eq!(docs.collection.extraction_count(), 1, "the abandoned outcome is cached");
    docs.refresh();
}

// DCL-FR-DLPX: a PDF over the input limit is not given to the extractor.
#[test]
fn a_pdf_over_the_extraction_limit_is_not_extracted() {
    let limits = Limits {
        max_read_bytes: 1024,
        max_extract_bytes: 4,
        extract_deadline: Duration::from_secs(5),
    };
    let docs = Docs::with_limits(extract_as_text, limits);
    docs.write("big.pdf", "more than four bytes");
    docs.add_file("big.pdf");
    docs.refresh();
    let id = docs.id_of("big.pdf");
    assert_eq!(
        docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap_err(),
        DocumentsError::NoText
    );
    assert_eq!(docs.collection.extraction_count(), 0);
    let records = docs.log.records();
    let warn = records.iter().find(|r| r.1.contains("no extractable text")).unwrap();
    assert_eq!(
        warn.2.get("reason").and_then(|v| v.as_str()),
        Some("larger than the extraction limit")
    );
}

// DCL-FR-VWSG: a source addition that a project switch overtakes stores its list
// under the key of the project it began for, and changes the sources of no other.
#[test]
fn a_source_addition_overtaken_by_a_project_switch_changes_no_other_project() {
    let docs = Docs::new();
    let stored = std::sync::Mutex::new(Vec::<(String, usize)>::new());
    let switch = |key: &str, list: &[StoredSource]| {
        stored.lock().unwrap().push((key.to_string(), list.len()));
        // The project switches while the list is being written.
        docs.collection.open("other-project", Ok(vec![source(SourceKind::File, "/other.md")]));
        Ok(())
    };
    let result = docs
        .collection
        .add_sources(&[source(SourceKind::File, "/a.md")], &switch);
    assert_eq!(result.unwrap_err(), DocumentsError::NoProjectOpen);
    assert_eq!(*stored.lock().unwrap(), [("project-key".to_string(), 1)]);
    assert_eq!(
        docs.collection.stored_sources(),
        [source(SourceKind::File, "/other.md")],
        "the new project keeps its own sources"
    );
}

// DCL-FR-VWSG: the same holds for a removal.
#[test]
fn a_source_removal_overtaken_by_a_project_switch_changes_no_other_project() {
    let docs = Docs::new();
    docs.collection.close();
    docs.collection
        .open("project-key", Ok(vec![source(SourceKind::File, "/a.md")]));
    let switch = |_key: &str, _list: &[StoredSource]| {
        docs.collection.open("other-project", Ok(Vec::new()));
        Ok(())
    };
    let result = docs.collection.remove_source("/a.md", &switch);
    assert_eq!(result.unwrap_err(), DocumentsError::NoProjectOpen);
    assert!(docs.collection.stored_sources().is_empty());
    assert!(docs.collection.is_open_for("other-project"));
}

// DCL-FR-VWSG: a close waits for the event that is being emitted, so none is
// emitted after the close returned, and a refresh after the close emits none.
#[test]
fn no_event_is_emitted_after_the_close_returned() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.add_folder("refs");
    let emitting = AtomicBool::new(false);
    let emitted = AtomicBool::new(false);
    let (started_send, started_receive) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            docs.collection.refresh(&docs.fs, &docs.log, &|_| {
                emitting.store(true, Ordering::SeqCst);
                let _ = started_send.send(());
                std::thread::sleep(Duration::from_millis(300));
                emitted.store(true, Ordering::SeqCst);
            });
        });
        started_receive.recv_timeout(Duration::from_secs(10)).unwrap();
        docs.collection.close();
        assert!(
            emitted.load(Ordering::SeqCst),
            "the close waited for the emit that was in progress"
        );
    });
    assert!(emitting.load(Ordering::SeqCst));

    let late = AtomicBool::new(false);
    let outcome = docs
        .collection
        .refresh(&docs.fs, &docs.log, &|_| late.store(true, Ordering::SeqCst));
    assert!(outcome.stale);
    assert!(!late.load(Ordering::SeqCst), "a closed collection emits nothing");
}

// DCL-FR-MSHW: every internal operation says `store_unavailable` while the
// stored sources cannot be read, and `no_project_open` with no project.
#[test]
fn the_internal_operations_refuse_with_the_typed_errors() {
    let docs = Docs::new();
    docs.collection.open("project-key", Err(()));
    assert_eq!(
        docs.collection.resolve_document("doc-0").unwrap_err(),
        DocumentsError::StoreUnavailable
    );
    assert_eq!(
        docs.collection.document_text(&docs.fs, &docs.log, "doc-0").unwrap_err(),
        DocumentsError::StoreUnavailable
    );
    assert_eq!(
        docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap_err(),
        DocumentsError::StoreUnavailable
    );
    docs.collection.close();
    assert_eq!(
        docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap_err(),
        DocumentsError::NoProjectOpen
    );
}

// DCL-FR-MSHW: a failed read of the stored sources is tried again, and the
// collection recovers when it works.
#[test]
fn a_failed_store_read_is_tried_again() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.collection.open("project-key", Err(()));
    assert!(docs.refresh().is_empty(), "nothing is committed while the store is unreadable");

    assert!(!docs.collection.retry_store(&|_| Err(())), "a failing read stays failed");
    assert_eq!(docs.collection.snapshot().unwrap_err(), DocumentsError::StoreUnavailable);

    let recovered = docs.collection.retry_store(&|key| {
        assert_eq!(key, "project-key");
        Ok(vec![source(SourceKind::Folder, &docs.path_of("refs"))])
    });
    assert!(recovered);
    assert_eq!(docs.refresh().len(), 1);
    assert_eq!(docs.names(), ["a.md"]);
    assert!(docs.collection.retry_store(&|_| panic!("a readable store is not read again")));
}

// DCL-FR-MSHW, DCL-FR-VWSG: a retry that a project switch overtakes does not
// put the old project's sources into the new one.
#[test]
fn a_store_retry_overtaken_by_a_project_switch_is_dropped() {
    let docs = Docs::new();
    docs.collection.open("project-key", Err(()));
    let recovered = docs.collection.retry_store(&|_| {
        docs.collection.open("other-project", Err(()));
        Ok(vec![source(SourceKind::File, "/a.md")])
    });
    assert!(!recovered);
    assert!(docs.collection.stored_sources().is_empty());
}

// DCL-FR-XHSJ: stopping the watch is bounded. It does not wait for a watch that
// is still registering its paths, and that watch is discarded when it is done.
#[test]
fn stopping_the_watch_does_not_wait_for_a_watch_being_set_up() {
    let docs = Docs::new();
    let watcher = DocumentsWatcher::default();
    let targets = vec![WatchTarget {
        path: docs.root.clone(),
        recursive: false,
    }];
    let (entered_send, entered_receive) = mpsc::channel();
    let (release_send, release_receive) = mpsc::channel::<()>();
    let (watcher_ref, docs_ref) = (&watcher, &docs);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            watcher_ref.watch_with(
                targets,
                &docs_ref.log,
                || {},
                &|_, _| {
                    let _ = entered_send.send(());
                    let _ = release_receive.recv_timeout(Duration::from_secs(10));
                    true
                },
            );
        });
        entered_receive.recv_timeout(Duration::from_secs(10)).unwrap();

        let (stopped_send, stopped_receive) = mpsc::channel();
        scope.spawn(move || {
            watcher_ref.stop();
            let _ = stopped_send.send(());
        });
        stopped_receive
            .recv_timeout(Duration::from_secs(2))
            .expect("the stop ended while the watch was being set up");
        release_send.send(()).unwrap();
    });
    assert!(!watcher.is_armed(), "a watch that finished after the stop is discarded");
    assert!(watcher.targets().is_empty());
}

// DCL-FR-ZYQC, DCL-FR-XHSJ: without a stop in between, the same set-up arms the
// watch.
#[test]
fn a_watch_set_up_without_a_stop_is_armed() {
    let docs = Docs::new();
    let watcher = DocumentsWatcher::default();
    watcher.watch_with(
        vec![WatchTarget {
            path: docs.root.clone(),
            recursive: false,
        }],
        &docs.log,
        || {},
        &|_, _| true,
    );
    assert!(watcher.is_armed());
    watcher.stop();
    assert!(!watcher.is_armed());
}
