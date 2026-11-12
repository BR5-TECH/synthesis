//! The check of every change before any archive (`OAR-openspec-archive.md`
//! OAR-FR-CCIN, OAR-FR-GJVM, OAR-FR-BEIK, OAR-FR-GTGQ, OAR-FR-QTBK,
//! OAR-FR-AEOK).
//!
//! The pending items of a change are read from the repository machine store,
//! and its task progress from one `apply_instructions` in the stream's working
//! copy. A check writes nothing.

use std::time::Instant;

use super::*;
use crate::openspec::OpenSpecReadError;

/// OAR contract surface `PendingKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingKind {
    EditProposal,
    NewChangeProposal,
    ActionRequest,
}

impl PendingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PendingKind::EditProposal => "edit_proposal",
            PendingKind::NewChangeProposal => "new_change_proposal",
            PendingKind::ActionRequest => "action_request",
        }
    }
}

/// OAR-FR-BEIK, OAR-FR-GTGQ: the kinds of pending item that the change holds
/// in the store, in the order of the contract. A change edit proposal (a plan
/// proposal included) and a new-change proposal are pending where they are
/// `pending` or owe their commit. An action request is pending where it is
/// `pending`.
pub fn pending_kinds(store: &RootFs, stream_id: &str, change: &str) -> Vec<PendingKind> {
    let mut kinds = Vec::new();
    if crate::change_proposals::list_proposals_impl(store, stream_id, change)
        .iter()
        .any(|p| p.state == crate::change_proposals::ChangeProposalState::Pending || p.commit_owed)
    {
        kinds.push(PendingKind::EditProposal);
    }
    if crate::new_change_proposals::list_proposals_impl(store, stream_id, change).iter().any(|p| {
        p.state == crate::new_change_proposals::NewChangeProposalState::Pending || p.commit_owed
    }) {
        kinds.push(PendingKind::NewChangeProposal);
    }
    if crate::change_action_requests::pending_request(store, stream_id, change).is_some() {
        kinds.push(PendingKind::ActionRequest);
    }
    kinds
}

/// OAR-FR-QTBK: the reason of a read of `apply_instructions` that failed.
fn reason_of(error: &OpenSpecReadError) -> &'static str {
    match error {
        OpenSpecReadError::NoOpenProject | OpenSpecReadError::OpenSpecModeInactive => {
            REASON_MODE_INACTIVE
        }
        OpenSpecReadError::CliUnavailable(_) => REASON_CLI_UNAVAILABLE,
        _ => REASON_PROGRESS_UNAVAILABLE,
    }
}

/// OAR-FR-QTBK, OAR-FR-AEOK: why the change cannot be archived, or `None`
/// where it can. One `apply_instructions` runs, and no other CLI process.
fn not_archivable<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    seam: Option<&OpenSpec>,
    change: &str,
) -> Option<&'static str> {
    if !OpenSpec::mode_active(cx.active.as_ref()) {
        return Some(REASON_MODE_INACTIVE);
    }
    let Some(seam) = seam else {
        return Some(REASON_CLI_UNAVAILABLE);
    };
    match seam.apply_instructions(cx.app, &BUFFER, cx.active.as_ref(), &cx.at, change) {
        Ok(read) if read.state == "all_done" => None,
        Ok(_) => Some(REASON_TASKS_INCOMPLETE),
        Err(error) => Some(reason_of(&error)),
    }
}

/// OAR-FR-CCIN, OAR-FR-GJVM: each change in archive order, its pending items
/// before its archivability. The first refusal refuses the merge, and nothing
/// is written.
pub(super) fn check_all<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    seam: Option<&OpenSpec>,
    changes: &[String],
) -> Result<(), String> {
    use tauri::Manager;
    let store = cx
        .app
        .try_state::<crate::project::ProjectState>()
        .and_then(|project| project.require_store().ok());
    if store.is_none() {
        // No store holds no record, and so no pending item.
        logging::log_warn(
            cx.app,
            &BUFFER,
            &[Domain::Backend],
            "openspec archive found no store to read pending items",
            log_fields! { "stream_id" => cx.stream_id.to_string(), "code" => "store_unavailable" },
        );
    }
    for change in changes {
        let started = Instant::now();
        let kinds =
            store.as_ref().map(|s| pending_kinds(s, cx.stream_id, change)).unwrap_or_default();
        let refusal = if !kinds.is_empty() {
            let names: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
            Some((ERR_CHANGE_HAS_PENDING, names.join(",")))
        } else {
            not_archivable(cx, seam, change)
                .map(|reason| (ERR_CHANGE_NOT_ARCHIVABLE, reason.to_string()))
        };
        let millis = started.elapsed().as_millis() as u64;
        if let Some((code, detail)) = refusal {
            logging::log_warn(
                cx.app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive check refused",
                log_fields! {
                    "stream_id" => cx.stream_id.to_string(),
                    "change" => change.clone(),
                    "code" => code,
                    "reason" => detail.clone(),
                    "durationMs" => millis,
                },
            );
            return Err(format!("{code}: {change}: {detail}"));
        }
        logging::log_info(
            cx.app,
            &BUFFER,
            &[Domain::Backend],
            "openspec archive check passed",
            log_fields! {
                "stream_id" => cx.stream_id.to_string(),
                "change" => change.clone(),
                "durationMs" => millis,
            },
        );
    }
    Ok(())
}
