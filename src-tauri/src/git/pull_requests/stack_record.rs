//! The stack record (`../../../specifications/core/GTC-git.md` GTC-FR-RNXK,
//! GTC-FR-OIMF, GTC-FR-AEBW; `RMS-repository-machine-storage.md`
//! RMS-FR-QPLE).
//!
//! A GitHub stack holds only pull requests. A branch that the author put on top
//! of a stack before it has a pull request is therefore known only on this
//! machine, in `stacks.toml` of the repository store. Every worktree of the
//! repository reads the same file. A change is a read of the whole file and one
//! atomic write of it.

use serde::{Deserialize, Serialize};

use crate::fs;

/// The file name of the record in the repository store.
pub(crate) const STACKS_FILE: &str = "stacks.toml";

/// The store cannot hold the record (RMS-FR-WGQS).
pub const ERR_STORE_UNAVAILABLE: &str = "store_unavailable";

/// One entry: `branch` stands on `parent` in a stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StackRecordEntry {
    pub(crate) branch: String,
    pub(crate) parent: String,
    /// The GitHub stack, when one was known when the entry was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stack: Option<u64>,
}

/// The whole file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct StackRecordFile {
    #[serde(default, rename = "entry")]
    entries: Vec<StackRecordEntry>,
}

/// One lock for every read-modify-write of the record in this process. It is
/// not a lock between processes (RMS-FR-PFOB).
static RECORD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    RECORD_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn record_path(machine: &fs::RootFs) -> Result<std::path::PathBuf, String> {
    fs::resolve_under(machine.path(), STACKS_FILE).map_err(|_| ERR_STORE_UNAVAILABLE.to_string())
}

/// GTC-FR-RNXK: every entry on disk. An absent file is an empty record. A file
/// that cannot be read or parsed is an error, which the caller reads as an
/// empty record and logs.
pub(crate) fn read_entries(machine: &fs::RootFs) -> Result<Vec<StackRecordEntry>, String> {
    let path = record_path(machine)?;
    match machine.file_info(&path) {
        Ok(_) => {}
        Err(fs::FsError::NotFound { .. }) => return Ok(Vec::new()),
        Err(_) => return Err(ERR_STORE_UNAVAILABLE.to_string()),
    }
    machine
        .read_toml::<StackRecordFile>(path)
        .map(|file| file.entries)
        .map_err(|_| ERR_STORE_UNAVAILABLE.to_string())
}

/// Read the record, change it, and write it back whole. A file that cannot be
/// read is not replaced, so no entry of it is lost.
fn update(
    machine: &fs::RootFs,
    change: impl FnOnce(&mut Vec<StackRecordEntry>),
) -> Result<(), String> {
    let _guard = lock();
    let mut entries = read_entries(machine)?;
    change(&mut entries);
    let path = record_path(machine)?;
    machine
        .write_toml_atomic(path, &StackRecordFile { entries })
        .map_err(|_| ERR_STORE_UNAVAILABLE.to_string())
}

/// GTC-FR-OIMF: write `entry`, in place of an earlier entry of its branch.
pub(crate) fn put_entry(machine: &fs::RootFs, entry: StackRecordEntry) -> Result<(), String> {
    update(machine, |entries| {
        entries.retain(|existing| existing.branch != entry.branch);
        entries.push(entry);
    })
}

/// GTC-FR-AEBW: remove the entry of `branch`. An absent entry is not an error.
pub(crate) fn remove_entry(machine: &fs::RootFs, branch: &str) -> Result<(), String> {
    update(machine, |entries| entries.retain(|existing| existing.branch != branch))
}
