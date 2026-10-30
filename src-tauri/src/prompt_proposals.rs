//! Prompt change proposals (`PCP-prompt-change-proposals.md`).
//!
//! A **proposal** is a set of anchored changes ("hunks") to one file the
//! project already holds and whose resolved artifact type is `prompt` or `doc`,
//! offered by an agent in a conversation about that file
//! (`crate::tools::propose_prompt_changes`) and held beside the project until the
//! author decides each hunk. It exists because a collaborator that can see how
//! a prompt would be better should be able to show the author what it would
//! write rather than describe it — and because an agent that rewrites a
//! published file without being asked is not a collaborator.
//!
//! On-disk layout, in the active worktree (PCP-FR-01):
//!
//! ```text
//! .synthesis/proposals/
//!   <proposal-id>.toml      — the record, with the per-hunk ledger (PCP-FR-HKLD)
//!   <proposal-id>.hunks     — the hunk documents and their anchors (PCP-FR-CNVT)
//!   <proposal-id>.content   — a legacy whole candidate (PCP-FR-LGXQ), never written now
//!   <proposal-id>.prior     — the artifact's bytes before an acceptance
//!   <proposal-id>.journal   — the standing acceptance, when one stands
//! ```
//!
//! The record and the hunks are separate files because the hunks are a
//! document and a record is not: listing is what a tab does on open, and it
//! reads records alone.
//!
//! ## The acceptance transaction
//!
//! Each hunk is accepted on its own (PCP-FR-ZQAH). An acceptance has **one
//! commit point**, and it is the artifact write landing. The order is the whole
//! of the guarantee (PCP-FR-14):
//!
//! ```text
//!   pre-checks             → nothing written
//!   comment id + event id  → minted in advance
//!   <id>.prior             → the artifact's bytes as they stood
//!   <id>.journal           → the hunk, its prior row, and both checksums
//!   record: row = accepted → comment_owed = true when the decision resolves
//!   artifact write         ← THE COMMIT POINT (atomic, and the last fallible step)
//!   event                  → emitted after the commit point and never before
//!   decision comment       → only when the decision resolves (PCP-FR-RSLV)
//!   comment_owed = false; remove .prior; remove journal LAST
//! ```
//!
//! Because the artifact write is the last fallible step and is atomic
//! (FSA-FR-04), **the artifact is never restored on any path through this
//! module**: a failure before it means the file was never written, and a failure
//! of it means the write did not land.
//!
//! ## What this module does not do
//!
//! It does not write an artifact itself. An acceptance hands the new text to
//! `crate::artifacts`' own save path (PST-FR-09), which stays the only write
//! path for an artifact's file.
//!
//! It also decides nothing on its own (PCP-FR-20). There is no timer, no queue,
//! and no threshold anywhere in it.
//!
//! It shares nothing with `crate::draft_proposals` (PCP-FR-29): different
//! folder, different commands, different event, different attachment kind, and a
//! different acceptance. It copies that module's ledger patterns and calls only
//! the neutral text module, `crate::text_hunks`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::artifacts::ContentTracker;
use crate::comments::{self, Attachment, Participant, ThreadRef};
use crate::fs;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;
use crate::text_hunks::anchors;

/// PCP-FR-01: where a proposal lives, relative to the active worktree.
pub const PROPOSALS_REL: &str = ".synthesis/proposals";

const RECORD_EXT: &str = "toml";
/// PCP-FR-LGXQ: a legacy whole candidate. Read, never written.
const CONTENT_EXT: &str = "content";
/// PCP-FR-CNVT: the hunk documents.
const HUNKS_EXT: &str = "hunks";
const PRIOR_EXT: &str = "prior";
const JOURNAL_EXT: &str = "journal";

/// PCP-FR-LGXQ: the fixed id of a legacy proposal's one hunk.
pub const LEGACY_HUNK_ID: &str = "legacy";

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// A proposal id is a bare, filename-safe token: it is joined into a path, so a
/// separator or a dot segment would name a file elsewhere inside `.synthesis/`
/// even though `resolve_under` refuses to leave the root (PCP-FR-25,
/// FSA-FR-10).
pub fn is_valid_proposal_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// PCP-FR-01: the folder, resolved against the active worktree.
pub fn proposals_dir(root: &fs::RootFs) -> Result<PathBuf, String> {
    fs::resolve_under(root.path(), PROPOSALS_REL).map_err(|e| e.to_string())
}

fn at(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// PCP-FR-02: the folder is gitignored, registered when it is created.
///
/// Best-effort on the ignore itself, for the reason `crate::project_settings`
/// treats it so: an ignore file that cannot be written must not lose the write
/// that needed the folder. It is attempted **before** the write, so a fresh
/// project never commits a candidate.
fn ensure_folder(root: &fs::RootFs) -> Result<PathBuf, String> {
    let dir = proposals_dir(root)?;
    let _ = root.ensure_gitignored(synthesis_dir(root)?);
    Ok(dir)
}

fn synthesis_dir(root: &fs::RootFs) -> Result<PathBuf, String> {
    fs::resolve_under(root.path(), ".synthesis").map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Reads (pure over a root, so they unit-test without a Tauri runtime)
// ---------------------------------------------------------------------------

/// Every record the folder holds, most recently created first.
fn read_all(root: &fs::RootFs) -> Vec<PromptChangeProposal> {
    let Ok(dir) = proposals_dir(root) else {
        return Vec::new();
    };
    let Ok(entries) = root.list_dir(&dir) else {
        // PCP-FR-01: the folder is created by the first write that needs it, so
        // a project nobody has proposed a change in holds none at all — which is
        // no proposals rather than an error.
        return Vec::new();
    };
    let mut out: Vec<PromptChangeProposal> = Vec::new();
    for entry in entries {
        let Some(id) = entry.name.strip_suffix(&format!(".{RECORD_EXT}")) else {
            continue;
        };
        if !is_valid_proposal_id(id) {
            continue;
        }
        // A record that will not read is skipped rather than fatal, on the same
        // principle a damaged comment log line is skipped: one unreadable
        // proposal never costs the author the rest of them.
        if let Ok(mut record) = root.read_toml::<PromptChangeProposal>(at(&dir, id, RECORD_EXT)) {
            // PCP-FR-LGXQ: a legacy record is served with its one ledger row.
            ledger::backfill_legacy_ledger(&mut record);
            out.push(record);
        }
    }
    // Tie-broken by id so the order is total and a reload never reshuffles two
    // proposals recorded in the same millisecond.
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    out
}

/// PCP-FR-10: every proposal the artifact holds — pending, accepted, and
/// rejected alike — most recently created first.
///
/// Reads records and never a hunk document, so an artifact carrying a long
/// history of proposals costs a list no more than one carrying a single
/// proposal.
pub fn list_proposals_impl(root: &fs::RootFs, artifact_id: &str) -> Vec<PromptChangeProposal> {
    read_all(root).into_iter().filter(|p| p.artifact_id == artifact_id).collect()
}

/// One proposal's record by id.
pub fn read_proposal(root: &fs::RootFs, proposal_id: &str) -> Result<PromptChangeProposal, String> {
    if !is_valid_proposal_id(proposal_id) {
        return Err(ERR_PROPOSAL_NOT_FOUND.to_string());
    }
    let dir = proposals_dir(root)?;
    let mut record = root
        .read_toml::<PromptChangeProposal>(at(&dir, proposal_id, RECORD_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    // PCP-FR-LGXQ: a legacy record is served with its one ledger row.
    ledger::backfill_legacy_ledger(&mut record);
    Ok(record)
}

/// PCP-FR-04: the artifact's undecided proposal, if it has one.
pub fn pending_for(root: &fs::RootFs, artifact_id: &str) -> Option<PromptChangeProposal> {
    list_proposals_impl(root, artifact_id)
        .into_iter()
        .find(|p| p.state == PromptProposalState::Pending)
}

// ---------------------------------------------------------------------------
// The artifact the proposal names
// ---------------------------------------------------------------------------

/// PCP-FR-07 / PCP-FR-19: the artifact exists in the active worktree and its
/// resolved type is `prompt` or `doc` (ASC-FR-06, PPC-FR-06).
///
/// The two refusals are told apart because the corrections differ: one is a path
/// to fix, the other is a file to stop proposing against.
pub fn require_prompt_artifact(root: &fs::RootFs, artifact_id: &str) -> Result<PathBuf, String> {
    let path = fs::resolve_under(root.path(), artifact_id)
        .map_err(|_| ERR_ARTIFACT_NOT_FOUND.to_string())?;
    // `file_info`, not `Path::is_file`: the latter follows a symlink, so a link
    // at the id's path would pass as an artifact whose every guarded read is
    // refused (FSA-FR-17).
    if !root.file_info(&path).is_ok_and(|i| i.kind == fs::EntryKind::File) {
        return Err(ERR_ARTIFACT_NOT_FOUND.to_string());
    }
    use crate::scanning::ArtifactType;
    if !matches!(
        crate::artifacts::resolved_type(root, artifact_id),
        Some(ArtifactType::Prompt | ArtifactType::Doc)
    ) {
        return Err(ERR_NOT_A_PROMPT_ARTIFACT.to_string());
    }
    Ok(path)
}

/// PCP-FR-07 / PCP-FR-ZQAH: the text as the project's write path would persist
/// it, which is what a `no_change` comparison and the journal's candidate
/// checksum are both taken over (PST-FR-22).
fn normalise_for_write(root: &fs::RootFs, body: &str) -> String {
    crate::project_settings::line_endings_for(root).normalize(body)
}

// ---------------------------------------------------------------------------
// Serialisation per artifact (PCP-FR-04)
// ---------------------------------------------------------------------------

/// PCP-FR-04 / PCP-FR-14: **serialises every operation that records, decides, or
/// reconciles one artifact's proposals.**
///
/// Three races it closes, and two of them are only visible once said out loud:
///
/// - two agents recording against one artifact can otherwise both pass the
///   one-pending check and leave the project holding two undecided proposals, of
///   which only one is ever reachable (PCP-FR-04);
/// - two windows deciding one proposal can otherwise both pass the
///   already-decided check (PCP-FR-18) and land two outcomes over each other —
///   and two concurrent acceptances would mint two `event_id`s and append two
///   decision comments for one decision, which a fold cannot collapse
///   (CMS-FR-06);
/// - and a **reconciliation** running while an acceptance is between its journal
///   and its commit point reads that journal as an abandoned operation. A
///   journal on disk says where an operation got to and nothing about whether
///   the process that wrote it is still alive, so the only thing that can tell
///   "crashed" from "still running" is this lock.
///
/// One map for the application, entries never removed — a mutex is two words and
/// a session touches a handful of artifacts, where reference-counting the
/// entries would cost more than it saves and reintroduce the very race this
/// exists to close.
fn artifact_lock(artifact_id: &str) -> std::sync::MutexGuard<'static, ()> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mutex = {
        let mut guard = locks.lock().unwrap_or_else(|e| e.into_inner());
        *guard.entry(artifact_id.to_string()).or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// PCP-FR-14: acceptances of one **proposal** never interleave.
///
/// Separate from [`artifact_lock`] and taken without blocking: a second decision
/// against a proposal already being applied is the typed
/// `acceptance_in_progress` rather than a caller that waits behind the artifact
/// lock and then finds the proposal decided.
fn in_flight() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn try_acceptance_slot(proposal_id: &str) -> Option<AcceptanceSlot> {
    let set = in_flight();
    let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
    if !guard.insert(proposal_id.to_string()) {
        return None;
    }
    Some(AcceptanceSlot { proposal_id: proposal_id.to_string() })
}

/// Whether an acceptance of this proposal is running right now.
fn is_accepting(proposal_id: &str) -> bool {
    in_flight().lock().unwrap_or_else(|e| e.into_inner()).contains(proposal_id)
}

struct AcceptanceSlot {
    proposal_id: String,
}

impl Drop for AcceptanceSlot {
    fn drop(&mut self) {
        in_flight().lock().unwrap_or_else(|e| e.into_inner()).remove(&self.proposal_id);
    }
}

// ---------------------------------------------------------------------------
// The conversation a proposal belongs to
// ---------------------------------------------------------------------------

/// Where a proposal's comment lives — the artifact whose log holds the thread,
/// and whether that thread is a discussion — together with the origin kind that
/// thread's target decides (AGC-FR-05).
///
/// Returned as owned values rather than as a [`ThreadRef`], which borrows: the
/// decision path holds this across a mutation of the record, and a reconciliation
/// reconstructs it from disk with no record in hand at all.
fn locate_thread(
    root: &fs::RootFs,
    store: &fs::RootFs,
    thread_id: &str,
) -> Result<(String, bool), String> {
    let found = comments::read_discussion_by_id(store, root, thread_id)
        .ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?;
    let artifact_id = found.artifact_id().ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?.to_string();
    Ok((artifact_id, !found.is_fragment_targeted()))
}

fn thread_ref(artifact_id: &str, discussion: bool) -> ThreadRef<'_> {
    if discussion {
        ThreadRef::artifact_discussion(artifact_id)
    } else {
        ThreadRef::artifact(artifact_id)
    }
}

fn origin_kind_of(discussion: bool) -> &'static str {
    if discussion {
        ORIGIN_ARTIFACT_DISCUSSION
    } else {
        ORIGIN_ARTIFACT_COMMENT
    }
}

/// The one-hunk acceptance transaction and the decision comment.
mod accepting;
/// The hunks a change is recorded as.
mod candidate;
/// The `#[tauri::command]` surface.
mod commands;
/// Deciding hunks: accept, reject, decline, hold.
mod deciding;
/// Reading hunk documents, and the author's hunk edits.
mod hunks;
/// The per-hunk ledger, its derived state, and revisions.
mod ledger;
/// The typed errors and the record shapes.
mod model;
/// Settling an interrupted acceptance, and the sweep.
mod reconcile;
/// The one path that records a proposal.
mod recording;

use accepting::*;
pub use candidate::ProposedChange;
use candidate::{composed, plan_change};
pub use commands::*;
pub use deciding::hold_hunk_impl;
use deciding::*;
pub use hunks::*;
use ledger::legacy_document;
pub use ledger::{
    derive_state, revise_prompt_change_hunk, revision_target, HunkCounts, HunkLedgerRow, HunkState,
    RevisionTarget,
};
pub use model::*;
pub use reconcile::*;
pub use recording::*;

#[cfg(test)]
mod tests;
