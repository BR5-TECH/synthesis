//! Prompt change proposals (`PCP-prompt-change-proposals.md`).
//!
//! A **proposal** is a complete alternative text for one file the project
//! already holds and whose resolved artifact type is `prompt`, offered by an
//! agent in a conversation about that file (`crate::tools::propose_prompt_changes`)
//! and held beside the project until the author accepts it or declines it. It
//! exists because a collaborator that can see how a prompt would be better should
//! be able to show the author the whole of what it would write rather than
//! describe it — and because an agent that rewrites a published file without
//! being asked is not a collaborator.
//!
//! On-disk layout, in the active worktree (PCP-FR-01):
//!
//! ```text
//! .synthesis/proposals/
//!   <proposal-id>.toml      — the record
//!   <proposal-id>.content   — the proposed text, verbatim
//!   <proposal-id>.prior     — the artifact's bytes before an acceptance
//!   <proposal-id>.journal   — the standing acceptance, when one stands
//! ```
//!
//! The record and the text are separate files because the text is a document
//! and a record is not: a record carrying a document in a field would be read
//! whole on every list, and listing is what a tab does on open.
//!
//! ## The acceptance transaction
//!
//! An acceptance has **one commit point**, and it is the artifact write landing.
//! The order is the whole of the guarantee (PCP-FR-14):
//!
//! ```text
//!   pre-checks             → nothing written
//!   comment id + event id  → minted in advance
//!   <id>.prior             → the artifact's bytes as they stood
//!   <id>.journal           → what a reconciliation needs, with both checksums
//!   record = accepted      → comment_owed = true
//!   artifact write         ← THE COMMIT POINT (atomic, and the last fallible step)
//!   event                  → emitted after the commit point and never before
//!   decision comment       → idempotent by the pre-minted event id
//!   comment_owed = false; remove .prior; remove journal LAST
//! ```
//!
//! Because the artifact write is the last fallible step and is atomic
//! (FSA-FR-04), **the artifact is never restored on any path through this
//! module**: a failure before it means the file was never written, and a failure
//! of it means the write did not land. `write_failed` therefore always means
//! exactly what the author is told it means.
//!
//! ## What this module does not do
//!
//! It does not write an artifact itself. An acceptance hands the candidate to
//! `crate::artifacts`' own save path (PST-FR-09), which stays the only write
//! path for an artifact's file — so an accepted rewrite normalises line endings,
//! records the artifact as recently edited, and suppresses its own
//! external-change event exactly as the author's own typing does.
//!
//! It also decides nothing on its own (PCP-FR-20). There is no timer, no queue,
//! and no threshold anywhere in it.
//!
//! It shares nothing with `crate::draft_proposals` (PCP-FR-29): different
//! folder, different commands, different event, different attachment kind, and a
//! different acceptance.

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

/// PCP-FR-01: where a proposal lives, relative to the active worktree.
pub const PROPOSALS_REL: &str = ".synthesis/proposals";

const RECORD_EXT: &str = "toml";
const CONTENT_EXT: &str = "content";
const PRIOR_EXT: &str = "prior";
const JOURNAL_EXT: &str = "journal";

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
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
        if let Ok(record) = root.read_toml::<PromptChangeProposal>(at(&dir, id, RECORD_EXT)) {
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
/// Reads records and never a `.content` file, so an artifact carrying a long
/// history of proposals costs a list no more than one carrying a single
/// proposal.
pub fn list_proposals_impl(root: &fs::RootFs, artifact_id: &str) -> Vec<PromptChangeProposal> {
    read_all(root)
        .into_iter()
        .filter(|p| p.artifact_id == artifact_id)
        .collect()
}

/// One proposal's record by id.
pub fn read_proposal(
    root: &fs::RootFs,
    proposal_id: &str,
) -> Result<PromptChangeProposal, String> {
    if !is_valid_proposal_id(proposal_id) {
        return Err(ERR_PROPOSAL_NOT_FOUND.to_string());
    }
    let dir = proposals_dir(root)?;
    root.read_toml::<PromptChangeProposal>(at(&dir, proposal_id, RECORD_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())
}

/// PCP-FR-04: the artifact's undecided proposal, if it has one.
pub fn pending_for(root: &fs::RootFs, artifact_id: &str) -> Option<PromptChangeProposal> {
    list_proposals_impl(root, artifact_id)
        .into_iter()
        .find(|p| p.state == PromptProposalState::Pending)
}

/// PCP-FR-11: the candidate of any proposal, decided or not, with the checksum
/// of the bytes served.
pub fn load_content_impl(
    root: &fs::RootFs,
    proposal_id: &str,
) -> Result<PromptProposalContent, String> {
    read_proposal(root, proposal_id)?;
    let dir = proposals_dir(root)?;
    let path = at(&dir, proposal_id, CONTENT_EXT);
    let content = root
        .read_text(&path)
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    // The checksum is taken over the same file the text came from, so what the
    // caller holds and what it will name as its baseline describe one set of
    // bytes (PCP-FR-24, on PST-FR-15's terms).
    let checksum = root
        .sha256_file(&path)
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    Ok(PromptProposalContent { content, checksum })
}

// ---------------------------------------------------------------------------
// The artifact the proposal names
// ---------------------------------------------------------------------------

/// PCP-FR-07 / PCP-FR-19: the artifact exists in the active worktree and its
/// resolved type is exactly `prompt` (ASC-FR-06).
///
/// The two refusals are told apart because the corrections differ: one is a path
/// to fix, the other is a file to stop proposing against.
pub fn require_prompt_artifact(root: &fs::RootFs, artifact_id: &str) -> Result<PathBuf, String> {
    let path = fs::resolve_under(root.path(), artifact_id)
        .map_err(|_| ERR_ARTIFACT_NOT_FOUND.to_string())?;
    // `file_info`, not `Path::is_file`: the latter follows a symlink, so a link
    // at the id's path would pass as an artifact whose every guarded read is
    // refused (FSA-FR-17).
    if !root
        .file_info(&path)
        .is_ok_and(|i| i.kind == fs::EntryKind::File)
    {
        return Err(ERR_ARTIFACT_NOT_FOUND.to_string());
    }
    if crate::artifacts::resolved_type(root, artifact_id)
        != Some(crate::scanning::ArtifactType::Prompt)
    {
        return Err(ERR_NOT_A_PROMPT_ARTIFACT.to_string());
    }
    Ok(path)
}

/// PCP-FR-07 / PCP-FR-13: the candidate as the project's write path would
/// persist it, which is what a `no_change` comparison and the journal's
/// candidate checksum are both taken over (PST-FR-22).
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
        *guard
            .entry(artifact_id.to_string())
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
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
    Some(AcceptanceSlot {
        proposal_id: proposal_id.to_string(),
    })
}

/// Whether an acceptance of this proposal is running right now.
fn is_accepting(proposal_id: &str) -> bool {
    in_flight()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(proposal_id)
}

struct AcceptanceSlot {
    proposal_id: String,
}

impl Drop for AcceptanceSlot {
    fn drop(&mut self) {
        in_flight()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.proposal_id);
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
    let artifact_id = found
        .artifact_id()
        .ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?
        .to_string();
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

// ---------------------------------------------------------------------------
// Recording (PCP-FR-05)
// ---------------------------------------------------------------------------

/// What a caller hands [`record_prompt_proposal`]. A struct rather than five
/// positional arguments, three of which are strings that would transpose
/// silently.
pub struct NewPromptProposal<'a> {
    pub artifact_id: &'a str,
    pub content: &'a str,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation the turn belongs to (PCP-FR-05's `origin`).
    pub thread_id: &'a str,
}

/// PCP-FR-05: the **only** path by which a proposal comes into existence.
///
/// Internal by construction — no Tauri command creates one, so no frontend call
/// can record a proposal, forge one against an artifact, or attribute one to an
/// agent, exactly as none can attribute a comment to one (CMS-FR-41). The one
/// caller is `crate::tools::propose_prompt_changes`.
///
/// PCP-FR-06: the two writes and the append succeed together or leave nothing
/// behind. The content and the record reach disk before the comment is appended,
/// so no folded comment ever names a proposal that is not there; and a failure to
/// append removes both files before returning, so no candidate sits in the folder
/// that no conversation refers to.
pub fn record_prompt_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the candidate is a file of the project and lives in the
    // worktree; the comment announcing it is a conversation and lives in the
    // repository machine store.
    store: &fs::RootFs,
    new: NewPromptProposal<'_>,
) -> Result<PromptChangeProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // PCP-FR-04: the one-pending check below is a read followed by a write, and
    // AGC-FR-03 puts several turns in flight at once — two agents asked about
    // one file can otherwise both pass it.
    let _guard = artifact_lock(new.artifact_id);

    // PCP-FR-07, checked in the order a caller can act on.
    let artifact_path =
        require_prompt_artifact(root, new.artifact_id).map_err(|e| match e.as_str() {
            ERR_NOT_A_PROMPT_ARTIFACT => RecordRefusal::NotAPromptArtifact,
            _ => RecordRefusal::ArtifactNotFound,
        })?;
    if pending_for(root, new.artifact_id).is_some() {
        return Err(RecordRefusal::ProposalPending);
    }
    // PCP-FR-07: a proposal that changes nothing is nothing to decide. The
    // comparison is against the **normalised** candidate — what the write path
    // would actually put on disk (PST-FR-22) — because a candidate differing
    // only in its line endings would change no byte, and offering the author a
    // comparison with nothing in it wastes the one decision an artifact may hold
    // at a time. An empty candidate meets this on the one term every candidate
    // meets it on and on no term of its own.
    let current = root
        .read_text(&artifact_path)
        .map_err(|_| RecordRefusal::ArtifactNotFound)?;
    if current == normalise_for_write(root, new.content) {
        return Err(RecordRefusal::NoChange);
    }

    let (thread_artifact, discussion) =
        locate_thread(root, store, new.thread_id).map_err(|_| RecordRefusal::NotRecorded)?;
    let target = thread_ref(&thread_artifact, discussion);
    // CMS-FR-17: a locked conversation takes no comment, and a proposal nobody
    // can be told about is a proposal nobody will decide (PCP-FR-07).
    if comments::thread_at(store, target, new.thread_id)
        .map_err(|_| RecordRefusal::NotRecorded)?
        .locked
    {
        return Err(RecordRefusal::ThreadLocked);
    }

    let dir = ensure_folder(root).map_err(|_| RecordRefusal::NotRecorded)?;
    let id = new_note_id();
    let now = now_rfc3339();

    // PCP-FR-09: the proposed text exactly as the agent composed it. Not
    // normalised the way the artifact *save* is (PST-FR-22): what the author
    // reads in the comparison and what an acceptance puts into the file have to
    // be the same bytes, and the write path applies the project's convention at
    // the moment the change actually lands.
    root.write_text_atomic(at(&dir, &id, CONTENT_EXT), new.content)
        .map_err(|_| RecordRefusal::NotRecorded)?;
    // PCP-FR-22: stamped from the bytes that just landed rather than computed
    // over `new.content` in memory, so it describes the file a candidate save
    // will later checksum rather than the string this call was handed.
    let origin_checksum = root
        .sha256_file(at(&dir, &id, CONTENT_EXT))
        .map_err(|_| RecordRefusal::NotRecorded)?;

    // PCP-FR-06: the comment id is minted **here**, before either write, so the
    // record can name the comment it will be announced by without the comment
    // having to exist first. That is what lets both files reach disk ahead of an
    // append that is append-only and cannot be taken back (CMS-FR-04).
    let comment_id = new_note_id();
    let record = PromptChangeProposal {
        id: id.clone(),
        artifact_id: new.artifact_id.to_string(),
        path: new.artifact_id.to_string(),
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: comment_id.clone(),
        state: PromptProposalState::Pending,
        origin_checksum: Some(origin_checksum),
        candidate_edited: false,
        comment_owed: false,
        created_at: now.clone(),
        decided_at: None,
        agent: new.agent.clone(),
    };

    let cleanup = || {
        let _ = root.delete_under(&dir, format!("{id}.{CONTENT_EXT}"), false);
        let _ = root.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
    };

    if root
        .write_toml_atomic(at(&dir, &id, RECORD_EXT), &record)
        .is_err()
    {
        cleanup();
        return Err(RecordRefusal::NotRecorded);
    }

    let attachment = Attachment::PromptProposal {
        proposal_id: id.clone(),
        artifact_id: new.artifact_id.to_string(),
        path: new.artifact_id.to_string(),
    };
    let appended = match comments::append_agent_comment_with(
        store,
        target,
        new.thread_id,
        comment_id,
        new.rationale.to_string(),
        vec![attachment],
        new.agent,
        &now,
    ) {
        Ok(thread) => thread,
        Err(error) => {
            // PCP-FR-06: a failure to append removes **both** files, so no
            // candidate sits in the folder that no conversation refers to — and
            // the artifact's one pending slot is free again rather than held by
            // a proposal the author was never told about.
            cleanup();
            return Err(if error == comments::ERR_DISCUSSION_LOCKED {
                RecordRefusal::ThreadLocked
            } else {
                RecordRefusal::NotRecorded
            });
        }
    };

    // CMS-FR-51 / PCP-FR-06: the conversation announces itself the moment the
    // offer is in it rather than at the moment it is answered.
    comments::emit_discussion_changed(app, &appended);

    // PCP-FR-17: emitted when a proposal is **recorded**, not only when one is
    // decided. This is what the tab's pending indication, the rail's control,
    // and the shell's notification all redraw from (PCR-FR-16, CMT-FR-67,
    // PCR-FR-17); without it a proposal made during a session reaches no surface
    // until the project is reopened.
    announce(app, &BUFFER, &record);
    Ok(record)
}

// ---------------------------------------------------------------------------
// Deciding (PCP-FR-12 … PCP-FR-16)
// ---------------------------------------------------------------------------

/// Which way a decision went, and the body its comment carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Accept,
    Decline,
}

impl Decision {
    /// PCP-FR-16: what the conversation records. Prose rather than a marker,
    /// because a reader scrolling the thread months later reads the offer and
    /// the answer in sequence and should not have to open anything. It says
    /// which candidate was decided, because an agent is owed the difference
    /// between "your text landed" and "your text was rewritten and that landed".
    fn sentence(self, path: &str, edited: bool) -> String {
        match (self, edited) {
            (Decision::Accept, false) => format!("Accepted the proposed change to `{path}`."),
            (Decision::Accept, true) => {
                format!("Accepted an edited version of the proposed change to `{path}`.")
            }
            (Decision::Decline, false) => format!("Declined the proposed change to `{path}`."),
            (Decision::Decline, true) => {
                format!("Declined the proposed change to `{path}`, having edited it first.")
            }
        }
    }
}

/// PCP-FR-16: the decision comment's body — the sentence, then the author's
/// feedback where they wrote any.
fn decision_body(
    decision: Decision,
    path: &str,
    edited: bool,
    feedback: Option<&str>,
) -> String {
    let sentence = decision.sentence(path, edited);
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

/// PCP-FR-12 / PCP-FR-14: accept a pending proposal, as one transaction with one
/// commit point.
#[allow(clippy::too_many_arguments)]
fn accept_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    record: &PromptChangeProposal,
    thread_artifact: &str,
    discussion: bool,
    feedback: Option<&str>,
    by: &Participant,
    tracker: &ContentTracker,
) -> Result<PromptDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // PCP-FR-14: one acceptance of a proposal at a time. A second is refused
    // rather than interleaved.
    let Some(_slot) = try_acceptance_slot(&record.id) else {
        return Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string());
    };

    let dir = ensure_folder(root)?;
    let artifact_path = require_prompt_artifact(root, &record.artifact_id)?;
    let candidate = root
        .read_text(at(&dir, &record.id, CONTENT_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;

    // PCP-FR-14: the comment's identity and its `event_id` are minted in
    // advance, so a re-append after a crash contributes nothing a second time
    // (CMS-FR-06).
    let journal = Journal {
        operation_id: new_note_id(),
        proposal_id: record.id.clone(),
        artifact_id: record.artifact_id.clone(),
        path: record.path.clone(),
        prior_sha256: String::new(),
        candidate_sha256: fs::sha256_bytes(normalise_for_write(root, &candidate).as_bytes()),
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: record.thread_id.clone(),
        comment_body: decision_body(
            Decision::Accept,
            &record.path,
            record.candidate_edited,
            feedback,
        ),
        created_at: now_rfc3339(),
        comment_by: by.clone(),
    };

    // Everything from here to the commit point either lands or is undone whole,
    // and the artifact is never written by any of it.
    let prepared = || -> Result<Journal, String> {
        // PCP-FR-14: the artifact's whole current bytes, byte-for-byte and
        // unnormalised, written and then read back and checksummed.
        let prior = root
            .read_bytes(&artifact_path)
            .map_err(|e| e.to_string())?;
        root.write_bytes_atomic(at(&dir, &record.id, PRIOR_EXT), &prior)
            .map_err(|e| e.to_string())?;
        let prior_sha256 = root
            .sha256_file(at(&dir, &record.id, PRIOR_EXT))
            .map_err(|e| e.to_string())?;
        let journal = Journal {
            prior_sha256,
            ..journal.clone()
        };
        root.write_toml_atomic(at(&dir, &record.id, JOURNAL_EXT), &journal)
            .map_err(|e| e.to_string())?;
        // The record moves before the artifact does, so a journal on disk beside
        // a record reading `accepted` and an artifact still holding its own
        // bytes is exactly the "did not commit" reading of PCP-FR-14.
        let decided = PromptChangeProposal {
            state: PromptProposalState::Accepted,
            decided_at: Some(now_rfc3339()),
            comment_owed: true,
            ..record.clone()
        };
        root.write_toml_atomic(at(&dir, &record.id, RECORD_EXT), &decided)
            .map_err(|e| e.to_string())?;
        Ok(journal)
    };

    let journal = match prepared() {
        Ok(journal) => journal,
        Err(reason) => {
            // PCP-FR-14: the artifact was never written, so there is nothing to
            // restore — only this module's own record to put back. A rollback
            // that could not be completed leaves the journal in place, so the
            // next reconciliation finishes it before any command answers; either
            // way the answer is `write_failed`, which is true of the artifact on
            // both paths without qualification.
            let _ = roll_back(root, &dir, record);
            log_recovery(app, "rollback", record, &reason);
            return Err(ERR_WRITE_FAILED.to_string());
        }
    };

    // ---- THE COMMIT POINT: the artifact write, atomic and last ---------------
    // PCP-FR-12 / PCP-FR-13: through `crate::artifacts`' own write path, which
    // stays the only write path for an artifact's file — so the acceptance
    // normalises line endings and suppresses its own external-change event
    // exactly as the author's own typing does. It reaches the Dashboard's
    // Recently edited widget the same way too, by the modification time the
    // write gives the file (PST-FR-31).
    if let Err(reason) = crate::artifacts::save_validated_artifact(
        root,
        &record.artifact_id,
        &candidate,
        tracker,
    ) {
        // The one case with anything to undo, and what it undoes is this
        // module's own record rather than the author's file: the write is atomic
        // (FSA-FR-04), so a failure means it did not land.
        let _ = roll_back(root, &dir, record);
        log_recovery(app, "rollback", record, &reason);
        return Err(ERR_WRITE_FAILED.to_string());
    }

    let accepted = read_proposal(root, &record.id)?;
    // PCP-FR-17: emitted **after** the commit point and never before, so no
    // surface is ever told a proposal is accepted while the acceptance could
    // still be rolled back.
    announce(app, &BUFFER, &accepted);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "prompt change acceptance committed",
        log_fields! {
            "artifactId" => &record.artifact_id,
            "proposalId" => &record.id,
            "path" => &record.path,
            "operationId" => &journal.operation_id,
        },
    );

    // PCP-FR-14 / PCP-FR-16: the append is the tail of a successful acceptance.
    match append_decision(app, store, &journal, thread_artifact, discussion) {
        Ok(()) => {
            let settled = clear_owed(root, &dir, &record.id).unwrap_or(accepted);
            finish(root, &dir, &record.id);
            Ok(PromptDecisionOutcome {
                proposal: settled,
                comment_id: Some(journal.comment_id),
                origin_kind: origin_kind_of(discussion),
            })
        }
        Err(reason) => {
            // PCP-FR-14: the acceptance landed and the comment is owed. The
            // journal and `.prior` stay on disk, the journal carrying the minted
            // identity the recovery needs.
            log_recovery(app, "comment_owed", record, &reason);
            Err(ERR_ACCEPTANCE_INCOMPLETE.to_string())
        }
    }
}

/// PCP-FR-14: undo a transaction that never reached its commit point.
///
/// What it undoes is **this module's own record**; the artifact never needs
/// restoring, the write being the last fallible step and atomic. The rollback is
/// read back and verified before the call returns, and the journal and `.prior`
/// are removed only once it has been.
fn roll_back(
    root: &fs::RootFs,
    dir: &Path,
    record: &PromptChangeProposal,
) -> Result<(), String> {
    let pending = PromptChangeProposal {
        state: PromptProposalState::Pending,
        decided_at: None,
        comment_owed: false,
        ..record.clone()
    };
    root.write_toml_atomic(at(dir, &record.id, RECORD_EXT), &pending)
        .map_err(|e| e.to_string())?;
    // Read back and verified: a rollback that cannot be shown to have happened
    // leaves the journal in place so the next reconciliation completes it.
    let verified = root
        .read_toml::<PromptChangeProposal>(at(dir, &record.id, RECORD_EXT))
        .map_err(|e| e.to_string())?;
    if verified.state != PromptProposalState::Pending {
        return Err("rollback not verified".to_string());
    }
    finish(root, dir, &record.id);
    Ok(())
}

/// PCP-FR-14: the journal and `.prior` go, the journal **last of all**, so a
/// folder holding a journal is always a folder with something still to finish.
fn finish(root: &fs::RootFs, dir: &Path, proposal_id: &str) {
    let _ = root.delete_under(dir, format!("{proposal_id}.{PRIOR_EXT}"), false);
    let _ = root.delete_under(dir, format!("{proposal_id}.{JOURNAL_EXT}"), false);
}

/// PCP-FR-14: `comment_owed` is cleared only once the append has succeeded.
fn clear_owed(
    root: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
) -> Result<PromptChangeProposal, String> {
    let mut record = read_proposal(root, proposal_id)?;
    if !record.comment_owed {
        return Ok(record);
    }
    record.comment_owed = false;
    root.write_toml_atomic(at(dir, proposal_id, RECORD_EXT), &record)
        .map_err(|e| e.to_string())?;
    Ok(record)
}

/// PCP-FR-14 / PCP-FR-16: append the decision comment under the ids the journal
/// minted, so a repeat contributes nothing a second time (CMS-FR-06).
///
/// The lock the conversation may since have taken is deliberately not consulted:
/// PCP-FR-16 checked it before the transaction began, and the transaction has
/// committed. A lock applied in the window between is a fact about what the
/// conversation will take **next**, not licence to leave a decision the author
/// already made unrecorded.
fn append_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    // CMS-FR-01: the decision comment goes into the repository machine store
    // alone; the artifact, the record, and the journal are the worktree's.
    store: &fs::RootFs,
    journal: &Journal,
    thread_artifact: &str,
    discussion: bool,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let thread = comments::append_decision_comment_event(
        store,
        thread_ref(thread_artifact, discussion),
        &journal.thread_id,
        journal.event_id.clone(),
        journal.comment_id.clone(),
        journal.comment_body.clone(),
        &journal.comment_by,
        &now_rfc3339(),
    )?;
    // AGC-FR-31 / CTA-FR-RUVS: the author's decision is a human-authored comment,
    // so it retires this conversation's offer to retry a failed turn exactly as
    // any other human comment does.
    crate::agent_conversations::retire_recoverable_for_human_comment(app, &journal.thread_id);
    // CMS-FR-51: the decision's own comment reaches the conversation's surface on
    // exactly the terms the proposal's did.
    comments::emit_discussion_changed(app, &thread);
    Ok(())
}

mod model;
mod reconcile;
mod commands;

pub use model::*;
pub use reconcile::*;
pub use commands::*;

#[cfg(test)]
mod tests;
