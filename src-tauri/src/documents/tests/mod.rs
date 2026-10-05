//! Tests for the Documents collection (`DCL-documents-collection.md`).
//!
//! The pure core runs against a real temporary tree through the documents
//! instance of `FsAccess`, so every claim about discovery, access, and reading is
//! checked against files rather than against a stub. The tests that need the
//! application — the event, the store, the index, the watch — use
//! `tauri::test::mock_app`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tempfile::TempDir;

use super::log::DocLog;
use super::manager::DocumentsCollection;
use super::model::{DocumentsSnapshot, SourceKind, StoredSource};
use crate::fs::FsAccessState;
use crate::logging::{Fields, LogLevel};

mod discovery_tests;
mod lifecycle_tests;
mod paths;
mod persistence;
mod reading_tests;
mod robustness;
mod sources;
mod watching;

/// A sink that keeps every record, so a test can read back what was reported.
#[derive(Default)]
pub struct Recorder(Mutex<Vec<(LogLevel, String, Fields)>>);

impl DocLog for Recorder {
    fn log(&self, level: LogLevel, message: &str, fields: Fields) {
        self.0
            .lock()
            .unwrap()
            .push((level, message.to_string(), fields));
    }
}

impl Recorder {
    pub fn records(&self) -> Vec<(LogLevel, String, Fields)> {
        self.0.lock().unwrap().clone()
    }

    pub fn count(&self, level: LogLevel) -> usize {
        self.records().iter().filter(|r| r.0 == level).count()
    }

    /// Every record as one string, for the claims that something never appears.
    pub fn everything(&self) -> String {
        format!("{:?}", self.records())
    }
}

/// The extractor most tests use: the bytes read as text, so a "PDF" is a file
/// whose content is its extracted text.
pub fn extract_as_text(bytes: &[u8]) -> Result<String, String> {
    String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
}

pub fn extract_with_failure(_bytes: &[u8]) -> Result<String, String> {
    Err("not a pdf".to_string())
}

pub fn extract_with_panic(_bytes: &[u8]) -> Result<String, String> {
    panic!("the extractor fell over");
}

/// A temporary tree, a collection, and the filesystem state it reads through.
pub struct Docs {
    pub _tmp: TempDir,
    pub root: PathBuf,
    pub fs: FsAccessState,
    pub collection: DocumentsCollection,
    pub log: Recorder,
}

impl Docs {
    pub fn new() -> Docs {
        Docs::with_extractor(extract_as_text)
    }

    pub fn with_extractor(extractor: super::manager::Extractor) -> Docs {
        Docs::with_limits(extractor, super::manager::Limits::default())
    }

    pub fn with_limits(
        extractor: super::manager::Extractor,
        limits: super::manager::Limits,
    ) -> Docs {
        let tmp = TempDir::new().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let docs = Docs {
            _tmp: tmp,
            root,
            fs: FsAccessState::default(),
            collection: DocumentsCollection::with_limits(extractor, limits),
            log: Recorder::default(),
        };
        docs.collection.open("project-key", Ok(Vec::new()));
        docs
    }

    /// Write `content` at `relative`, creating parents, and return the path.
    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn path_of(&self, relative: &str) -> String {
        self.root.join(relative).to_string_lossy().into_owned()
    }

    pub fn add_file(&self, relative: &str) {
        self.add(SourceKind::File, relative);
    }

    pub fn add_folder(&self, relative: &str) {
        self.add(SourceKind::Folder, relative);
    }

    pub fn add(&self, kind: SourceKind, relative: &str) {
        let source = StoredSource {
            kind,
            path: self.path_of(relative),
        };
        self.collection
            .add_sources(&[source], &|_, _| Ok(()))
            .expect("the source is added");
    }

    pub fn remove(&self, relative: &str) {
        self.collection
            .remove_source(&self.path_of(relative), &|_, _| Ok(()))
            .expect("the source is removed");
    }

    /// One refresh, with the changes it announces collected.
    pub fn refresh(&self) -> Vec<DocumentsSnapshot> {
        let announced = Mutex::new(Vec::new());
        self.collection
            .refresh(&self.fs, &self.log, &|snapshot| {
                announced.lock().unwrap().push(snapshot.clone())
            });
        announced.into_inner().unwrap()
    }

    pub fn snapshot(&self) -> DocumentsSnapshot {
        self.collection.snapshot().expect("the collection is open")
    }

    /// The names of the documents in the snapshot, in snapshot order.
    pub fn names(&self) -> Vec<String> {
        self.snapshot()
            .documents
            .iter()
            .map(|d| d.name.clone())
            .collect()
    }

    pub fn id_of(&self, relative: &str) -> String {
        super::model::document_id(&super::model::normalise_path(&self.path_of(relative)))
    }
}

/// Whether the process can still read a folder after its permissions were
/// removed, which is the case for a privileged user.
pub fn can_read_despite_mode_zero(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok()
}
