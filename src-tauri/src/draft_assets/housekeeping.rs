//! One housekeeping pass over one draft.

use super::*;

// ---------------------------------------------------------------------------
// Housekeeping (DAS-FR-12 – DAS-FR-21, DAS-FR-28)
// ---------------------------------------------------------------------------

/// DAS-FR-12: one housekeeping pass over one draft.
///
/// It reads the draft's **saved** prompt, resolves every reference in it, and
/// then deletes a file under that draft's `assets/` only where **all** of these
/// hold together: the file lies directly inside that folder; it is an ordinary
/// file rather than a directory or a symbolic link; its extension and its
/// content are one of the image kinds DAS-FR-03 accepts; it is not held; and no
/// resolved reference in the saved prompt names it. A file failing any one of
/// them is retained.
///
/// `locked` is DAS-FR-25's answer for this draft — a non-terminal graduation run
/// holds it, or it has graduated. A sweep against either **deletes nothing**:
/// the prompt cannot change, so there is nothing to collect, and a run in flight
/// is reading the material. Passed in rather than read here because the lock
/// lives in a queue this module has no business reaching into.
pub fn sweep_draft_impl(
    root: &fs::RootFs,
    draft_id: &str,
    locked: bool,
    // DAS-FR-14: asked **per candidate and again immediately before the
    // delete**, rather than snapshotted once for the pass. A snapshot taken
    // when the pass began says nothing about an image the author pasted while
    // it was running, and a pass that decided on one would delete the picture
    // seconds after they put it there — which is the one thing the hold exists
    // to prevent.
    held: &dyn Fn(&str) -> bool,
    cancelled: &AtomicBool,
) -> DraftAssetSweep {
    let mut sweep = DraftAssetSweep { complete: true, ..Default::default() };
    let Ok(dir) = crate::drafts::draft_assets_dir(root, draft_id) else {
        // DAS-FR-21: a draft that resolved to nothing ends the pass with
        // `complete` false and leaves everything else exactly as it stands.
        sweep.complete = false;
        return sweep;
    };
    let entries = match root.list_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => {
            // A folder that cannot be listed decided nothing, so it deletes
            // nothing at all (DAS-FR-28). A draft whose `assets/` has never been
            // written is the same shape and is equally not a licence to delete.
            sweep.complete = false;
            return sweep;
        }
    };

    // DAS-FR-13 / DAS-FR-16: a prompt this module cannot read, or one belonging
    // to an inconsistent draft, protects everything — the pass deletes nothing
    // at all, because a set of references it could not read is not evidence that
    // an asset is unreferenced.
    let protected = crate::drafts::require_prompt(root, draft_id)
        .and_then(|prompt| crate::drafts::load_draft_file_impl(root, draft_id, &prompt))
        .map(|contents| read_references(&contents.body).protected);
    let protected = match protected {
        Ok(protected) if !locked => protected,
        // DAS-FR-25: a locked or graduated draft's prompt cannot change, so
        // there is nothing to collect. Every file is retained, and the pass is
        // complete rather than failed — it decided every file it scanned.
        Ok(_) => {
            sweep.scanned = entries.len() as u32;
            sweep.retained = sweep.scanned;
            return sweep;
        }
        Err(_) => {
            sweep.scanned = entries.len() as u32;
            sweep.retained = sweep.scanned;
            sweep.complete = false;
            return sweep;
        }
    };

    for entry in entries {
        // DAS-FR-20: a pass abandoned at shutdown leaves every asset it had not
        // yet decided untouched and reports itself incomplete, rather than
        // reporting success for work it did not do.
        if cancelled.load(Ordering::SeqCst) {
            sweep.complete = false;
            return sweep;
        }
        sweep.scanned += 1;
        if !removable(root, &dir, &entry, &protected, held) {
            sweep.retained += 1;
            continue;
        }
        // DAS-FR-14 / DAS-FR-15: both conditions that can change under a
        // running pass are re-checked immediately before the delete — the
        // saved prompt may have gained a reference, and a store may have taken
        // a hold. Re-read and re-asked rather than remembered, because the
        // whole point is that either may have moved on since the pass began.
        if !held(&entry.name) && still_unreferenced(root, draft_id, &entry.name) {
            match root.delete_under(&dir, &entry.name, false) {
                Ok(()) => {
                    sweep.removed += 1;
                }
                // DAS-FR-28: a cleanup failure is per file and stops nothing.
                // The candidate is counted failed, left exactly as it was, and
                // the pass carries on with the candidates it has left.
                Err(_) => {
                    sweep.failed += 1;
                    sweep.complete = false;
                }
            }
        } else {
            sweep.retained += 1;
        }
    }
    sweep
}

/// DAS-FR-12: whether every condition for deleting this entry holds together.
pub(super) fn removable(
    root: &fs::RootFs,
    dir: &Path,
    entry: &fs::DirEntry,
    protected: &BTreeSet<String>,
    held: &dyn Fn(&str) -> bool,
) -> bool {
    // An ordinary file rather than a directory or a symbolic link. A link's
    // target is neither read nor removed (DAS-FR-16, DAS-FR-12, DAS-FR-18).
    if entry.kind != fs::EntryKind::File {
        return false;
    }
    // DAS-FR-14: an asset this session stored whose insertion has not settled is
    // skipped by every sweep, so a pass that runs in the instant between a store
    // and the write carrying its reference deletes nothing.
    if held(&entry.name) {
        return false;
    }
    // Its extension **and** its content are one of the image kinds DAS-FR-03
    // accepts. A temporary name an interrupted store left behind fails the first
    // test and is retained (DAS-FR-04); a `.png` whose bytes are not an image
    // fails the second and is retained too.
    let Some(kind) = entry.name.rsplit_once('.').and_then(|(_, e)| kind_of_extension(e)) else {
        return false;
    };
    if protected.contains(&entry.name) {
        return false;
    }
    let Ok(bytes) = root.read_bytes(dir.join(&entry.name)) else {
        // A file this module cannot read is a file it knows nothing about, and
        // knowing nothing is never grounds for deleting anything (DAS-FR-16).
        return false;
    };
    bytes_are(kind, &bytes)
}

/// DAS-FR-15: re-read the saved prompt and ask again whether this asset is
/// referenced, immediately before the delete.
///
/// A prompt that has become unreadable since the pass began answers "referenced"
/// — the safe direction, and the same direction DAS-FR-16 takes everywhere else.
pub(super) fn still_unreferenced(root: &fs::RootFs, draft_id: &str, name: &str) -> bool {
    let Ok(prompt) = crate::drafts::require_prompt(root, draft_id) else {
        return false;
    };
    let Ok(contents) = crate::drafts::load_draft_file_impl(root, draft_id, &prompt) else {
        return false;
    };
    !read_references(&contents.body).protected.contains(name)
}
