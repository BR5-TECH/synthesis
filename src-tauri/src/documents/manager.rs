//! The state of the Documents collection and its refresh
//! (`DCL-documents-collection.md`).
//!
//! The collection is a pure core: it holds the stored sources, the last
//! snapshot, a record per document, and the session PDF text cache, and it
//! reaches the disk only through the documents instance of `FsAccess`
//! (DCL-FR-BAQY). Tauri types stay out of it; `commands.rs` and `lifecycle.rs`
//! put the core on the application. The reading operations are in `reading.rs`.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use crate::fs::{EntryKind, FsAccess, FsAccessState, GrantRefusal};
use crate::logging::LogLevel;

use super::discovery::{discover, SourceReport};
use super::log::DocLog;
use super::model::{
    document_id, file_name_of, is_absolute_path, normalise_path, Availability, DocumentEntry,
    DocumentFormat, DocumentSource, DocumentsError, DocumentsSnapshot, SourceKind, StoredSource,
};

/// How a PDF's extraction ended, kept for the session (DCL-FR-QVYZ).
pub(super) struct PdfCacheEntry {
    pub revision: String,
    /// `None` is a PDF with no text, which is remembered so it is not extracted
    /// again for the same revision.
    pub text: Option<String>,
}

/// The extractor of PDF text. A function pointer so a test can substitute one
/// that panics or fails (DCL-FR-VRTS).
pub type Extractor = fn(&[u8]) -> Result<String, String>;

fn default_extractor(bytes: &[u8]) -> Result<String, String> {
    pdf_extract::extract_text_from_mem(bytes).map_err(|e| e.to_string())
}

/// DCL-FR-RXMB: the most bytes read from one document.
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
/// DCL-FR-DLPX: the largest PDF given to the extractor.
pub const MAX_EXTRACT_BYTES: usize = 32 * 1024 * 1024;
/// DCL-FR-DLPX: how long the caller waits for the extractor.
pub const EXTRACT_DEADLINE: Duration = Duration::from_secs(30);

/// The bounds on reads and on PDF extraction (DCL-FR-RXMB, DCL-FR-DLPX). A test
/// sets small ones, so it needs no file of 64 MiB.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_read_bytes: u64,
    pub max_extract_bytes: usize,
    pub extract_deadline: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_read_bytes: MAX_DOCUMENT_BYTES,
            max_extract_bytes: MAX_EXTRACT_BYTES,
            extract_deadline: EXTRACT_DEADLINE,
        }
    }
}

/// What a refresh compares to decide that a file must be read again
/// (DCL non-functional: size or modification time).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Fingerprint {
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// One document as the last refresh saw it.
#[derive(Clone, Debug)]
pub(super) struct DocRecord {
    pub entry: DocumentEntry,
    pub fingerprint: Option<Fingerprint>,
}

#[derive(Default)]
pub(super) struct Inner {
    /// Whether a project is open and this collection belongs to it.
    pub open: bool,
    /// Bumped on every open and close, so a refresh that began before one is
    /// recognised and dropped instead of committed.
    pub generation: u64,
    /// The per-project slot the sources are filed under (GSS-FR-CDYK). Empty
    /// while no project is open.
    pub project_key: String,
    /// False when the stored sources could not be read (DCL-FR-MSHW).
    pub store_ok: bool,
    pub sources: Vec<StoredSource>,
    pub snapshot: DocumentsSnapshot,
    pub records: BTreeMap<String, DocRecord>,
}

/// What a refresh did.
#[derive(Debug, Default)]
pub struct RefreshOutcome {
    /// The snapshot, when it differs from the previous one (DCL-FR-PTRP).
    pub changed: Option<DocumentsSnapshot>,
    /// The refresh was overtaken by a close, an open, or a source change and
    /// committed nothing.
    pub stale: bool,
}

/// The Documents collection, as managed state.
pub struct DocumentsCollection {
    pub(super) inner: Mutex<Inner>,
    /// Serialises source changes, so two of them cannot interleave their reads
    /// of the stored list.
    mutation: Mutex<()>,
    /// Serialises refreshes, so snapshots arise, and are announced, in order.
    refresh_gate: Mutex<()>,
    /// DCL-FR-VWSG: held while an event is emitted. An open or a close waits for
    /// it after it moved the generation, so no event is emitted after the open
    /// or the close returned.
    emit_gate: Mutex<()>,
    /// DCL-FR-QVYZ: extracted PDF text, kept for the application session. It is
    /// never written to disk and a close does not clear it.
    pub(super) pdf_cache: Mutex<HashMap<String, PdfCacheEntry>>,
    pub(super) extractor: Extractor,
    pub(super) limits: Limits,
    /// How many times the extractor ran. Lets a test observe cache reuse.
    pub(super) extractions: AtomicUsize,
}

impl Default for DocumentsCollection {
    fn default() -> Self {
        DocumentsCollection::with_extractor(default_extractor)
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock means another thread panicked while holding it. The data
    // is a snapshot that is rebuilt by the next refresh, so it is taken as is.
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl DocumentsCollection {
    /// A collection that extracts PDF text with `extractor`.
    pub fn with_extractor(extractor: Extractor) -> Self {
        DocumentsCollection::with_limits(extractor, Limits::default())
    }

    /// A collection that extracts PDF text with `extractor` and holds to
    /// `limits`.
    pub fn with_limits(extractor: Extractor, limits: Limits) -> Self {
        DocumentsCollection {
            inner: Mutex::new(Inner::default()),
            mutation: Mutex::new(()),
            refresh_gate: Mutex::new(()),
            emit_gate: Mutex::new(()),
            pdf_cache: Mutex::new(HashMap::new()),
            extractor,
            limits,
            extractions: AtomicUsize::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle (DCL-FR-QGLH, DCL-FR-XHSJ)
    // -----------------------------------------------------------------------

    /// DCL-FR-QGLH: the collection opens with the project. `stored` is what the
    /// store answered with; a failure to read it leaves the collection showing
    /// no documents (DCL-FR-MSHW).
    pub fn open(&self, project_key: &str, stored: Result<Vec<StoredSource>, ()>) {
        let mut inner = lock(&self.inner);
        let generation = inner.generation + 1;
        *inner = Inner {
            open: true,
            generation,
            project_key: project_key.to_string(),
            store_ok: stored.is_ok(),
            sources: stored.unwrap_or_default(),
            snapshot: DocumentsSnapshot::default(),
            records: BTreeMap::new(),
        };
        drop(inner);
        self.fence_emitters();
    }

    /// DCL-FR-VWSG: wait for the event that is being emitted, if any. Called
    /// after the generation moved, so a later refresh finds itself overtaken and
    /// emits nothing.
    fn fence_emitters(&self) {
        drop(lock(&self.emit_gate));
    }

    /// DCL-FR-MSHW: try the read of the stored sources again, for a collection
    /// that opened with a store it could not read. Returns whether the store is
    /// readable now. A collection that is closed, or that opened for another
    /// project while `load` ran, is left as it is.
    pub fn retry_store(&self, load: &dyn Fn(&str) -> Result<Vec<StoredSource>, ()>) -> bool {
        let (generation, key) = {
            let inner = lock(&self.inner);
            if !inner.open {
                return false;
            }
            if inner.store_ok {
                return true;
            }
            (inner.generation, inner.project_key.clone())
        };
        let Ok(sources) = load(&key) else {
            return false;
        };
        let mut inner = lock(&self.inner);
        if inner.generation != generation || !inner.open {
            return false;
        }
        inner.sources = sources;
        inner.store_ok = true;
        true
    }

    /// DCL-FR-XHSJ: closing the project discards the sources, the snapshot, and
    /// the records, and keeps the PDF cache for the application session.
    pub fn close(&self) {
        let mut inner = lock(&self.inner);
        let generation = inner.generation + 1;
        *inner = Inner {
            generation,
            ..Inner::default()
        };
        drop(inner);
        self.fence_emitters();
    }

    /// Whether a project is open and the collection is in use.
    pub fn is_open(&self) -> bool {
        lock(&self.inner).open
    }

    /// Whether the collection is open for the project filed under `project_key`.
    /// A change of active worktree keeps the key, so the collection stays open
    /// (DCL-FR-XHSJ).
    pub fn is_open_for(&self, project_key: &str) -> bool {
        let inner = lock(&self.inner);
        inner.open && inner.project_key == project_key
    }

    /// The slot the sources are stored under, or `None` with no project open.
    pub fn project_key(&self) -> Option<String> {
        let inner = lock(&self.inner);
        inner.open.then(|| inner.project_key.clone())
    }

    /// The sources as stored, in order of first addition.
    pub fn stored_sources(&self) -> Vec<StoredSource> {
        lock(&self.inner).sources.clone()
    }

    // -----------------------------------------------------------------------
    // Snapshot (DCL-FR-SQEP, DCL-FR-MSHW)
    // -----------------------------------------------------------------------

    /// DCL-FR-MSHW: the typed refusals every operation shares.
    pub(super) fn require_open(&self) -> Result<(), DocumentsError> {
        let inner = lock(&self.inner);
        if !inner.open {
            return Err(DocumentsError::NoProjectOpen);
        }
        if !inner.store_ok {
            return Err(DocumentsError::StoreUnavailable);
        }
        Ok(())
    }

    /// DCL-FR-SQEP: the in-memory snapshot. It never waits for a refresh.
    pub fn snapshot(&self) -> Result<DocumentsSnapshot, DocumentsError> {
        let inner = lock(&self.inner);
        if !inner.open {
            return Err(DocumentsError::NoProjectOpen);
        }
        if !inner.store_ok {
            return Err(DocumentsError::StoreUnavailable);
        }
        Ok(inner.snapshot.clone())
    }

    // -----------------------------------------------------------------------
    // Sources (DCL-FR-PHYP, DCL-FR-ATNL)
    // -----------------------------------------------------------------------

    /// DCL-FR-PHYP: add sources, in the order given, after the stored ones.
    ///
    /// A source whose normalised path equals a stored one's, or an earlier one's
    /// in this call, adds nothing and is not an error. A path that is not
    /// absolute refuses the whole call with `invalid_path` before anything is
    /// stored. `persist` writes the new list to the store; the in-memory list
    /// changes only when it succeeded. Returns the sources that were new.
    pub fn add_sources(
        &self,
        requested: &[StoredSource],
        persist: &dyn Fn(&str, &[StoredSource]) -> Result<(), String>,
    ) -> Result<Vec<StoredSource>, DocumentsError> {
        let _mutation = lock(&self.mutation);
        let (generation, key, mut list) = self.begin_change()?;
        if requested.iter().any(|s| !is_absolute_path(&s.path)) {
            return Err(DocumentsError::InvalidPath);
        }
        let mut added = Vec::new();
        for source in requested {
            let key = normalise_path(&source.path);
            if list.iter().any(|s| normalise_path(&s.path) == key) {
                continue;
            }
            list.push(source.clone());
            added.push(source.clone());
        }
        if added.is_empty() {
            return Ok(added);
        }
        persist(&key, &list).map_err(|_| DocumentsError::StoreUnavailable)?;
        self.apply_sources(generation, list)?;
        Ok(added)
    }

    /// DCL-FR-VWSG: the generation, the project key, and the stored sources a
    /// change begins from, read together so they belong to one project.
    fn begin_change(&self) -> Result<(u64, String, Vec<StoredSource>), DocumentsError> {
        let inner = lock(&self.inner);
        if !inner.open {
            return Err(DocumentsError::NoProjectOpen);
        }
        if !inner.store_ok {
            return Err(DocumentsError::StoreUnavailable);
        }
        Ok((inner.generation, inner.project_key.clone(), inner.sources.clone()))
    }

    /// DCL-FR-VWSG: put a changed list in memory, but only while the project the
    /// change began for is still the open one. The list was stored under that
    /// project's key already, and no other project takes it.
    fn apply_sources(&self, generation: u64, list: Vec<StoredSource>) -> Result<(), DocumentsError> {
        let mut inner = lock(&self.inner);
        if inner.generation != generation || !inner.open {
            return Err(DocumentsError::NoProjectOpen);
        }
        inner.sources = list;
        Ok(())
    }

    /// DCL-FR-ATNL: remove the stored source whose normalised path equals the
    /// normalised `path`. A path that matches nothing removes nothing and is not
    /// an error. Returns the source that was removed.
    pub fn remove_source(
        &self,
        path: &str,
        persist: &dyn Fn(&str, &[StoredSource]) -> Result<(), String>,
    ) -> Result<Option<StoredSource>, DocumentsError> {
        let _mutation = lock(&self.mutation);
        let (generation, project_key, mut list) = self.begin_change()?;
        let key = normalise_path(path);
        let Some(at) = list.iter().position(|s| normalise_path(&s.path) == key) else {
            return Ok(None);
        };
        let removed = list.remove(at);
        persist(&project_key, &list).map_err(|_| DocumentsError::StoreUnavailable)?;
        self.apply_sources(generation, list)?;
        Ok(Some(removed))
    }

    // -----------------------------------------------------------------------
    // Refresh (DCL-FR-QGLH, DCL-FR-LRUP, DCL-FR-ZYQC, DCL-FR-PTRP)
    // -----------------------------------------------------------------------

    /// Run one refresh: bring the documents instance in line, so its reach
    /// equals the grantable stored sources exactly (DCL-FR-LRUP), discover the documents, read the
    /// ones whose size or modification time changed, and replace the snapshot.
    ///
    /// Refreshes run one at a time, in the order they were asked for, so the
    /// snapshots they produce arise in that order (DCL-FR-PTRP). The snapshot is
    /// replaced only when no close, open, or source change overtook the refresh.
    ///
    /// `on_change` runs, still inside that ordering, with the new snapshot when
    /// it differs from the previous one. It is where `"documents changed"` is
    /// emitted, so the event reaches the frontend in the order the snapshots
    /// arose.
    pub fn refresh(
        &self,
        fs: &FsAccessState,
        log: &dyn DocLog,
        on_change: &dyn Fn(&DocumentsSnapshot),
    ) -> RefreshOutcome {
        let _gate = lock(&self.refresh_gate);
        let started = Instant::now();
        let (generation, sources, previous_snapshot, previous_records) = {
            let inner = lock(&self.inner);
            if !inner.open || !inner.store_ok {
                return RefreshOutcome {
                    changed: None,
                    stale: true,
                };
            }
            (
                inner.generation,
                inner.sources.clone(),
                inner.snapshot.clone(),
                inner.records.clone(),
            )
        };

        // FSA-FR-WBKZ: the instance follows the sources as they stand, so a
        // removed source is refused from here on. It is rebuilt only when the
        // set of grantable sources changed.
        let mut files: Vec<PathBuf> = Vec::new();
        let mut folders: Vec<PathBuf> = Vec::new();
        for source in &sources {
            match source.kind {
                SourceKind::File => files.push(PathBuf::from(&source.path)),
                SourceKind::Folder => folders.push(PathBuf::from(&source.path)),
            }
        }
        let refused: HashMap<String, GrantRefusal> = fs
            .install_documents(&files, &folders)
            .into_iter()
            .map(|(path, reason)| (normalise_path(&path.to_string_lossy()), reason))
            .collect();
        let access = fs.documents();
        let discovery = discover(access.as_deref(), &refused, &sources, log);

        let mut records: BTreeMap<String, DocRecord> = BTreeMap::new();
        for (path, format) in &discovery.files {
            let id = document_id(path);
            let record = read_record(
                access.as_deref(),
                &id,
                path,
                *format,
                previous_records.get(&id),
                &self.limits,
                log,
            );
            records.insert(id, record);
        }
        for (path, format) in &discovery.unavailable_files {
            let id = document_id(path);
            records
                .entry(id.clone())
                .or_insert_with(|| unavailable_record(&id, path, *format, None));
        }
        let snapshot = build_snapshot(&discovery.reports, &records);

        let changed = {
            let mut inner = lock(&self.inner);
            // A close or an open moved the generation, and a source change
            // moved the list this walk was made over. Either way the next
            // refresh is already owed, so this result is dropped.
            if inner.generation != generation || !inner.open || inner.sources != sources {
                // A close that overtook this refresh already discarded the
                // documents instance, and the rebuild above must not bring it
                // back (DCL-FR-XHSJ).
                if !inner.open {
                    fs.discard_documents();
                }
                return RefreshOutcome {
                    changed: None,
                    stale: true,
                };
            }
            let changed = inner.snapshot != snapshot;
            inner.records = records;
            inner.snapshot = snapshot.clone();
            changed
        };

        log_refresh(log, &previous_snapshot, &snapshot, changed, started);
        if changed {
            // DCL-FR-VWSG: the event is emitted only while the project this
            // refresh ran for is still open, and an open or a close waits for
            // the emit that is in progress, so none follows its return.
            let _emit = lock(&self.emit_gate);
            let current = {
                let inner = lock(&self.inner);
                inner.open && inner.generation == generation
            };
            if !current {
                return RefreshOutcome {
                    changed: None,
                    stale: true,
                };
            }
            on_change(&snapshot);
        }
        RefreshOutcome {
            changed: changed.then_some(snapshot),
            stale: false,
        }
    }
}

/// The record of one discovered file: the previous one when its fingerprint is
/// unchanged, otherwise a fresh read (DCL-FR-ZYQC).
fn read_record(
    access: Option<&FsAccess>,
    id: &str,
    path: &str,
    format: DocumentFormat,
    previous: Option<&DocRecord>,
    limits: &Limits,
    log: &dyn DocLog,
) -> DocRecord {
    let Some(access) = access else {
        return unavailable_record(id, path, format, None);
    };
    let fingerprint = match access.file_info(PathBuf::from(path)) {
        Ok(info) if info.kind == EntryKind::File => Some(Fingerprint {
            size: info.size,
            modified: info.modified,
        }),
        _ => None,
    };
    let Some(fingerprint) = fingerprint else {
        return unavailable_record(id, path, format, None);
    };
    if let Some(previous) = previous {
        if previous.entry.status == Availability::Available
            && previous.entry.revision.is_some()
            && previous.fingerprint.as_ref() == Some(&fingerprint)
        {
            return previous.clone();
        }
    }
    match super::reading::revision_of(access, path, format, limits.max_read_bytes) {
        Ok(revision) => DocRecord {
            entry: DocumentEntry {
                id: id.to_string(),
                path: path.to_string(),
                name: file_name_of(path).to_string(),
                format,
                status: Availability::Available,
                revision: Some(revision),
            },
            fingerprint: Some(fingerprint),
        },
        Err(failure) => {
            log.log(
                LogLevel::Warn,
                "document cannot be read",
                crate::log_fields! {
                    "document" => id,
                    "format" => format.as_str(),
                    "reason" => failure.reason(),
                },
            );
            unavailable_record(id, path, format, Some(fingerprint))
        }
    }
}

fn unavailable_record(
    id: &str,
    path: &str,
    format: DocumentFormat,
    fingerprint: Option<Fingerprint>,
) -> DocRecord {
    DocRecord {
        entry: DocumentEntry {
            id: id.to_string(),
            path: path.to_string(),
            name: file_name_of(path).to_string(),
            format,
            status: Availability::Unavailable,
            revision: None,
        },
        fingerprint,
    }
}

/// DCL-FR-SQEP: the sources in stored order and the documents sorted by
/// normalised path.
fn build_snapshot(
    reports: &[SourceReport],
    records: &BTreeMap<String, DocRecord>,
) -> DocumentsSnapshot {
    let mut documents: Vec<DocumentEntry> = records.values().map(|r| r.entry.clone()).collect();
    documents.sort_by(|a, b| a.path.cmp(&b.path));
    DocumentsSnapshot {
        sources: reports
            .iter()
            .map(|report| DocumentSource {
                kind: report.source.kind,
                path: report.source.path.clone(),
                status: report.status,
                reason: report.reason,
            })
            .collect(),
        documents,
    }
}

/// DCL-FR-UPFP: one `INFO` record per refresh, and one `WARN` record for each
/// source that has newly become unavailable. No path, name, or content.
fn log_refresh(
    log: &dyn DocLog,
    previous: &DocumentsSnapshot,
    snapshot: &DocumentsSnapshot,
    changed: bool,
    started: Instant,
) {
    for source in &snapshot.sources {
        if source.status != Availability::Unavailable {
            continue;
        }
        let was_unavailable = previous.sources.iter().any(|old| {
            old.kind == source.kind
                && old.path == source.path
                && old.status == Availability::Unavailable
                && old.reason == source.reason
        });
        if !was_unavailable {
            log.log(
                LogLevel::Warn,
                "document source is unavailable",
                crate::log_fields! {
                    "source_kind" => source.kind.as_str(),
                    "reason" => source.reason.map(|r| r.as_str()).unwrap_or("unreadable"),
                },
            );
        }
    }
    let unavailable_documents = snapshot
        .documents
        .iter()
        .filter(|d| d.status == Availability::Unavailable)
        .count();
    log.log(
        LogLevel::Info,
        "documents refreshed",
        crate::log_fields! {
            "sources" => snapshot.sources.len(),
            "documents" => snapshot.documents.len(),
            "unavailable_documents" => unavailable_documents,
            "changed" => changed,
            "duration_ms" => started.elapsed().as_millis() as u64,
        },
    );
}
