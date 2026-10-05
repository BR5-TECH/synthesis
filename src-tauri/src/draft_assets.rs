//! Draft assets — the images an author puts inside a draft's prompt.
//! See `specifications/core/DAS-draft-assets.md`.
//!
//! The bytes sit in the draft's own `assets/` folder beside the `files/` folder
//! that holds the prompt (`DRS-draft-storage.md` DRS-FR-01), so an image travels
//! with the draft, is committed with it (DRS-FR-04), and is deleted with it. This module owns that folder, the store and the read
//! of one asset, the reference semantics by which a destination in the prompt
//! resolves to an asset, the housekeeping that removes an asset no saved
//! reference names any more, and the read path that hands a conversation the
//! bytes of the images a prompt carries.
//!
//! Two rules decide everything here:
//!
//! - **A reference is read exactly as the renderer reads it** (DAS-FR-06) —
//!   inline images and reference-style images alike, defined before or after
//!   their use — so an author who edits the Markdown by hand is understood
//!   rather than punished. That is why the reading is a real CommonMark parse
//!   rather than a scan.
//! - **Housekeeping refuses to guess** (DAS-FR-16). It deletes an asset only
//!   where the asset is clearly this draft's own and clearly unreferenced, and
//!   it leaves everything else exactly as it stands. Every uncertainty — an
//!   unreadable prompt, a symbolic link, an unrecognised file, a destination
//!   that leaves the draft — is a reason to retain rather than a reason to
//!   delete.
//!
//! Nothing here contacts the network: no destination is fetched, no remote
//! address is resolved, and an external image reference is metadata about the
//! prompt rather than something to retrieve.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::fs;
use crate::log_fields;
use crate::logging::{log_debug, log_info, log_warn, Domain, BUFFER};
use crate::project::ProjectState;

mod housekeeping;
mod kinds;
mod prompt_images;
mod records;
mod references;
mod scheduling;
mod storage;
mod sweeper;

pub use housekeeping::*;
pub use kinds::*;
pub use prompt_images::*;
pub use records::*;
pub use references::*;
pub use scheduling::*;
pub use storage::*;
pub use sweeper::*;

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// DAS-FR-02 / DAS-FR-03 / DAS-FR-05: store a pasted or dropped image and answer
/// with the destination the surface inserts.
#[tauri::command]
pub fn store_draft_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    media_type: String,
    filename: Option<String>,
    data: String,
    project: State<'_, ProjectState>,
    sweeper: State<'_, DraftAssetSweeper>,
) -> Result<DraftAsset, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    // DAS-FR-25: the graduation lock reaches this module exactly as it reaches
    // the prompt — refused while a non-terminal run holds the draft, and
    // permanently against a graduated one, whose whole content is retained as
    // the record of what a published specification was written from.
    // DRS-FR-QPSC: a GitHub-shadow draft refuses with its own reason first.
    crate::drafts::require_not_github_shadow(&root, &draft_id)?;
    crate::graduation::require_unlocked_draft(&app, &draft_id)?;
    // DAS-FR-14: held from the moment the identifier is minted — before the
    // bytes reach the folder — until the insertion that asked for it settles,
    // so no pass can ever see the file without also seeing the hold. The name
    // is remembered here as well, because a store that then fails leaves a hold
    // on a file that will never exist and this is the only place that knows it.
    let attempted: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let hold = |name: &str| {
        sweeper.hold(&draft_id, name);
        if let Ok(mut slot) = attempted.lock() {
            *slot = Some(name.to_string());
        }
    };
    let result = store_image_impl(
        &root,
        &draft_id,
        &media_type,
        filename.as_deref(),
        &data,
        &hold,
    );
    match &result {
        Ok(asset) => {
            log_info(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "draft image stored",
                log_fields! {
                    "draftId" => draft_id,
                    "mediaType" => asset.media_type,
                    "bytes" => asset.bytes,
                },
            );
        }
        Err(error) => {
            // A store that did not complete leaves no file, so the hold it took
            // names nothing. Released rather than left to expire, so a session
            // that refuses many images does not accumulate names for assets
            // that never existed. Nothing is protected by dropping it: the file
            // is not there, and DAS-FR-04 promises the folder holds no entry
            // for a store that failed.
            if let Some(name) = attempted.lock().ok().and_then(|slot| slot.clone()) {
                sweeper.release(&draft_id, &name);
            }
            log_warn(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "draft image refused",
                // The typed value and the media type alone. The image's own
                // bytes, its supplied filename, and the prompt around it are
                // the author's content and reach no record here.
                log_fields! {
                    "draftId" => draft_id,
                    "error" => error,
                    "mediaType" => media_base(&media_type),
                },
            );
        }
    }
    result
}

/// DAS-FR-08: the stored bytes of an image the prompt references, for a surface
/// to draw.
#[tauri::command]
pub fn read_draft_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    path: String,
    project: State<'_, ProjectState>,
) -> Result<DraftImageContent, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let result = read_image_impl(&root, &draft_id, &path);
    if let Err(error) = &result {
        log_debug(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "draft image not served",
            // The typed value alone: the destination the caller named is the
            // author's own text, and a path that left the draft is precisely
            // what must not reach a record (DAS-FR-27).
            log_fields! { "draftId" => draft_id, "error" => error },
        );
    }
    result
}

/// DAS-FR-09: take back a store whose reference never reached the saved prompt.
#[tauri::command]
pub fn discard_draft_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    path: String,
    project: State<'_, ProjectState>,
    sweeper: State<'_, DraftAssetSweeper>,
) -> Result<DraftAssetDiscarded, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses with its own reason first.
    crate::drafts::require_not_github_shadow(&root, &draft_id)?;
    crate::graduation::require_unlocked_draft(&app, &draft_id)?;
    let result = discard_image_impl(&root, &draft_id, &path, &sweeper);
    match &result {
        Ok(outcome) => log_debug(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "draft image discarded",
            log_fields! {
                "draftId" => draft_id,
                "discarded" => outcome.discarded,
                "retained" => outcome.retained,
            },
        ),
        Err(error) => log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "draft image discard refused",
            log_fields! { "draftId" => draft_id, "error" => error },
        ),
    }
    result
}

/// DAS-FR-15: **schedule** one housekeeping pass over that draft and answer at
/// once, before the pass has read the prompt, listed the folder, or deleted
/// anything.
///
/// The return contract is an acknowledgement and not a result. A caller
/// therefore learns that housekeeping was asked for and never what it found: no
/// command return value, no event, and no payload anywhere carries a
/// [`DraftAssetSweep`], which reaches the internal caller and the log alone.
#[tauri::command]
pub fn sweep_draft_assets<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    project: State<'_, ProjectState>,
    sweeper: State<'_, DraftAssetSweeper>,
) -> Result<DraftAssetSweepScheduled, String> {
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    // DAS-FR-25: a sweep against a locked draft, or one whose `graduated` status
    // stands, deletes nothing. Resolved here, where the queue is reachable,
    // rather than inside the pass, and taken from the one refusal the draft's
    // own writes are taken from (DRS-FR-19, DRS-FR-KQTW) so the sweep cannot
    // collect from a draft the author is refused a write against.
    let locked = crate::graduation::require_unlocked_draft(&app, &draft_id).is_err();
    Ok(schedule_draft_sweep(&app, &sweeper, root, &draft_id, locked))
}

#[cfg(test)]
mod tests;
