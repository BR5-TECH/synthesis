//! The Documents collection on the application: open with the project, refresh
//! in the background, watch the selected paths, and close
//! (`DCL-documents-collection.md` DCL-FR-QGLH, DCL-FR-LRUP, DCL-FR-ZYQC,
//! DCL-FR-PTRP, DCL-FR-XHSJ).
//!
//! The pure core is in `manager.rs`. This module gives it the Tauri pieces it
//! must not hold: the user-global store that keeps the sources, the event that
//! announces a new snapshot, the watch, the background thread, and the hand-over
//! to the `documents` index of `BMI-bm25-indexing.md`.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use tauri::{Emitter, Manager};

use crate::bm25_index::{self, PassScope};
use crate::fs::FsAccessState;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::LogLevel;

use super::log::DocLog;
use super::manager::{DocumentsCollection, RefreshOutcome};
use super::model::{
    format_of, DocumentsError, DocumentsSnapshot, PickDocumentSourcesResult, PickMode, SourceKind,
    StoredSource,
};
use super::watcher::{watch_targets, DocumentsWatcher};

/// DCL-FR-PTRP: the event that carries the whole snapshot to the frontend.
pub const DOCUMENTS_CHANGED: &str = "documents-changed";

/// DCL-FR-QGLH: the content root of a project was mounted. This runs on a project
/// open and on every change of active worktree, because both mount a root.
///
/// An open loads the stored sources and starts the collection. A change of
/// worktree finds the collection already open for the same project key, so it
/// changes no source and only asks for one refresh (DCL-FR-XHSJ). The refresh
/// runs in the background, so the mount never waits for it.
pub fn on_content_root_mounted<R: tauri::Runtime>(app: &tauri::AppHandle<R>, project_key: &str) {
    let (Some(collection), Some(_)) = (
        app.try_state::<DocumentsCollection>(),
        app.try_state::<DocumentsWatcher>(),
    ) else {
        return;
    };
    if project_key.is_empty() {
        return;
    }
    if !collection.is_open_for(project_key) {
        let stored = match app.try_state::<GlobalSettingsStore>() {
            Some(store) => store.load_document_sources(project_key).map_err(|_| ()),
            None => Err(()),
        };
        let store_ok = stored.is_ok();
        let count = stored.as_ref().map(Vec::len).unwrap_or(0);
        collection.open(project_key, stored);
        if store_ok {
            app.log(
                LogLevel::Info,
                "documents collection opened",
                crate::log_fields! { "sources" => count },
            );
        } else {
            app.log(
                LogLevel::Error,
                "document sources could not be read",
                crate::log_fields! {},
            );
        }
    }
    request_refresh(app);
}

/// DCL-FR-XHSJ: closing the project stops the watch, discards the snapshot, the
/// sources in memory, and the documents instance, and keeps the PDF cache for
/// the application session. The `documents` index goes with the other indexes
/// (BMI-FR-25).
pub fn on_project_closed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    // The collection closes first and the watch stops after it, so a refresh that
    // is still running and arms a watch late finds the collection closed, and
    // stops that watch itself (see `rearm_watch`).
    if let Some(collection) = app.try_state::<DocumentsCollection>() {
        if collection.is_open() {
            collection.close();
            app.log(
                LogLevel::Info,
                "documents collection closed",
                crate::log_fields! {},
            );
        }
    }
    if let Some(watcher) = app.try_state::<DocumentsWatcher>() {
        watcher.stop();
    }
    if let Some(fs) = app.try_state::<FsAccessState>() {
        fs.discard_documents();
    }
}

/// DCL-FR-QGLH, DCL-FR-ZYQC: ask for a refresh. It returns at once. A burst of
/// requests costs one refresh after the one that is running, so the watch and
/// the commands never queue one refresh per request.
pub fn request_refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(watcher) = app.try_state::<DocumentsWatcher>() else {
        return;
    };
    watcher.pending.store(true, Ordering::SeqCst);
    if watcher.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(watcher) = app.try_state::<DocumentsWatcher>() else {
            return;
        };
        loop {
            while watcher.pending.swap(false, Ordering::SeqCst) {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_refresh(&app)
                }));
                if outcome.is_err() {
                    // DCL-FR-UPFP: a failed refresh is an `ERROR` record. The
                    // thread carries on, so the next change still refreshes.
                    app.log(
                        LogLevel::Error,
                        "documents refresh failed",
                        crate::log_fields! {},
                    );
                }
            }
            watcher.running.store(false, Ordering::SeqCst);
            // A request that landed after the last check and before the store
            // above would otherwise wait for the next one.
            if !watcher.pending.load(Ordering::SeqCst)
                || watcher.running.swap(true, Ordering::SeqCst)
            {
                return;
            }
        }
    });
}

/// One refresh, on the calling thread: rebuild the documents instance, discover
/// and read, replace the snapshot, announce a changed snapshot, feed the index,
/// and bring the watch in line with the sources.
pub fn run_refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> RefreshOutcome {
    let (Some(collection), Some(fs)) = (
        app.try_state::<DocumentsCollection>(),
        app.try_state::<FsAccessState>(),
    ) else {
        return RefreshOutcome {
            changed: None,
            stale: true,
        };
    };
    if collection.is_open() && collection.snapshot() == Err(DocumentsError::StoreUnavailable) {
        retry_store(app);
    }
    let outcome = collection.refresh(&fs, app, &|snapshot| {
        // DCL-FR-PTRP: emitted inside the refresh gate, so the events reach the
        // frontend in the order the snapshots arose.
        if let Err(error) = app.emit(DOCUMENTS_CHANGED, snapshot) {
            app.log(
                LogLevel::Warn,
                "documents changed event could not be sent",
                crate::log_fields! { "reason" => error.to_string() },
            );
        }
        // BMI-FR-MWNQ: an added, changed, or removed document reaches the
        // `documents` index through one coalesced pass.
        bm25_index::request_pass(app, PassScope::DOCUMENTS);
    });
    if !outcome.stale {
        rearm_watch(app, &collection);
    }
    outcome
}

/// DCL-FR-ZYQC: watch exactly the available sources. The watch is replaced only
/// when the set of targets changed, so a refresh that a watched change caused
/// does not tear down the watch that reported it.
fn rearm_watch<R: tauri::Runtime>(app: &tauri::AppHandle<R>, collection: &DocumentsCollection) {
    let Some(watcher) = app.try_state::<DocumentsWatcher>() else {
        return;
    };
    let Ok(snapshot) = collection.snapshot() else {
        watcher.stop();
        return;
    };
    let targets = watch_targets(&snapshot.sources);
    if watcher.targets() == targets && (watcher.is_armed() || targets.is_empty()) {
        return;
    }
    let handle = app.clone();
    watcher.watch(targets, app, move || request_refresh(&handle));
    // DCL-FR-XHSJ: a close that landed while this refresh ran must not leave a
    // watch behind.
    if !collection.is_open() {
        watcher.stop();
    }
}

/// DCL-FR-LRUP: add `requested`, persist the list, run a refresh, and answer with
/// the snapshot. The documents instance is rebuilt by the refresh, so its reach
/// equals the stored sources when this returns.
pub fn add_sources<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    requested: &[StoredSource],
) -> Result<DocumentsSnapshot, DocumentsError> {
    let started = std::time::Instant::now();
    let collection = app
        .try_state::<DocumentsCollection>()
        .ok_or(DocumentsError::NoProjectOpen)?;
    let persist = persister(app);
    let added = collection
        .add_sources(requested, &*persist)
        .inspect_err(|error| log_refused(app, "add document sources", *error))?;
    run_refresh(app);
    let snapshot = collection.snapshot()?;
    app.log(
        LogLevel::Info,
        "document sources added",
        crate::log_fields! {
            "files" => added.iter().filter(|s| s.kind == SourceKind::File).count(),
            "folders" => added.iter().filter(|s| s.kind == SourceKind::Folder).count(),
            "documents" => snapshot.documents.len(),
            "duration_ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(snapshot)
}

/// DCL-FR-ATNL, DCL-FR-LRUP: remove one source and refresh before answering. The
/// file or folder the source named is not touched.
pub fn remove_source<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    path: &str,
) -> Result<DocumentsSnapshot, DocumentsError> {
    let started = std::time::Instant::now();
    let collection = app
        .try_state::<DocumentsCollection>()
        .ok_or(DocumentsError::NoProjectOpen)?;
    let persist = persister(app);
    let removed = collection
        .remove_source(path, &*persist)
        .inspect_err(|error| log_refused(app, "remove document source", *error))?;
    run_refresh(app);
    let snapshot = collection.snapshot()?;
    app.log(
        LogLevel::Info,
        "document source removed",
        crate::log_fields! {
            "removed" => removed.is_some(),
            "source_kind" => removed.as_ref().map(|s| s.kind.as_str()).unwrap_or("none"),
            "documents" => snapshot.documents.len(),
            "duration_ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(snapshot)
}

/// DCL-FR-VXXI: turn what the picker returned into sources and add them.
///
/// `picked` is `None` when the user cancelled. A cancel adds nothing and is not
/// an error. With the `files` mode a selected file of an unsupported type is
/// ignored and counted (DCL-FR-FGGU); with `folder` the one folder is a source.
pub fn add_picked_sources<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    mode: PickMode,
    picked: Option<Vec<PathBuf>>,
) -> Result<PickDocumentSourcesResult, DocumentsError> {
    let collection = app
        .try_state::<DocumentsCollection>()
        .ok_or(DocumentsError::NoProjectOpen)?;
    let Some(paths) = picked else {
        app.log(
            LogLevel::Info,
            "document source selection cancelled",
            crate::log_fields! { "mode" => mode_name(mode) },
        );
        return Ok(PickDocumentSourcesResult {
            cancelled: true,
            ignored_count: 0,
            snapshot: collection.snapshot()?,
        });
    };
    let (sources, ignored_count) = sources_from_picked(mode, &paths);
    if ignored_count > 0 {
        app.log(
            LogLevel::Info,
            "unsupported files were ignored",
            crate::log_fields! { "ignored" => ignored_count },
        );
    }
    let snapshot = add_sources(app, &sources)?;
    Ok(PickDocumentSourcesResult {
        cancelled: false,
        ignored_count,
        snapshot,
    })
}

/// DCL-FR-VXXI, DCL-FR-FGGU: the sources a picker result stands for, and how
/// many selected files were ignored for their type. A path is taken as the
/// picker gave it and is not touched on disk here.
pub fn sources_from_picked(mode: PickMode, paths: &[PathBuf]) -> (Vec<StoredSource>, usize) {
    let mut sources = Vec::new();
    let mut ignored = 0usize;
    for path in paths {
        let text = path.to_string_lossy().into_owned();
        match mode {
            PickMode::Folder => sources.push(StoredSource {
                kind: SourceKind::Folder,
                path: text,
            }),
            PickMode::Files => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if format_of(&name).is_some() {
                    sources.push(StoredSource {
                        kind: SourceKind::File,
                        path: text,
                    });
                } else {
                    ignored += 1;
                }
            }
        }
    }
    (sources, ignored)
}

fn mode_name(mode: PickMode) -> &'static str {
    match mode {
        PickMode::Files => "files",
        PickMode::Folder => "folder",
    }
}

/// The write of the source list into the per-project slot of the user-global
/// store (GSS-FR-CDYK). It stores paths and nothing else (DCL-FR-VEVZ).
///
/// DCL-FR-VWSG: the key is the one the change began for, which the collection
/// passes in, so a project switch during the write cannot move the list into
/// another project's slot.
fn persister<'a, R: tauri::Runtime>(
    app: &'a tauri::AppHandle<R>,
) -> Box<dyn Fn(&str, &[StoredSource]) -> Result<(), String> + 'a> {
    Box::new(move |key: &str, list: &[StoredSource]| {
        let store = app
            .try_state::<GlobalSettingsStore>()
            .ok_or("the settings store is not available")?;
        store.save_document_sources(key, list.to_vec())
    })
}

/// DCL-FR-MSHW: try the read of the stored sources again when the collection
/// opened without them. Returns whether the store is readable now. It is the
/// first step of every refresh, and a command that met `store_unavailable` calls
/// it before it answers.
pub fn retry_store<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let Some(collection) = app.try_state::<DocumentsCollection>() else {
        return false;
    };
    let Some(store) = app.try_state::<GlobalSettingsStore>() else {
        return false;
    };
    let recovered = collection.retry_store(&|key| store.load_document_sources(key).map_err(|_| ()));
    if recovered {
        app.log(
            LogLevel::Info,
            "document sources are readable again",
            crate::log_fields! {},
        );
    }
    recovered
}

/// DCL-FR-UPFP: a refused command is a `WARN` record that names the command and
/// the typed error and nothing the user selected.
pub fn log_refused<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    command: &str,
    error: DocumentsError,
) {
    app.log(
        LogLevel::Warn,
        "documents command refused",
        crate::log_fields! { "command" => command, "error" => error.code() },
    );
}
