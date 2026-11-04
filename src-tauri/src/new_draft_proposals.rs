//! New-draft proposals of a decomposition (`DCP-draft-change-proposals.md`,
//! the "New-draft proposals" section).
//!
//! An agent proposes, one at a time, the new drafts that replace a draft that
//! grew too big (`crate::tools::propose_new_draft`). Each proposal is a record
//! and a body beside the original draft. Nothing is created until the author
//! accepts a proposal, and accepting one creates exactly one draft in the
//! original draft's folder. Only the author ends the decomposition: the
//! confirmation archives the original draft and writes its `superseded`
//! marker (DCP-FR-VLLP).
//!
//! On-disk layout, inside the draft's own folder (DCP-FR-VQWK):
//!
//! ```text
//! <draft>/proposals/new-drafts/
//!   <proposal-id>.toml   — the record
//!   <proposal-id>.md     — the proposed body
//! ```
//!
//! The folder sits inside `proposals/`, so it is private draft storage and is
//! deleted with the draft (DCP-FR-02). It is a folder of its own, so a listing
//! of draft change proposals never reads a new-draft record.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Emitter;

use crate::drafts;
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

mod commands;
mod confirming;
mod deciding;
mod guards;
mod model;
mod recording;

pub use commands::*;
pub use confirming::*;
pub use deciding::*;
pub use guards::*;
pub use model::*;
pub use recording::*;

#[cfg(test)]
mod tests;

/// DCP-FR-VQWK: the folder inside `proposals/`.
pub const NEW_DRAFTS_DIR: &str = "new-drafts";
const RECORD_EXT: &str = "toml";
const BODY_EXT: &str = "md";

// ---------------------------------------------------------------------------
// Typed errors (DCP contract surface)
// ---------------------------------------------------------------------------

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
pub const ERR_DRAFT_ARCHIVED: &str = "draft_archived";
pub const ERR_DRAFT_GRADUATED: &str = drafts::ERR_DRAFT_GRADUATED;
pub const ERR_DRAFT_LOCKED: &str = "draft_locked_by_graduation";
pub const ERR_DRAFT_GITHUB_SHADOW: &str = drafts::ERR_GITHUB_SHADOW;
pub const ERR_DECOMPOSITION_CONFIRMED: &str = drafts::ERR_ALREADY_SUPERSEDED;
pub const ERR_PROPOSAL_PENDING: &str = "new_draft_proposal_pending";
pub const ERR_NOTHING_ACCEPTED: &str = "nothing_accepted";
pub const ERR_DISCUSSION_RESOLVED: &str = crate::comments::ERR_DISCUSSION_RESOLVED;
pub const ERR_WRITE_FAILED: &str = "write_failed";
/// DCP-FR-OBAW: the validation errors of a recording and of an update.
pub const ERR_BLANK_NAME: &str = "blank_name";
pub const ERR_BLANK_BODY: &str = "blank_body";
pub const ERR_BAD_POSITION: &str = "bad_position";
/// DCP-FR-ZJRW.
pub const ERR_NO_OPEN_PROPOSAL: &str = "no_open_proposal";
pub const ERR_NO_CHANGE: &str = "no_change";

// ---------------------------------------------------------------------------
// Paths and reads
// ---------------------------------------------------------------------------

/// A proposal id is joined into a path, on the terms of a draft change
/// proposal id (`crate::draft_proposals::is_valid_proposal_id`).
fn is_valid_id(id: &str) -> bool {
    crate::draft_proposals::is_valid_proposal_id(id)
}

/// DCP-FR-VQWK: the draft's `proposals/new-drafts/` folder.
fn dir_of(root: &fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    Ok(drafts::draft_proposals_dir(root, draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?
        .join(NEW_DRAFTS_DIR))
}

fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{RECORD_EXT}"))
}

fn body_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{BODY_EXT}"))
}

/// Every record in one folder, in no order. A record that does not parse is
/// skipped, as a damaged change proposal is.
fn records_in(root: &fs::RootFs, dir: &Path) -> Vec<NewDraftProposal> {
    let Ok(entries) = root.list_dir(dir) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter_map(|entry| {
            let id = entry.name.strip_suffix(&format!(".{RECORD_EXT}"))?;
            if !is_valid_id(id) {
                return None;
            }
            root.read_toml::<NewDraftProposal>(&record_path(dir, id)).ok()
        })
        .collect()
}

/// DCP-FR-ZDSP: every new-draft proposal of the draft, most recently created
/// first. It reads records alone and never a body.
pub fn list_impl(root: &fs::RootFs, draft_id: &str) -> Result<Vec<NewDraftProposal>, String> {
    let dir = dir_of(root, draft_id)?;
    let mut all = records_in(root, &dir);
    all.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    Ok(all)
}

/// DCP-FR-OBAW: the draft's pending new-draft proposal, if it has one.
pub fn pending_for(root: &fs::RootFs, draft_id: &str) -> Option<NewDraftProposal> {
    list_impl(root, draft_id).ok()?.into_iter().find(|p| p.state == NewDraftState::Pending)
}

/// DCP-FR-BZHR / DRS-FR-WMIK: whether the draft holds a pending new-draft
/// proposal, for a caller that holds the draft's own directory already.
pub fn has_pending_at(root: &fs::RootFs, draft_dir: &Path) -> bool {
    let dir = draft_dir.join(drafts::PROPOSALS_DIR).join(NEW_DRAFTS_DIR);
    records_in(root, &dir).iter().any(|p| p.state == NewDraftState::Pending)
}

/// The draft and the record of one proposal, found by its id alone.
pub fn find(root: &fs::RootFs, proposal_id: &str) -> Option<NewDraftProposal> {
    if !is_valid_id(proposal_id) {
        return None;
    }
    drafts::list_drafts_impl(root).drafts.into_iter().find_map(|draft| {
        let dir = dir_of(root, &draft.id).ok()?;
        root.read_toml::<NewDraftProposal>(&record_path(&dir, proposal_id)).ok()
    })
}

/// DCP contract surface: the proposed body as it stands.
pub fn load_body_impl(root: &fs::RootFs, proposal_id: &str) -> Result<String, String> {
    let record = find(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let dir = dir_of(root, &record.draft_id)?;
    root.read_text(body_path(&dir, proposal_id)).map_err(|_| ERR_PROPOSAL_NOT_FOUND.into())
}

/// DCP-FR-FKMT: every new-draft proposal of the draft, oldest first, with the
/// body of the pending one, for a turn that is assembling its input. It
/// writes nothing.
pub fn read_new_draft_proposals(
    root: &fs::RootFs,
    draft_id: &str,
) -> Option<(Vec<NewDraftProposal>, Option<String>)> {
    let mut all = list_impl(root, draft_id).ok()?;
    if all.is_empty() {
        return None;
    }
    all.reverse();
    let body = all
        .iter()
        .find(|p| p.state == NewDraftState::Pending)
        .and_then(|p| load_body_impl(root, &p.id).ok());
    Some((all, body))
}

// ---------------------------------------------------------------------------
// The event (DCP-FR-ZDSP) and the log (DCP-FR-EEGX)
// ---------------------------------------------------------------------------

/// DCP-FR-ZDSP: kebab-case, as every event of this application is.
pub const NEW_DRAFT_PROPOSALS_CHANGED: &str = "new-draft-proposals-changed";

/// DCP-FR-ZDSP: which of the four things the event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NewDraftChangeCause {
    Recorded,
    Updated,
    Decided,
    Confirmed,
}

impl NewDraftChangeCause {
    fn as_str(self) -> &'static str {
        match self {
            NewDraftChangeCause::Recorded => "recorded",
            NewDraftChangeCause::Updated => "updated",
            NewDraftChangeCause::Decided => "decided",
            NewDraftChangeCause::Confirmed => "confirmed",
        }
    }
}

/// DCP contract surface: `NewDraftProposalsChanged`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDraftProposalsChanged {
    pub draft_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<NewDraftProposal>,
    pub cause: NewDraftChangeCause,
}

/// DCP-FR-ZDSP / DCP-FR-EEGX: emit the event and write one `INFO` record. The
/// record names ids, numbers, and states, and never a name, a body, a
/// rationale, or a feedback.
pub(crate) fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
    proposal: Option<&NewDraftProposal>,
    cause: NewDraftChangeCause,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _ = app.emit(
        NEW_DRAFT_PROPOSALS_CHANGED,
        NewDraftProposalsChanged {
            draft_id: draft_id.to_string(),
            proposal: proposal.cloned(),
            cause,
        },
    );
    let mut fields = log_fields! { "draftId" => draft_id, "cause" => cause.as_str() };
    if let Some(p) = proposal {
        fields.insert("proposalId".into(), serde_json::json!(p.id));
        fields.insert("setId".into(), serde_json::json!(p.set_id));
        fields.insert("position".into(), serde_json::json!(p.position));
        fields.insert("total".into(), serde_json::json!(p.total));
        fields.insert("state".into(), serde_json::json!(p.state.as_str()));
    }
    logging::log_info(app, &BUFFER, &[Domain::Backend], "new draft proposal changed", fields);
}

/// DCP-FR-EEGX: the typed code of a refusal, or `write_failed` for an untyped
/// error. An untyped error can carry a path, so it never reaches a record.
pub(crate) fn typed_reason(reason: &str) -> &'static str {
    const KNOWN: [&str; 17] = [
        ERR_NO_PROJECT_OPEN,
        ERR_DRAFT_NOT_FOUND,
        ERR_PROPOSAL_NOT_FOUND,
        ERR_ALREADY_DECIDED,
        ERR_DRAFT_ARCHIVED,
        ERR_DRAFT_GRADUATED,
        ERR_DRAFT_LOCKED,
        ERR_DRAFT_GITHUB_SHADOW,
        ERR_DECOMPOSITION_CONFIRMED,
        ERR_PROPOSAL_PENDING,
        ERR_NOTHING_ACCEPTED,
        ERR_DISCUSSION_RESOLVED,
        ERR_BLANK_NAME,
        ERR_BLANK_BODY,
        ERR_BAD_POSITION,
        ERR_NO_OPEN_PROPOSAL,
        ERR_NO_CHANGE,
    ];
    KNOWN.into_iter().find(|known| *known == reason).unwrap_or(ERR_WRITE_FAILED)
}

/// DCP-FR-EEGX: one `WARN` naming what refused and why.
pub(crate) fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    draft_id: &str,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "new draft proposal operation refused",
        log_fields! {
            "operation" => operation,
            "draftId" => draft_id,
            "reason" => typed_reason(reason),
        },
    );
}
