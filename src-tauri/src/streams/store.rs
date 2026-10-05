//! Where a stream lives on disk, and how it is read back (WKS-FR-JGCA).

use std::path::{Path, PathBuf};

use super::*;

/// WKS-FR-JGCA: every stream's own directory, under the store root.
const STREAMS_DIR: &str = "w";
/// The stream's record, beside the working copy rather than inside it.
///
/// WKS-FR-JGCA puts the working copy at `w/<id>/` exactly, because the
/// container names the path the host names and every segment above the
/// checkout is a segment the agent repeats on each command. The record
/// therefore stands beside that directory rather than under it.
const RECORD_SUFFIX: &str = ".toml";

/// WKS-FR-JGCA / WKS-FR-SMKU: the one root this module writes under.
///
/// It is `short_data_dir()` and nothing else: no part of a stream is written
/// into a project's `.synthesis/`, and none of it is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamStore {
    root: PathBuf,
}

impl StreamStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The one root this module writes under.
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    /// The directory every stream of every project stands under.
    pub fn streams_root(&self) -> PathBuf {
        self.root.join(STREAMS_DIR)
    }

    /// One stream's record, beside its working copy.
    pub fn record(&self, stream_id: &str) -> PathBuf {
        self.streams_root().join(format!("{stream_id}{RECORD_SUFFIX}"))
    }

    /// WKS-FR-JGCA: one stream's working copy, at `w/<id>/` and no deeper.
    ///
    /// The path an execution turn stands in, and the path the container names,
    /// so every segment of it is short.
    pub fn worktree(&self, stream_id: &str) -> PathBuf {
        self.streams_root().join(stream_id)
    }
}

/// WKS-FR-QMTV: a stream id, unique across projects.
///
/// Prefixed so a directory under the store root is recognisable as a stream's
/// without opening it.
pub(super) fn new_stream_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("w{nanos:x}{:x}", std::process::id())
}

/// Whether a directory name under the store root is a stream's.
pub(super) fn is_stream_id(name: &str) -> bool {
    match name.strip_prefix('w') {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// The record of one stream, or `None` where it cannot be read.
///
/// A record that cannot be read is dropped from a listing rather than failing
/// the whole of it, exactly as a run record is: one unreadable file would
/// otherwise take every other stream of the project with it.
pub(super) fn read_stream(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream_id: &str,
) -> Option<WorkStream> {
    let mut stream: WorkStream = fs.read_toml(store.record(stream_id)).ok()?;
    mark_missing(&mut stream);
    Some(stream)
}

/// Write one stream's record durably.
pub(super) fn write_stream(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream: &WorkStream,
) -> Result<(), String> {
    ensure_dir(fs, &store.streams_root())?;
    fs.write_toml_atomic(store.record(&stream.id), stream)
        .map_err(|e| e.to_string())
}

/// Remove one stream's record and its working copy.
pub(super) fn remove_stream_home(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream_id: &str,
) -> Result<(), String> {
    let dir = store.worktree(stream_id);
    if dir.exists() {
        // FSA-FR-ZUCF: the working copy holds whatever the project's own
        // install put there, and a store of symbolic links is not a reason to
        // refuse the removal. The record below is one file and stays strict.
        fs.delete_owned_tree(&dir).map_err(|e| e.to_string())?;
    }
    let record = store.record(stream_id);
    if record.exists() {
        fs.delete_path(&record, false).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Every stream the store holds, in a stable order by id.
///
/// The store is read rather than an index, so a stream is never lost by an
/// index that disagrees with the directories beside it.
pub(super) fn list_streams(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    project_key: &str,
) -> Vec<WorkStream> {
    let Ok(entries) = fs.list_dir(store.streams_root()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .iter()
        .filter_map(|e| {
            let name = e.name.as_str();
            name.strip_suffix(RECORD_SUFFIX)
                .filter(|id| is_stream_id(id))
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids.iter()
        .filter_map(|id| read_stream(fs, store, id))
        // WKS-FR-SGCM / WKS-FR-HRUZ: every listing is of one project's streams.
        .filter(|stream| stream.project_key == project_key)
        .collect()
}

/// WKS-FR-IPCE: create a directory through the guarded instance.
pub(super) fn ensure_dir(fs: &fsa::FsAccess, dir: &Path) -> Result<(), String> {
    if dir.is_dir() {
        return Ok(());
    }
    fs.create_dir(dir).map_err(|e| e.to_string())
}
