//! Draft history — a draft's own settled versions of its one prompt
//! (`../../specifications/core/DHS-draft-history.md`).
//!
//! One thing puts a version in a draft's history and nothing else does:
//! accepting a change an agent proposed (DHS-FR-08). That acceptance writes two
//! entries the first time it runs — the prompt as it stood before it, which is
//! the `Original`, and the accepted text — and one every time after. The
//! author's own typing between those moments is the live prompt rather than a
//! version of it, so a rail of fifty rows is fifty decisions rather than fifty
//! pauses in a sentence.
//!
//! A draft nobody has proposed a change to therefore holds **no entry at all**:
//! its live prompt is its `Original` and there is nothing it could have moved on
//! from (DHS-FR-07). A version is a thing the prompt used to be, and until a
//! change lands there is no such thing.
//!
//! On-disk layout, inside the draft's own directory (DRS-FR-01):
//!
//! ```text
//! .synthesis/drafts/<folder>/<draft-id>/history/
//!   <entry-id>.toml       — the manifest: seq, path, source, timestamp, digest
//!   <entry-id>.snapshot   — the prompt's bytes, exactly as the save path wrote them
//!   journal.toml          — the standing acceptance operation, when one stands
//!   journal.prior         — the prompt's bytes as they stood before it began
//! ```
//!
//! The manifest and the payload are separate files because the payload is a
//! document and a manifest is not: a manifest carrying a document in a field
//! would be read on every list, and listing is the operation that has to stay
//! cheap however long the history has grown (DHS-FR-01, DHS-FR-10).
//!
//! ## The acceptance transaction
//!
//! An acceptance has to land in three places at once — the prompt's bytes, this
//! history, and the proposal's decided state — and then owe the conversation one
//! comment. This module owns that whole transaction because it owns the journal
//! that makes it recoverable (DHS-FR-13 … DHS-FR-21). The order is fixed and is
//! the whole of what makes a crash recoverable:
//!
//! ```text
//!   journal.prior         → the bytes a rollback restores
//!   journal = prepared    → nothing this operation did is final
//!   <original>.snapshot   → the prompt as it stood, where the draft holds no
//!   <original>.toml         entry yet (DHS-FR-07)
//!   <entry>.snapshot      → payload before manifest, so a half-written entry
//!   <entry>.toml            has no manifest to be listed by
//!   prompt                → through DRS-FR-12's own save path, the only one
//!   proposal = accepted
//!   journal = committed   ← THE COMMIT POINT (one atomic rewrite)
//!   decision comment      → idempotent, by a pre-minted event id
//!   journal = complete    → removed
//! ```
//!
//! Everything before the commit point rolls **back**; everything after it rolls
//! **forward**. The decision comment stands after the commit point deliberately:
//! a conversation log is append-only and no line of it is ever rewritten or
//! removed (CMS-FR-04), so the one recoverable place for the append is where a
//! repeat of it costs nothing — which the pre-minted `event_id` guarantees,
//! the fold applying the first event it sees for an id and ignoring every later
//! one (CMS-FR-06).
//!
//! Deliberately absent: anything AI, and any notion of the application-wide
//! artifact and Git history (`HIS-history.md`), with which this shares neither
//! storage nor surface.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::comments::{self, Participant};
use crate::draft_proposals::{self, DraftChangeProposal, ProposalState};
use crate::drafts;
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};
use crate::project::ProjectState;

/// The manifest half of an entry.
const MANIFEST_EXT: &str = "toml";
/// The payload half of an entry — the prompt's bytes, unadorned.
const SNAPSHOT_EXT: &str = "snapshot";
/// DHS-FR-03: the standing acceptance operation.
const JOURNAL_FILE: &str = "journal.toml";
/// DHS-FR-03: the prompt's bytes as they stood before that operation began.
const JOURNAL_PRIOR: &str = "journal.prior";

// ---------------------------------------------------------------------------
// Typed errors (DHS contract surface)
// ---------------------------------------------------------------------------

/// DHS-FR-25: `draft_id` names no draft in the active worktree.
///
/// Typed here rather than passed through, because `crate::drafts` answers a
/// missing draft with prose naming the id — which is right for a log line and
/// wrong for a contract four surfaces have to recognise.
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
/// DHS-FR-25: `entry_id` names no entry of the draft.
pub const ERR_ENTRY_NOT_FOUND: &str = "entry_not_found";
/// DHS-FR-11: the payload is absent, does not decode, or does not match the
/// digest and length its manifest recorded.
pub const ERR_SNAPSHOT_CORRUPT: &str = "snapshot_corrupt";
/// DHS-FR-23: a journal stands for this draft that reconciliation could not
/// clear, so two acceptances would otherwise interleave over one prompt.
pub const ERR_ACCEPTANCE_IN_PROGRESS: &str = "acceptance_in_progress";
/// DHS-FR-17: the acceptance landed and the decision comment is still owed.
///
/// Not a failure of the acceptance and never rendered as one (DCR-FR-16): the
/// prompt holds the accepted text, the entry stands, and the proposal reads
/// `accepted`. What is outstanding is the line the conversation is owed, which
/// the next reconciliation completes.
pub const ERR_ACCEPTANCE_INCOMPLETE: &str = "acceptance_incomplete";
/// DHS-FR-21: a journal stands that could not be rolled back or forward, so no
/// command here answers from a state it could not resolve.
pub const ERR_HISTORY_RECOVERY_FAILED: &str = "history_recovery_failed";
/// DHS-FR-17: a failure before the commit point, rolled back whole.
pub const ERR_WRITE_FAILED: &str = "write_failed";

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// DHS-FR-07 / DHS-FR-08: why a version exists.
///
/// A tagged union rather than a flag, because the two carry different material:
/// `Original` carries nothing at all — it is the prompt the author had written
/// by the time the first change was accepted — and an accepted proposal carries
/// the proposal it came from and the agent that composed it, which is what the
/// rail's row renders as its source label (NAW-FR-07).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DraftHistorySource {
    Original,
    #[serde(rename_all = "camelCase")]
    ProposalAccepted {
        proposal_id: String,
        /// DHS-FR-08: the change this version came from. Absent on an entry a
        /// build before per-hunk acceptance wrote, which reads as its own
        /// single-entry group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hunk_id: Option<String>,
        agent: Participant,
    },
}

/// One settled version of a draft's prompt — the manifest half of an entry, and
/// what `list_draft_history` returns a row of.
///
/// Field order is load-bearing for TOML rather than for the wire: `serde`'s TOML
/// serialiser emits fields in declaration order and refuses a scalar written
/// after a table, so `source` — which is a table — stands last.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftHistoryEntry {
    /// DHS-FR-04: opaque, generated at creation, stable for the entry's
    /// lifetime, and derived from neither the draft, nor the path, nor the
    /// content — so two entries holding identical bytes are two entries.
    pub id: String,
    pub draft_id: String,
    /// DHS-FR-05: `1` for the `Original` the first acceptance writes and one
    /// greater than the highest the draft holds for every entry after it. The
    /// sort key, so the rail renders the same sequence on every list rather than
    /// sorting on a timestamp two entries could share.
    pub seq: u32,
    /// DHS-FR-09: the draft-relative path the prompt occupied when the snapshot
    /// was taken. A snapshot is a reading of the prompt at a moment rather than
    /// a pointer to wherever it later moved (DRS-FR-14).
    pub path: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub byte_len: u64,
    pub sha256: String,
    pub source: DraftHistorySource,
}

/// DHS-FR-10: the live prompt's own digest, returned beside the entries so the
/// rail can say whether the author has typed since the last settled version
/// without reading a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePrompt {
    pub path: String,
    pub byte_len: u64,
    pub sha256: String,
    /// The live prompt's bytes are the newest entry's bytes (NAW-FR-37).
    ///
    /// True for a draft holding no entry at all: its live prompt is its
    /// `Original` (DHS-FR-07), so there is no version it could have moved on
    /// from and nothing for the rail to mark it as modified since.
    pub matches_latest: bool,
}

/// What `list_draft_history` answers with (DHS-FR-10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftHistoryList {
    /// Ascending `seq`, oldest first.
    pub entries: Vec<DraftHistoryEntry>,
    pub live: LivePrompt,
}

/// What `load_draft_history_entry` answers with (DHS-FR-11) — the snapshot's
/// text together with the digest it was verified against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftHistoryContent {
    pub content: String,
    pub sha256: String,
}

// ---------------------------------------------------------------------------
// The journal (DHS-FR-13, DHS-FR-14)
// ---------------------------------------------------------------------------

/// DHS-FR-14: where a standing acceptance has got to.
///
/// Three positions and one commit point. `prepared` says nothing the operation
/// did is final; `committed` says the prompt, the entry, and the proposal's
/// accepted state are final together; `complete` says the conversation has been
/// told too, after which the journal goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Committed,
    Complete,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Prepared => "prepared",
            Phase::Committed => "committed",
            Phase::Complete => "complete",
        }
    }
}

/// The standing acceptance operation (DHS-FR-13).
///
/// It carries everything a reconciliation needs to finish or undo the operation
/// **without its caller**, because the caller may be a process that no longer
/// exists. That is why the decision comment's body, its identity, and both of
/// its pre-minted ids are here rather than being recomposed: a body recomposed
/// after a crash could differ from the one that was owed, and an id minted a
/// second time would fold as a second comment (CMS-FR-06).
///
/// It is not a user-visible history and is exposed by no command, no event, and
/// no surface. As with [`DraftHistoryEntry`], the tables stand last so the TOML
/// serialiser accepts the shape.
fn resolves_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Journal {
    operation_id: String,
    draft_id: String,
    proposal_id: String,
    /// DHS-FR-13: the change being accepted. Absent on a journal an older build
    /// left, which reads as the whole-proposal acceptance it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hunk_id: Option<String>,
    /// DHS-FR-13: whether this decision leaves the proposal with nothing
    /// undecided. Only a resolving decision owes the conversation a comment.
    #[serde(default = "resolves_default")]
    resolves: bool,
    /// The prompt's draft-relative path at the moment the operation began.
    path: String,
    /// The digest of the bytes `journal.prior` holds.
    prior_sha256: String,
    /// The digest of the candidate **as the save path will persist it**, which
    /// is what tells a rollback whether the prompt has already been written.
    candidate_sha256: String,
    /// Absent where the acceptance changes no persisted byte (DHS-FR-16), in
    /// which case the operation creates no entry at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entry_id: Option<String>,
    /// DHS-FR-07: the `Original` this operation writes alongside the accepted
    /// entry, where the draft holds no entry yet. Absent for every acceptance
    /// after the first, and for one that changes nothing.
    ///
    /// Carried in the journal for the same reason `entry_id` is: a rollback has
    /// to remove exactly the entries the operation created, and a reconciliation
    /// running in another process has nothing but this record to learn them from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_entry_id: Option<String>,
    entry_seq: u32,
    /// DHS-FR-20 / DHS-FR-21: minted before the transaction begins, so a
    /// re-append after a crash contributes nothing a second time.
    comment_id: String,
    event_id: String,
    thread_id: String,
    comment_body: String,
    created_at: String,
    phase: Phase,
    /// The proposing agent, carried into the entry's source metadata.
    agent: Participant,
    /// The acting human the decision comment is stamped with.
    comment_by: Participant,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// DHS-FR-03: an entry id is a bare, filename-safe token that is neither
/// reserved name, so no entry's manifest or payload can be mistaken for the
/// journal or the journal for an entry.
pub fn is_valid_entry_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id != "journal"
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn manifest_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{MANIFEST_EXT}"))
}

fn snapshot_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{SNAPSHOT_EXT}"))
}

// ---------------------------------------------------------------------------
// Reads (pure over a root, so they unit-test without a Tauri runtime)
// ---------------------------------------------------------------------------

/// Every manifest in a draft's `history/`, ascending by `seq` (DHS-FR-05).
///
/// Reads manifests and never a payload, so a draft carrying a long history costs
/// a list one directory walk however large its prompt has grown (DHS-FR-10). A
/// manifest that will not parse is skipped rather than failing the list: one
/// damaged version must not cost the author every other one.
/// [`drafts::draft_dir`] with DHS-FR-25's typed refusal in place of its prose.
fn draft_dir(root: &fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    drafts::draft_dir(root, draft_id).map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())
}

fn manifests(root: &fs::RootFs, dir: &Path) -> Vec<DraftHistoryEntry> {
    let Ok(entries) = root.list_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<DraftHistoryEntry> = Vec::new();
    for entry in entries {
        if entry.kind != fs::EntryKind::File {
            continue;
        }
        let Some(id) = entry.name.strip_suffix(&format!(".{MANIFEST_EXT}")) else {
            continue;
        };
        if !is_valid_entry_id(id) {
            continue;
        }
        if let Ok(manifest) = root.read_toml::<DraftHistoryEntry>(dir.join(&entry.name)) {
            out.push(manifest);
        }
    }
    out.sort_by(|a, b| a.seq.cmp(&b.seq).then_with(|| a.id.cmp(&b.id)));
    out
}

/// The journal standing for this draft, if one is (DHS-FR-13).
///
/// A journal that will not parse is an error rather than an absence: acting as
/// though no operation stood would let a second acceptance run over a prompt
/// whose first one may have written half of itself.
fn read_journal(root: &fs::RootFs, dir: &Path) -> Result<Option<Journal>, String> {
    let path = dir.join(JOURNAL_FILE);
    if root.file_info(&path).is_err() {
        return Ok(None);
    }
    root.read_toml::<Journal>(&path)
        .map(Some)
        .map_err(|_| ERR_HISTORY_RECOVERY_FAILED.to_string())
}

/// DHS-FR-12: an entry is exposed unless the standing journal is `prepared` and
/// names it — so an entry written for an acceptance that has not committed is
/// invisible to every reader, and no partial acceptance is ever presented as a
/// version of the prompt.
///
/// Both of the operation's entries are named: a first acceptance writes the
/// `Original` as well as the accepted version (DHS-FR-07), and exposing one
/// without the other would put a version in the rail for an acceptance that may
/// yet be rolled back.
fn is_hidden(journal: &Option<Journal>, id: &str) -> bool {
    match journal {
        Some(j) if j.phase == Phase::Prepared => {
            j.entry_id.as_deref() == Some(id) || j.original_entry_id.as_deref() == Some(id)
        }
        _ => false,
    }
}

/// DHS-FR-10: every exposed entry oldest first, together with the live prompt's
/// own path, length, and digest.
pub fn list_impl(root: &fs::RootFs, draft_id: &str) -> Result<DraftHistoryList, String> {
    let dir = draft_dir(root, draft_id)?;
    let prompt = drafts::require_prompt(root, draft_id)?;
    let hist = dir.join(drafts::HISTORY_DIR);
    let journal = read_journal(root, &hist)?;
    let entries: Vec<DraftHistoryEntry> = manifests(root, &hist)
        .into_iter()
        .filter(|e| !is_hidden(&journal, &e.id))
        .collect();

    let prompt_abs = drafts::draft_file_abs_path(root, draft_id, &prompt)?;
    let sha256 = root.sha256_file(&prompt_abs).map_err(|e| e.to_string())?;
    let byte_len = root.file_info(&prompt_abs).map(|i| i.size).unwrap_or(0);
    // DHS-FR-07: a draft holding no entry is one nothing has been accepted
    // against, and its live prompt is its `Original` — so it has moved on from
    // nothing, rather than from a version that is not there.
    let matches_latest = entries.last().is_none_or(|e| e.sha256 == sha256);
    Ok(DraftHistoryList {
        entries,
        live: LivePrompt {
            path: prompt,
            byte_len,
            sha256,
            matches_latest,
        },
    })
}

/// Which draft holds this entry, and where.
///
/// An entry id is opaque and says nothing about the draft carrying it, so the
/// draft is found the way a proposal's is — by walking the hierarchy that
/// already knows where every draft is filed.
fn locate(root: &fs::RootFs, entry_id: &str) -> Option<(String, PathBuf, DraftHistoryEntry)> {
    if !is_valid_entry_id(entry_id) {
        return None;
    }
    for summary in drafts::list_drafts_impl(root).drafts {
        let Ok(dir) = drafts::draft_dir(root, &summary.id) else {
            continue;
        };
        let hist = dir.join(drafts::HISTORY_DIR);
        if let Ok(manifest) = root.read_toml::<DraftHistoryEntry>(manifest_path(&hist, entry_id)) {
            return Some((summary.id, hist, manifest));
        }
    }
    None
}

/// DHS-FR-11: one entry's payload as UTF-8 text, verified against the digest and
/// the length its manifest recorded before a byte of it is returned.
///
/// Content is loaded here rather than by the list, so a rail that is open costs
/// nothing until a version is actually selected (NAW-FR-40).
pub fn load_impl(root: &fs::RootFs, entry_id: &str) -> Result<DraftHistoryContent, String> {
    let (draft_id, hist, manifest) = locate(root, entry_id).ok_or(ERR_ENTRY_NOT_FOUND)?;
    // An entry of a draft that has since become inconsistent is still an entry
    // of that draft, and DHS-FR-25 refuses every command against one.
    drafts::require_prompt(root, &draft_id)?;
    // DHS-FR-12: an entry the standing journal has not committed is not exposed,
    // by this operation any more than by the list.
    let journal = read_journal(root, &hist)?;
    if is_hidden(&journal, entry_id) {
        return Err(ERR_ENTRY_NOT_FOUND.to_string());
    }
    let payload = snapshot_path(&hist, entry_id);
    let bytes = root
        .read_bytes(&payload)
        .map_err(|_| ERR_SNAPSHOT_CORRUPT.to_string())?;
    if bytes.len() as u64 != manifest.byte_len
        || fs::sha256_bytes(&bytes) != manifest.sha256
    {
        return Err(ERR_SNAPSHOT_CORRUPT.to_string());
    }
    let content = String::from_utf8(bytes).map_err(|_| ERR_SNAPSHOT_CORRUPT.to_string())?;
    Ok(DraftHistoryContent {
        content,
        sha256: manifest.sha256,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands and the event
// ---------------------------------------------------------------------------

/// DHS-FR-22: a version became visible, or reconciliation withdrew one.
///
/// Kebab-case for the reason every event in this application is: Tauri rejects
/// the rest of the character set at `emit`, leaving a channel that is silently
/// dead in production.
pub const HISTORY_CHANGED: &str = "draft-history-changed";

/// What the event carries: the draft, and the entry — `null` where a
/// reconciliation removed one (DHS-FR-22).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryChanged {
    pub draft_id: String,
    pub entry: Option<DraftHistoryEntry>,
}

/// Emit [`HISTORY_CHANGED`].
///
/// Best-effort, for the reason every emit site here discards its result: an
/// entry that was written must not be reported as failed because the
/// notification of it could not be delivered.
pub fn announce_entry<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: &str,
    entry: Option<&DraftHistoryEntry>,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _ = app.emit(
        HISTORY_CHANGED,
        HistoryChanged {
            draft_id: draft_id.to_string(),
            entry: entry.cloned(),
        },
    );
    // DHS-FR-ZQNM: an entry that becomes **exposed** contributes exactly one
    // `draft_history_entry` line to the draft's statistics log, appended at the
    // same commit point this event is emitted at. Asynchronous and never
    // blocking (DSS-FR-TUMX): it is not part of the acceptance transaction, it
    // is not journaled, and no acceptance is refused, delayed, rolled back, or
    // reported incomplete because a line could not be written. Withdrawal
    // carries `None` and appends nothing — the fold reconciles a re-appended
    // line by the entry's own identity instead (DSS-FR-PNUE).
    if let Some(entry) = entry {
        crate::statistics::record_statistics_event(
            app,
            draft_id,
            crate::statistics::EventBody::DraftHistoryEntry {
                entry_id: entry.id.clone(),
                seq: entry.seq,
                source_kind: match entry.source {
                    DraftHistorySource::Original => {
                        crate::statistics::HistorySourceKind::Original
                    }
                    DraftHistorySource::ProposalAccepted { .. } => {
                        crate::statistics::HistorySourceKind::ProposalAccepted
                    }
                },
            },
        );
    }
    // DHS-FR-26: an `INFO` record when an entry is created, naming the draft and
    // the entry — and **never a byte of the snapshot**, which is material the
    // author or a model composed. The length is the shape of the payload rather
    // than any of its content.
    if let Some(entry) = entry {
        logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "draft history entry created",
            log_fields! {
                "draftId" => draft_id,
                "entryId" => &entry.id,
                "seq" => entry.seq,
                "source" => match entry.source {
                    DraftHistorySource::Original => "original",
                    DraftHistorySource::ProposalAccepted { .. } => "proposal_accepted",
                },
                "bytes" => entry.byte_len,
            },
        );
    }
}

/// DHS-FR-26: a `WARN` naming the draft, the entry, the operation, and the
/// reason — and never a byte of a snapshot, of the prompt, of a candidate, of a
/// rationale, or of any feedback, all of which are material an author or a model
/// composed. The reason is this module's own fixed prose, never a value.
fn log_recovery<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    outcome: &'static str,
    draft_id: &str,
    journal: &Journal,
    reason: &str,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft acceptance reconciled",
        log_fields! {
            "draftId" => draft_id,
            "proposalId" => &journal.proposal_id,
            "operationId" => &journal.operation_id,
            "entryId" => journal.entry_id.clone().unwrap_or_default(),
            "originalEntryId" => journal.original_entry_id.clone().unwrap_or_default(),
            "phase" => journal.phase.as_str(),
            "outcome" => outcome,
            "reason" => reason,
        },
    );
}

#[tauri::command]
pub fn list_draft_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    project: State<'_, ProjectState>,
) -> Result<DraftHistoryList, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    let store = project.require_store()?;
    // DHS-FR-18 / DHS-FR-21: never answer from a state that could not be
    // reconciled.
    reconcile(&app, &root, &store, &draft_id)?;
    list_impl(&root, &draft_id)
}

#[tauri::command]
pub fn load_draft_history_entry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entry_id: String,
    project: State<'_, ProjectState>,
) -> Result<DraftHistoryContent, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    let store = project.require_store()?;
    if let Some((draft_id, _, _)) = locate(&root, &entry_id) {
        reconcile(&app, &root, &store, &draft_id)?;
    }
    load_impl(&root, &entry_id)
}

/// Leave a draft in the state a process killed inside an acceptance leaves it,
/// for a test in another module (`crate::draft_proposals`, DCP-FR-28, DHS-FR-19, DHS-FR-20, DHS-FR-21).
///
/// Every write the transaction of DHS-FR-15 makes up to `phase`, and the journal
/// standing there — replayed rather than run-and-rewound, because the one thing
/// that distinguishes the phases is whether the decision comment was appended,
/// and running the real transaction always appends it.
#[cfg(test)]
fn stage_for_test(
    root: &fs::RootFs,
    draft_id: &str,
    proposal: &DraftChangeProposal,
    prior: &str,
    by: &Participant,
    phase: Phase,
) -> Result<Journal, String> {
    let dir = drafts::draft_dir(root, draft_id)?;
    let hist = dir.join(drafts::HISTORY_DIR);
    let candidate = draft_proposals::load_content_impl(root, &proposal.id)?;
    let normalised = drafts::normalise_for_write(root, &candidate.content);
    let entry_id = new_note_id();
    let held = manifests(root, &hist);
    // DHS-FR-07: a first acceptance settles the `Original` on its way past, so a
    // staging that skipped it would not be the operation a crash interrupts.
    let original_id = held.is_empty().then(new_note_id);
    let base_seq = held.iter().map(|e| e.seq).max().unwrap_or(0);
    let seq = if original_id.is_some() {
        base_seq + 2
    } else {
        base_seq + 1
    };
    let journal = Journal {
        operation_id: new_note_id(),
        draft_id: draft_id.to_string(),
        proposal_id: proposal.id.clone(),
        hunk_id: None,
        resolves: true,
        path: proposal.path.clone(),
        prior_sha256: fs::sha256_bytes(prior.as_bytes()),
        candidate_sha256: fs::sha256_bytes(normalised.as_bytes()),
        entry_id: Some(entry_id.clone()),
        original_entry_id: original_id.clone(),
        entry_seq: seq,
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: proposal.thread_id.clone(),
        comment_body: format!("Accepted the proposed change to `{}`.", proposal.path),
        created_at: now_rfc3339(),
        phase,
        agent: proposal.agent.clone(),
        comment_by: by.clone(),
    };
    root.write_bytes_atomic(hist.join(JOURNAL_PRIOR), prior.as_bytes())
        .map_err(|e| e.to_string())?;
    if let Some(id) = original_id.as_deref() {
        write_entry(
            root,
            &hist,
            id,
            draft_id,
            base_seq + 1,
            &proposal.path,
            DraftHistorySource::Original,
            prior.as_bytes(),
        )?;
    }
    write_entry(
        root,
        &hist,
        &entry_id,
        draft_id,
        seq,
        &proposal.path,
        DraftHistorySource::ProposalAccepted {
            proposal_id: proposal.id.clone(),
            hunk_id: None,
            agent: proposal.agent.clone(),
        },
        normalised.as_bytes(),
    )?;
    drafts::save_prompt_at(root, &dir, draft_id, &proposal.path, &candidate.content)?;
    draft_proposals::set_decided(root, draft_id, &proposal.id, ProposalState::Accepted)?;
    root.write_toml_atomic(hist.join(JOURNAL_FILE), &journal)
        .map_err(|e| e.to_string())?;
    Ok(journal)
}

/// The entry id a staged operation minted, for a test in another module.
#[cfg(test)]
pub(crate) fn journal_entry_id(journal: &Journal) -> Option<String> {
    journal.entry_id.clone()
}

/// The id of the `Original` a staged first acceptance minted (DHS-FR-07), for a
/// test in another module.
#[cfg(test)]
pub(crate) fn journal_original_entry_id(journal: &Journal) -> Option<String> {
    journal.original_entry_id.clone()
}

/// [`stage_for_test`] stopped before the commit point (DHS-FR-14).
#[cfg(test)]
pub(crate) fn stage_prepared_for_test(
    root: &fs::RootFs,
    draft_id: &str,
    proposal: &DraftChangeProposal,
    prior: &str,
    by: &Participant,
) -> Result<Journal, String> {
    stage_for_test(root, draft_id, proposal, prior, by, Phase::Prepared)
}

/// [`stage_for_test`] stopped after the commit point and before the decision
/// comment was appended (DHS-FR-17).
#[cfg(test)]
pub(crate) fn stage_committed_for_test(
    root: &fs::RootFs,
    draft_id: &str,
    proposal: &DraftChangeProposal,
    prior: &str,
    by: &Participant,
) -> Result<Journal, String> {
    stage_for_test(root, draft_id, proposal, prior, by, Phase::Committed)
}

#[cfg(test)]
mod tests;

/// The acceptance transaction: the journal, the entries it writes, the commit
/// point, the rollback, and the reconciliation that finishes an interrupted
/// one. Split out because it is one thing read end to end, and reading it
/// among the list and the commands is what makes it hard to check.
mod transaction;
pub use transaction::*;
