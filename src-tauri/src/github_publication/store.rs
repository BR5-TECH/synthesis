//! The draft-owned publication store (`DRS-draft-storage.md` DRS-FR-EJBM,
//! DRS-FR-PSCH, DRS-FR-VDQR, DRS-FR-JOEV, DRS-FR-XNLP, DRS-FR-WBTA).
//!
//! One file per draft, `publication.toml`, holding two distinct sections: the
//! append-only successful history and the draft's one standing attempt. The
//! file is **absent until the first attempt**, so a draft nobody published
//! carries nothing at all.

use super::records::*;
use crate::fs;

/// DRS-FR-PSCH: the store as it now stands on disk.
///
/// An absent file reads as an empty history with no attempt. A file that no
/// longer parses — a Git conflict left in it, a half-written value — is a typed
/// error against that draft alone and is neither repaired nor rewritten
/// (DRS-FR-GWNI).
pub fn read_store(root: &fs::RootFs, draft_id: &str) -> Result<PublicationStore, String> {
    let path = crate::drafts::draft_publication_path(root, draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    // DRS-FR-PSCH: absent until the first attempt.
    if root.file_info(&path).is_err() {
        return Ok(PublicationStore::default());
    }
    root.read_toml::<PublicationStore>(path).map_err(|_| ERR_STORE_WRITE_FAILED.to_string())
}

/// DRS-FR-PSCH: replace the whole file **atomically**.
///
/// Every state change of an attempt and every appended record goes through one
/// call of this, so a reader never observes a store with a record appended and
/// its attempt still standing.
pub fn write_store(
    root: &fs::RootFs,
    draft_id: &str,
    store: &PublicationStore,
) -> Result<(), String> {
    let path = crate::drafts::draft_publication_path(root, draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    root.write_toml_atomic(path, store).map_err(|_| ERR_STORE_WRITE_FAILED.to_string())?;
    // DRS-FR-WBTA / DRS-FR-JDRY: the store is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    crate::drafts::git_storage::ensure_private_ignored(root);
    Ok(())
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
    draft_id: &str,
    record: PublicationRecord,
) -> Result<PublicationRecord, String> {
    let mut store = read_store(root, draft_id)?;
    match store.attempt.as_ref() {
        Some(attempt) if attempt.marker == record.marker => {}
        // The attempt this record belongs to is gone or has been replaced: the
        // issue exists, but recording it here would clear somebody else's
        // attempt. Report rather than write.
        _ => return Err(ERR_ATTEMPT_IN_PROGRESS.to_string()),
    }
    store.publication.push(record.clone());
    store.attempt = None;
    write_store(root, draft_id, &store)?;
    Ok(record)
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
