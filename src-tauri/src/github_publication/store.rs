//! The draft-owned publication store (`DRS-draft-storage.md` DRS-FR-EJBM,
//! DRS-FR-PSCH, DRS-FR-VDQR, DRS-FR-JOEV, DRS-FR-XNLP, DRS-FR-WBTA,
//! DRS-FR-ULKN, DRS-FR-MQBT).
//!
//! One file per draft, `publication.toml`, holding two distinct sections: the
//! append-only successful history and the draft's one standing attempt. The
//! file is in the **repository machine store** under the draft's stable id
//! (RMS-FR-MVDU), so every worktree of the repository reads the same history.
//! It is **absent until the first attempt**, so a draft nobody published
//! carries nothing at all.
//!
//! An earlier build kept the file inside the draft's own directory. That
//! **legacy store** is read where the machine store holds no file for the
//! draft, and is never written (DRS-FR-ULKN).

use super::records::*;
use crate::fs;

/// The one lock every read-modify-write of a publication store takes.
///
/// The store is one file that every worktree of the repository shares, and a
/// change is a read of the whole file followed by an atomic write of it. Two
/// changes that overlap in this process — a quick second click, the import
/// pass running beside a publication — would otherwise let the slower one
/// erase what the faster one wrote (GHP-FR-KZAP). It is held only while the
/// file is read and written, never across a GitHub request. It is not a lock
/// between processes (RMS-FR-PFOB).
static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take [`STORE_LOCK`]. A panic while it was held leaves no half-written
/// file, every write being atomic, so a poisoned lock is still usable.
pub(crate) fn lock_stores() -> std::sync::MutexGuard<'static, ()> {
    STORE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// DRS-FR-PSCH: read the store, change it, and write it back whole, under
/// [`STORE_LOCK`]. A change that returns an error writes nothing.
pub fn update_store<T>(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    change: impl FnOnce(&mut PublicationStore) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = lock_stores();
    let mut store = read_store(root, machine, draft_id)?;
    let out = change(&mut store)?;
    write_store(root, machine, draft_id, &store)?;
    Ok(out)
}

/// RMS-FR-MVDU: the store file of one draft, relative to the machine store.
///
/// The id is checked before a path is composed, so a crafted id cannot name a
/// file outside the draft's own folder of the store.
fn machine_rel(draft_id: &str) -> Result<String, String> {
    if !crate::drafts::is_valid_draft_id(draft_id) {
        return Err(ERR_DRAFT_NOT_FOUND.to_string());
    }
    Ok(format!(
        "{}/{draft_id}/{}",
        crate::repository_store::DRAFTS_DIRNAME,
        crate::drafts::PUBLICATION_FILE
    ))
}

/// [`machine_rel`] as a path, through the escape gate.
fn machine_path(machine: &fs::RootFs, draft_id: &str) -> Result<std::path::PathBuf, String> {
    fs::resolve_under(machine.path(), machine_rel(draft_id)?)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())
}

/// DRS-FR-PSCH: the store as it now stands on disk.
///
/// The draft is resolved in the active worktree first (DRS-FR-EJBM). The
/// machine store's file is read where it exists, else the legacy store, else
/// an empty history with no attempt. A file that no longer parses — a Git
/// conflict left in a legacy file, a half-written value — is a typed error
/// against that draft alone and is neither repaired nor rewritten
/// (DRS-FR-GWNI). A machine file that does not parse is never answered from the
/// legacy one: that would show a history the next write then replaces.
pub fn read_store(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
) -> Result<PublicationStore, String> {
    let legacy = crate::drafts::draft_publication_path(root, draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    read_store_at(root, machine, draft_id, &legacy)
}

/// [`read_store`] for a caller that already knows where the draft's legacy
/// store would sit — the drafts listing, which would otherwise pay for a walk
/// of the drafts root per draft it lists (DRS-FR-PXHW).
pub fn read_store_at(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    legacy: &std::path::Path,
) -> Result<PublicationStore, String> {
    let path = machine_path(machine, draft_id)?;
    match machine.file_info(&path) {
        Ok(_) => {
            return machine
                .read_toml::<PublicationStore>(path)
                .map_err(|_| ERR_STORE_WRITE_FAILED.to_string());
        }
        Err(fs::FsError::NotFound { .. }) => {}
        Err(_) => return Err(ERR_STORE_WRITE_FAILED.to_string()),
    }
    // DRS-FR-ULKN: the legacy store answers only where the machine store has no
    // file for the draft.
    if root.file_info(legacy).is_err() {
        return Ok(PublicationStore::default());
    }
    root.read_toml::<PublicationStore>(legacy).map_err(|_| ERR_STORE_WRITE_FAILED.to_string())
}

/// DRS-FR-PSCH: replace the whole file **atomically**, in the machine store.
///
/// Every state change of an attempt and every appended record goes through one
/// call of this, so a reader never observes a store with a record appended and
/// its attempt still standing. The legacy store is never written: the first
/// write after a legacy read carries its records here (DRS-FR-ULKN).
pub fn write_store(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    store: &PublicationStore,
) -> Result<(), String> {
    // DRS-FR-EJBM: an id that names no draft in the active worktree reaches no
    // store.
    crate::drafts::draft_dir(root, draft_id).map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    let path = machine_path(machine, draft_id)?;
    machine.write_toml_atomic(path, store).map_err(|_| ERR_STORE_WRITE_FAILED.to_string())
}

/// DRS-FR-MQBT: remove one draft's store from the machine store.
///
/// Scoped to that one stable id, and a store already absent is an already
/// completed deletion, so a repeated draft deletion completes what is left.
pub fn delete_store(machine: &fs::RootFs, draft_id: &str) -> Result<(), String> {
    match machine.delete_under(machine.path(), machine_rel(draft_id)?, false) {
        Ok(()) | Err(fs::FsError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// GHP-FR-JAWD: append one record and clear the attempt, in **one** atomic
/// write.
///
/// The attempt is re-read and its marker checked before it is cleared, so a
/// completion never clears an attempt that is not the one it belongs to. That
/// matters because the store is a read-modify-write of a file: two publications
/// racing for one draft would otherwise let the slower one erase the faster
/// one's standing attempt (GHP-FR-KZAP).
pub fn complete_attempt(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    record: PublicationRecord,
) -> Result<PublicationRecord, String> {
    update_store(root, machine, draft_id, |store| {
        match store.attempt.as_ref() {
            Some(attempt) if attempt.marker == record.marker => {}
            // The attempt this record belongs to is gone or has been replaced:
            // the issue exists, but recording it here would clear somebody
            // else's attempt. Report rather than write.
            _ => return Err(ERR_ATTEMPT_IN_PROGRESS.to_string()),
        }
        store.publication.push(record.clone());
        store.attempt = None;
        Ok(record)
    })
}

/// DRS-FR-XNLP: the newest record is the draft's **current** publication.
pub fn current_of(store: &PublicationStore) -> Option<PublicationRecord> {
    store.publication.last().cloned()
}

/// GHP-FR-CWTG: the history newest first, which is the order both surfaces
/// render. The stored order is oldest first (DRS-FR-EJBM) and is not disturbed.
pub fn history_newest_first(store: &PublicationStore) -> Vec<PublicationRecord> {
    let mut history = store.publication.clone();
    history.reverse();
    history
}

/// GHP-FR-WUJQ: append the record of an update, in one atomic write.
///
/// The store is re-read first. An attempt that started meanwhile, or a record
/// appended meanwhile, means `before` is no longer the current record, and the
/// record of the update would then name an issue that is not the current one.
/// Both are reported rather than written (GHP-FR-MZMW).
pub fn append_update(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    before: &PublicationRecord,
    record: PublicationRecord,
) -> Result<PublicationRecord, String> {
    update_store(root, machine, draft_id, |store| {
        // GHP-FR-BCAV: GitHub has already accepted the edit here, so each
        // refusal is the failed store write of an accepted edit.
        if store.attempt.is_some() || store.publication.last() != Some(before) {
            return Err(ERR_STORE_WRITE_FAILED.to_string());
        }
        store.publication.push(record.clone());
        Ok(record)
    })
}
