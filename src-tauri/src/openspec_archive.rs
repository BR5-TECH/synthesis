//! The OpenSpec archive at a stream merge (`OAR-openspec-archive.md`).
//!
//! When a work stream merges into its base branch, each OpenSpec change that
//! the stream built is archived on the stream branch first, with one commit
//! for each change, so the base branch gets the built code, the updated live
//! specifications, and the archived change in one merge. The application runs
//! `openspec archive`, not an agent, and only for a change whose tasks are all
//! done and whose discussion holds nothing that the author did not decide.
//!
//! `merge_work_stream` is the one caller, under its repository update guard
//! (OAR-FR-SEQU). This module registers no Tauri command, and the step is a
//! part of the operation `Merging <stream name>` of that call, with no
//! operation of its own (OAR-FR-VSSH).
//!
//! OAR-FR-EZNX: every file read and write goes through `crate::fs`, every Git
//! read and write through `crate::git`, and nothing reaches the network.

use std::time::Instant;

use serde::Serialize;

use crate::fs::RootFs;
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};
use crate::openspec::{ChangeLocation, OpenSpec, OpenSpecFailure, Refusal};

mod check;
mod digest;
pub mod notice;
mod notice_record;
mod step;
mod store_move;

pub use check::{pending_kinds, PendingKind};
pub use digest::change_folder_digest;
pub use notice::{give_merge_notice, MergeNotice};
pub use notice_record::{read_notice_record, NoticeRecord, NOTICE_RECORD_FILENAME};
pub use store_move::{move_change_records, store_target};

/// OAR-FR-BEIK: carries `<change>: <kinds>` after a colon.
pub const ERR_CHANGE_HAS_PENDING: &str = "change_has_pending";
/// OAR-FR-QTBK: carries `<change>: <reason>` after a colon.
pub const ERR_CHANGE_NOT_ARCHIVABLE: &str = "change_not_archivable";
/// OAR-FR-EUXZ: carries `<change>: <code>` after a colon.
pub const ERR_ARCHIVE_FAILED: &str = "archive_failed";

/// OAR-FR-QTBK: the reasons a change cannot be archived. A read of
/// `apply_instructions` that fails gives its own code (OFP-FR-GPCO).
pub const REASON_MODE_INACTIVE: &str = crate::openspec::failure::CODE_MODE_INACTIVE;
pub const REASON_CLI_UNAVAILABLE: &str = crate::openspec::failure::CODE_CLI_UNAVAILABLE;
pub const REASON_TASKS_INCOMPLETE: &str = crate::openspec::failure::CODE_TASKS_INCOMPLETE;

/// OAR-FR-EUXZ: the code of an archive whose commit failed.
pub const CODE_COMMIT_FAILED: &str = "commit_failed";

/// OAR-FR-KSZI: the code of a store move that failed.
pub const CODE_STORE_MOVE_FAILED: &str = "store_move_failed";

/// OAR contract surface, fixed text: the archive commit message.
pub fn commit_message(change: &str) -> String {
    format!("Archive OpenSpec change {change}")
}

/// OAR-FR-QTBK, OFP-FR-ZDEE: `change_not_archivable` for `change`, with its
/// reason as the cause.
pub(crate) fn refuse_not_archivable(change: &str, reason: &str) -> Refusal {
    let mut failure = OpenSpecFailure::new(ERR_CHANGE_NOT_ARCHIVABLE).for_change(change);
    failure.cause = Some(reason.to_string());
    failure.into()
}

/// OAR contract surface `ArchivedChange`, in `StreamMergeResult.archived`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedChange {
    pub change: String,
    /// `<YYYY-MM-DD>-<change>` (OAR-FR-XTLB).
    pub archived_as: String,
}

/// What the step works with: the seam, the active worktree, the stream's
/// working copy, and the repository machine store.
pub(crate) struct ArchiveContext<'a, R: tauri::Runtime> {
    pub(crate) app: &'a tauri::AppHandle<R>,
    pub(crate) stream_id: &'a str,
    pub(crate) active: Option<RootFs>,
    pub(crate) at: RootFs,
}

/// OAR-FR-KGDP, OAR-FR-SURB: the changes to archive of a stream merge, in
/// archive order: each change that a completed apply run of the stream names,
/// with no revert on the stream branch and no landing on the base branch, and
/// whose folder `openspec/changes/<change>/` stands in the working copy.
pub fn changes_to_archive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    at: &RootFs,
) -> Vec<String> {
    crate::graduation::stream_archive_candidates(
        app,
        &stream.id,
        &stream.branch,
        &stream.base_branch,
    )
    .into_iter()
    .map(|candidate| candidate.change)
    .filter(|change| crate::openspec::change::valid_change_name(change))
    .filter(|change| {
        at.file_info(at.path().join("openspec/changes").join(change))
            .is_ok_and(|info| info.kind == crate::fs::EntryKind::Dir)
    })
    .collect()
}

/// The code of a WARN record where the CLI wrote a path that the archive
/// commit does not hold (OAR-FR-BLBJ).
pub const CODE_ARCHIVE_WROTE_OUTSIDE: &str = "archive_wrote_outside";

/// OAR-FR-BLBJ, WKS-FR-UZHT: the working copy was clean at the check, and the
/// archive commit holds exactly the paths below its three folders. A path that
/// still differs from the branch head after the commit is a write of the CLI
/// outside them. The commit stays (OAR-FR-OQVF), the path stays as the CLI
/// left it, and the merge is refused with `stream_dirty`, as the clean check
/// refuses a stream with uncommitted work, so that no later archive commits
/// the path and the merge does not go on over it.
fn refuse_writes_outside<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    change: &str,
) -> Result<(), String> {
    let left = crate::streams::uncommitted_paths_of(cx.at.path());
    if left.is_empty() {
        return Ok(());
    }
    logging::log_warn(
        cx.app,
        &BUFFER,
        &[Domain::Backend],
        "openspec archive left paths outside its folders",
        log_fields! {
            "stream_id" => cx.stream_id.to_string(),
            "change" => change.to_string(),
            "code" => CODE_ARCHIVE_WROTE_OUTSIDE,
            "paths" => left.len(),
        },
    );
    Err(format!("{}: {}", crate::streams::ERR_STREAM_DIRTY, left.join(", ")))
}

/// OAR-FR-SEQU: the archive step of a stream merge, in the stream's working
/// copy. It answers the changes it archived, in archive order, or the typed
/// refusal of the merge. A refusal after an archive commit keeps that commit
/// (OAR-FR-OQVF) and carries its own code alone (OAR-FR-SOFQ).
///
/// The caller holds the repository update guard, has checked that both sides
/// are clean, and has found that the stream holds commits that its base
/// branch does not.
pub fn archive_stream_changes<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
) -> Result<Vec<ArchivedChange>, Refusal> {
    use tauri::Manager;
    let started = Instant::now();
    let location = ChangeLocation::Stream { stream_id: stream.id.clone() };
    let at = match crate::openspec::change::working_copy(app, &location) {
        Ok(at) => at,
        // A working copy that cannot be governed holds no change folder this
        // step can see, so it archives nothing and the merge goes on.
        Err(error) => {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive could not open the stream working copy",
                log_fields! { "stream_id" => stream.id.clone(), "code" => error.code() },
            );
            return Ok(Vec::new());
        }
    };
    // OAR-FR-LZLW: no change to archive starts no process and writes nothing.
    let changes = changes_to_archive(app, stream, &at);
    if changes.is_empty() {
        return Ok(Vec::new());
    }
    let active =
        app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok());
    let cx = ArchiveContext { app, stream_id: &stream.id, active, at };
    let seam = app.try_state::<OpenSpec>();

    // OAR-FR-CCIN, OAR-FR-GJVM: every change is checked before any archive.
    check::check_all(&cx, seam.as_deref(), &changes)?;
    let seam = seam.ok_or_else(|| refuse_not_archivable(&changes[0], REASON_CLI_UNAVAILABLE))?;

    // OAR-FR-IAKL: the archives, in archive order. One that fails ends the
    // step; the commits before it stay (OAR-FR-OQVF).
    let mut archived = Vec::new();
    for change in &changes {
        archived.push(step::archive_one(&cx, &seam, change)?);
        refuse_writes_outside(&cx, change)?;
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "openspec archive step finished",
        log_fields! {
            "stream_id" => stream.id.clone(),
            "archived" => archived.len(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(archived)
}
