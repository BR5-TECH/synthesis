//! Draft change proposals (`DCP-draft-change-proposals.md`).
//!
//! A **proposal** is a complete alternative text for one file of one draft,
//! offered by an agent in that draft's conversation
//! (`crate::tools::propose_draft_changes`) and held beside the draft until the
//! author accepts it or declines it. It exists because a collaborator that can
//! see how a passage would be better should be able to show the author the whole
//! of what it would write rather than describe it — and because an agent that
//! edits the author's working material without being asked is not a
//! collaborator.
//!
//! On-disk layout, inside the draft's own folder (DRS-FR-01):
//!
//! ```text
//! .synthesis/drafts/<draft-id>/proposals/
//!   <proposal-id>.toml      — the record
//!   <proposal-id>.content   — the proposed text, verbatim
//! ```
//!
//! Two files rather than one, because the text is a document and a record is not
//! (DCP-FR-01): a record carrying a document in a field would be read whole on
//! every list, and listing is what a tab does on open.
//!
//! ## What this module does not do
//!
//! It does not **apply** a proposal by writing a draft file itself. An acceptance
//! composes the content and hands it to `crate::drafts`' own save path
//! (DRS-FR-12), which stays the only write path for a draft file's contents — so
//! an accepted rewrite refreshes `updated_at`, emits `drafts-changed`, and
//! reaches the indexer exactly as the author's own typing does, rather than
//! through a second route that would have to remember to do all three.
//!
//! It also decides nothing on its own (DCP-FR-19). There is no timer, no queue,
//! and no threshold anywhere in it: a proposal stands until the author decides it
//! or until the draft goes, which is what makes one left for a month cost exactly
//! what one decided immediately costs.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Emitter;

use crate::comments::{self, Attachment, Participant, ThreadRef};
use crate::drafts;
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use crate::notes::{new_note_id, now_rfc3339};

/// Suffix of the record half of a proposal.
const RECORD_EXT: &str = "toml";
/// Suffix of the document half.
const CONTENT_EXT: &str = "content";
/// DCP-FR-01: the ordered hunk bodies and their anchors.
const HUNKS_EXT: &str = "hunks";

// ---------------------------------------------------------------------------
// Typed errors (DCP contract surface)
// ---------------------------------------------------------------------------

pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
/// DCP-FR-17: decided once, and a second decision finds it settled.
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
/// DCP-FR-18: the file the proposal would replace is no longer in the draft.
pub const ERR_PATH_MISSING: &str = "path_missing";
/// DCP-FR-18: the text a change names is no longer in the prompt.
pub const ERR_ANCHOR_LOST: &str = "anchor_lost";
/// DCP-FR-JGCD: the text a change names is in the prompt more than once, so it
/// names no one place. Told apart from a text the prompt does not hold at all,
/// because the corrections are opposites: copy the text again, against copy more
/// of the words around it.
pub const ERR_HUNK_AMBIGUOUS: &str = "hunk_ambiguous";
/// DCP-FR-22: `hunk_id` names no change of this proposal.
pub const ERR_HUNK_NOT_FOUND: &str = "hunk_not_found";
/// DCP-FR-17: that change is already accepted or rejected.
pub const ERR_HUNK_ALREADY_DECIDED: &str = "hunk_already_decided";
/// DCP-FR-27: the stored candidate has moved on since the caller read it.
pub const ERR_CANDIDATE_STALE: &str = "candidate_stale";
/// DCP-FR-13: the draft file could not be written; the proposal stays pending.
pub const ERR_WRITE_FAILED: &str = "write_failed";

/// AGC-FR-05's spellings, which a dispatched turn's origin is named by.
pub const ORIGIN_DRAFT_DISCUSSION: &str = "draft_discussion";
pub const ORIGIN_DRAFT_COMMENT: &str = "draft_comment";

/// DCP-FR-07: the refusals `record_proposal` reports, which
/// `crate::tools::propose_draft_changes` renders as its own (PDC-FR-05,
/// PDC-FR-07, PDC-FR-08, PDC-FR-09).
///
/// A typed enum rather than the string errors the Tauri commands return, because
/// the tool has to tell them apart to pick a refusal message and a retryable
/// flag, and matching on prose would break the moment a sentence was edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordRefusal {
    /// DCP-FR-04: the draft already carries an undecided proposal.
    ProposalPending,
    /// DCP-FR-07: `path` names no file the draft currently holds.
    PathMissing,
    /// DCP-FR-07: the proposed changes leave the file as it is.
    NoChange,
    /// DCP-FR-07: no changes at all.
    NoHunks,
    /// DCP-FR-JGCD: a `before` the prompt does not hold. Carries the one-based
    /// place of the change in the list the caller passed, and how many it
    /// passed — a proposal of eight changes refused for one of them cannot be
    /// corrected by a caller that is not told which.
    HunkAnchorLost { at: usize, of: usize },
    /// DCP-FR-JGCD: a `before` naming more than one place, and which change.
    HunkAmbiguous { at: usize, of: usize },
    /// DCP-FR-JGCD: two changes covering the same text, and the later of them.
    HunkOverlap { at: usize, of: usize },
    /// CMS-FR-17: the conversation takes no further comments.
    ThreadLocked,
    /// Anything else — a write that failed, a draft that is not there.
    NotRecorded,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// A proposal id is a bare, filename-safe token, on exactly the terms a draft id
/// is (DRS-FR-16): it is joined into a path, so a separator or a dot segment
/// would name a file elsewhere inside `.synthesis/` even though `resolve_under`
/// refuses to leave the root.
pub fn is_valid_proposal_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{RECORD_EXT}"))
}

fn content_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{CONTENT_EXT}"))
}

fn hunks_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{HUNKS_EXT}"))
}

// ---------------------------------------------------------------------------
// Reads (pure over a root, so they unit-test without a Tauri runtime)
// ---------------------------------------------------------------------------

/// DCP-FR-25: write the author's own text over a **pending** proposal's
/// candidate, and nothing else anywhere.
///
/// The whole reason the review surface can promise that a candidate under
/// revision has cost the draft nothing (DCR-FR-26): not one file under `files/`
/// is touched, `updated_at` is not refreshed, no `"drafts changed"` is emitted,
/// the indexing channel carries nothing, and neither decision operation is
/// called. It writes exactly one file.
pub fn save_candidate_impl(
    root: &fs::RootFs,
    proposal_id: &str,
    content: &str,
    baseline_checksum: &str,
) -> Result<(String, CandidateSaved), String> {
    let (draft_id, mut record) =
        find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    // DCP-FR-26: a candidate is the author's to shape only while the decision it
    // is for is still owed. The text a decided proposal holds is the record of
    // what was decided.
    if record.state != ProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    let prompt = drafts::require_prompt(root, &draft_id)
        .and_then(|path| drafts::load_draft_file_impl(root, &draft_id, &path))
        .map(|contents| contents.body)
        .unwrap_or_default();

    // DCP-FR-27: checked before anything is written, so a stale save leaves the
    // proposal byte-for-byte as it was and the caller resolves the divergence
    // explicitly rather than discovering it after an overwrite.
    let (held, _legacy) = read_hunk_document(root, &dir, proposal_id, &prompt)?;
    let current = hunks::apply_all(&prompt, &held)
        .map(|text| crate::fs::sha256_bytes(text.as_bytes()))
        .ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    if current != baseline_checksum {
        return Err(ERR_CANDIDATE_STALE.to_string());
    }

    // DCP-FR-25: the author's own text over the agent's, written as the one
    // change it is. A whole document rewritten by hand is exactly a replacement
    // of everything the prompt holds.
    let next = hunks::HunkDocument::new(
        hunks::plan(&prompt, &hunks::whole_document(&prompt, content), || {
            held.hunks.first().map(|h| h.id.clone()).unwrap_or_else(new_note_id)
        })
        .map_err(|_| ERR_WRITE_FAILED.to_string())?
        .hunks,
    );
    root.write_text_atomic(&hunks_path(&dir, proposal_id), &next.to_toml())
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);
    let checksum = crate::fs::sha256_bytes(content.as_bytes());
    record.hunk_count = next.hunks.len() as u32;
    record.ledger = next.hunks.iter().map(hunks::HunkLedgerRow::new).collect();
    record.counts = hunks::HunkCounts::of(record.ledger.iter().map(|row| row.state));

    // DCP-FR-25: the flag describes the *text*, so an edit that restores the
    // agent's own bytes exactly clears it again. A record written before
    // `origin_checksum` existed has nothing to compare against, and its flag is
    // left wherever it already stood rather than being asserted from a guess.
    if let Some(origin) = record.origin_checksum.as_deref() {
        let edited = checksum != origin;
        {
            record.candidate_edited = edited;
            record.ledger.iter_mut().for_each(|row| row.edited = edited);
            // A record that will not write leaves the candidate written and the
            // flag stale. That is the harmless way round — the flag drives a
            // line of prose, while the candidate is the author's work — so the
            // save still succeeds and the miss is recorded by the caller.
            let _ = root.write_toml_atomic(&record_path(&dir, proposal_id), &record);
        }
    }

    Ok((draft_id, CandidateSaved { checksum }))
}

/// DRP-FR-19: whether the draft carries an undecided proposal.
///
/// Cheaper than [`pending_for`] and used where only the fact is wanted — the
/// Drafts panel's row marker, read once per draft inside `list_drafts`' own walk
/// (DRS-FR-08). It stops at the first pending record rather than reading them
/// all, and a draft with no proposals folder answers `false` without a read.
pub fn has_pending(root: &fs::RootFs, draft_id: &str) -> bool {
    let Ok(dir) = drafts::draft_proposals_dir(root, draft_id) else {
        return false;
    };
    has_pending_in(root, &dir)
}

/// [`has_pending`] for a caller that already holds the draft's own directory.
///
/// `list_drafts`' walk has just found it (DRS-FR-08), and resolving a draft id
/// back to a directory now means walking the drafts root again — which would
/// make the panel's one list call quadratic in the number of drafts.
pub fn has_pending_at(root: &fs::RootFs, draft_dir: &std::path::Path) -> bool {
    has_pending_in(root, &draft_dir.join(drafts::PROPOSALS_DIR))
}

fn has_pending_in(root: &fs::RootFs, dir: &std::path::Path) -> bool {
    let dir = dir.to_path_buf();
    let Ok(entries) = root.list_dir(&dir) else {
        return false;
    };
    entries.into_iter().any(|entry| {
        entry
            .name
            .strip_suffix(&format!(".{RECORD_EXT}"))
            .is_some_and(|id| {
                is_valid_proposal_id(id)
                    && root
                        .read_toml::<DraftChangeProposal>(&record_path(&dir, id))
                        .is_ok_and(|p| p.state == ProposalState::Pending)
            })
    })
}

/// DCP-FR-04: the draft's undecided proposal, if it has one.
pub fn pending_for(root: &fs::RootFs, draft_id: &str) -> Option<DraftChangeProposal> {
    list_proposals_impl(root, draft_id)
        .ok()?
        .into_iter()
        .find(|p| p.state == ProposalState::Pending)
}

/// What a change id a model named can mean on the draft as it stands
/// (`../specifications/tools/PDC-propose-draft-changes-tool.md` PDC-FR-GMWR).
///
/// Three answers rather than two, because the two ways a name can fail call for
/// different corrections: a draft holding a standing proposal that does not hold
/// that change is a name the model can correct, and a draft holding no proposal
/// at all is a name that could not have meant anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionTarget {
    /// The change is one of the standing proposal's and awaits the author.
    Revisable,
    /// A proposal stands and holds no such undecided change.
    NoSuchChange,
    /// The draft holds no standing proposal, so no name could have meant one.
    NothingStanding,
}

/// DCP-FR-XDRV: whether a change id names something the author has still to
/// decide on this draft.
pub fn revision_target(root: &fs::RootFs, draft_id: &str, hunk_id: &str) -> RevisionTarget {
    let Some(mut record) = pending_for(root, draft_id) else {
        return RevisionTarget::NothingStanding;
    };
    // DCP-FR-QLMH: a proposal an older build recorded carries no ledger, and its
    // one change is looked up by the row this gives it. Without the backfill the
    // lookup below finds nothing whatever the model names, so the one change
    // actually standing reads as a change that is not there.
    if let Ok(dir) = drafts::draft_proposals_dir(root, draft_id) {
        let prompt = drafts::require_prompt(root, draft_id)
            .and_then(|path| drafts::load_draft_file_impl(root, draft_id, &path))
            .map(|contents| contents.body)
            .unwrap_or_default();
        reads::backfill_legacy_ledger(root, &dir, &mut record, &prompt);
    }
    let revisable = record
        .ledger
        .iter()
        .any(|row| row.id == hunk_id && row.state.is_undecided());
    if revisable {
        RevisionTarget::Revisable
    } else {
        RevisionTarget::NoSuchChange
    }
}

// ---------------------------------------------------------------------------
// Recording (DCP-FR-05)
// ---------------------------------------------------------------------------

/// What a caller hands `record_proposal`. A struct rather than seven positional
/// arguments, four of which are strings that would transpose silently.
pub struct NewProposal<'a> {
    pub draft_id: &'a str,
    pub path: &'a str,
    /// DCP-FR-HRQN: the changes, in the order they should be read. Each names
    /// the text it changes rather than a position in the file.
    pub hunks: &'a [hunks::ProposedHunk],
    pub rationale: &'a str,
    pub agent: &'a Participant,
    pub target: ThreadRef<'a>,
    pub thread_id: &'a str,
}

/// Which way a decision went, and the body its comment carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Accept,
    Decline,
}

impl Decision {
    /// DCP-FR-15: what the conversation records. Prose rather than a marker,
    /// because a reader scrolling the thread months later reads the offer and
    /// the answer in sequence and should not have to open anything.
    /// DCP-FR-25: an agent reading the thread to learn what happened is owed the
    /// difference between "your text landed" and "your text was rewritten and
    /// that landed", so the sentence says which candidate was decided.
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

    fn state(self) -> ProposalState {
        match self {
            Decision::Accept => ProposalState::Accepted,
            Decision::Decline => ProposalState::Rejected,
        }
    }
}

/// DCP-FR-15: the counts the ledger holds once this decision has settled it.
///
/// Computed rather than read off the record, because the accepting path composes
/// its comment before the transaction it delegates has moved a single row
/// (DCP-FR-11). It applies the same rule the decline path applies: a decision
/// naming one change settles that change, and one naming none settles every
/// change still undecided (DCP-FR-14).
fn counts_after(
    record: &DraftChangeProposal,
    decision: Decision,
    hunk: Option<&str>,
) -> hunks::HunkCounts {
    // `decide` backfills a legacy record's ledger before it reaches here
    // (DCP-FR-QLMH), so an empty one means the hunk document could not be read at
    // all. Such a proposal is still one change to the author, and the decision
    // they just made settled it.
    if record.ledger.is_empty() {
        let mut counts = hunks::HunkCounts::default();
        match decision {
            Decision::Accept => counts.accepted = 1,
            Decision::Decline => counts.rejected = 1,
        }
        return counts;
    }
    let settled = match decision {
        Decision::Accept => hunks::HunkState::Accepted,
        Decision::Decline => hunks::HunkState::Rejected,
    };
    hunks::HunkCounts::of(record.ledger.iter().map(|row| {
        let settles = match hunk {
            Some(hunk_id) => row.id == hunk_id,
            None => row.state.is_undecided(),
        };
        if settles { settled } else { row.state }
    }))
}

/// DCP-FR-15: the decision comment's body — the sentence, the tally of what the
/// author accepted and rejected, then their feedback where they wrote any.
fn decision_body(
    decision: Decision,
    path: &str,
    edited: bool,
    counts: &hunks::HunkCounts,
    feedback: Option<&str>,
) -> String {
    let sentence = decision.sentence(path, edited);
    let tally = format!("{} accepted, {} rejected.", counts.accepted, counts.rejected);
    let sentence = format!("{sentence} {tally}");
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

/// Where a proposal's comment lives: the anchored file it is on, or `None` for a
/// discussion — together with the origin kind that thread's target decides
/// (AGC-FR-05).
///
/// Returned as an owned path rather than as a [`ThreadRef`], which borrows: the
/// decision path has to hold this across a mutation of the record, and a
/// reconciliation reconstructs it from disk with no record in hand at all. A
/// proposal is always in a draft conversation (PDC-FR-06), so it is one of the
/// draft's two shapes.
fn locate_thread(
    store: &fs::RootFs,
    draft_id: &str,
    thread_id: &str,
) -> Result<(Option<String>, &'static str), String> {
    let target = comments::DiscussionTarget::Draft {
        draft_id: draft_id.to_string(),
    };
    let found = comments::locate_discussion_of(store, &target, thread_id)
        .ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?;
    Ok(match found.fragment_target {
        Some(fragment) => (Some(fragment.path), ORIGIN_DRAFT_COMMENT),
        None => (None, ORIGIN_DRAFT_DISCUSSION),
    })
}

/// [`locate_thread`]'s answer as the [`ThreadRef`] the write paths take.
fn thread_ref<'a>(draft_id: &'a str, file_rel: Option<&'a String>) -> ThreadRef<'a> {
    match file_rel {
        None => ThreadRef::discussion(draft_id),
        Some(rel) => ThreadRef::draft_file(draft_id, rel),
    }
}

// ---------------------------------------------------------------------------
// What the acceptance transaction reaches back for (DHS-FR-15, DHS-FR-19)
//
// `crate::draft_history` owns the transaction an acceptance is performed as, and
// two of its steps are writes to *this* module's storage: moving the record to
// `accepted` at the commit point, and putting it back to `pending` when an
// uncommitted operation is rolled back. They live here rather than there because
// the record's shape and its folder are this module's, and because a
// reconciliation running after a relaunch has no caller left to ask.
// ---------------------------------------------------------------------------

/// DHS-FR-20: append the decision comment for a committed acceptance, under the
/// ids the journal minted so a repeat contributes nothing a second time.
///
/// The thread is located from the record rather than carried in, because a
/// `ThreadRef` borrows and a reconciliation reconstructs one from disk.
pub(crate) fn append_decision_comment(
    store: &fs::RootFs,
    draft_id: &str,
    thread_id: &str,
    comment_id: String,
    event_id: String,
    body: String,
    by: &Participant,
) -> Result<comments::Discussion, String> {
    let (file_rel, _) = locate_thread(store, draft_id, thread_id)?;
    comments::append_decision_comment_event(
        store,
        thread_ref(draft_id, file_rel.as_ref()),
        thread_id,
        event_id,
        comment_id,
        body,
        by,
        &now_rfc3339(),
    )
}

/// One decision, whichever way it went.
///
/// The two differ in one respect that matters: an acceptance is a **transaction**
/// — the prompt's new bytes, the version it records in the draft's history, the
/// proposal's accepted state, and the decision comment, recoverable together —
/// which this module delegates whole to `crate::draft_history` (DCP-FR-11,
/// DHS-FR-15) rather than sequencing itself. A decline writes nothing into the
/// draft (DCP-FR-14), so its one act is the comment, appended here.
///
/// What is common to both, and what stands **before** either path, is the set of
/// checks that would otherwise make the decision impossible half-way through
/// (DCP-FR-15): an identity that does not resolve and a locked conversation are
/// each refused up front, with the prompt untouched, no history entry, and the
/// proposal still pending.
fn decide<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    decision: Decision,
    // DCP-FR-11 / DCP-FR-14: the one change this decision settles, or `None`
    // for a decision about the whole proposal.
    hunk: Option<&str>,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<Decided, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (draft_id, _) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    // DRS-FR-QPSC: the accepted-proposal write of DHS-FR-15 refuses a
    // GitHub-shadow draft and changes nothing. A decline writes no prompt.
    if decision == Decision::Accept {
        drafts::require_not_github_shadow(root, &draft_id)?;
    }
    // DCP-FR-17 / DHS-FR-23: held for the whole decision — the already-decided
    // check below and the writes that follow it are one act, and the
    // reconciliation between them must not race an acceptance running in
    // another window. Everything under it therefore uses the `_locked` variants,
    // which is also what keeps this from deadlocking against itself.
    // DCP-FR-17 / DHS-FR-23: held for the whole decision — the already-decided
    // check below and the writes that follow it are one act, and the
    // reconciliation between them must not race an acceptance running in
    // another window. Everything under it therefore uses the `_locked` variants,
    // which is also what keeps this from deadlocking against itself.
    let _guard = draft_lock(&draft_id);
    // DCP-FR-28: reconciled before the proposal is reported in any state, so one
    // whose acceptance never committed reads `pending` again and one whose
    // acceptance did reads `accepted` with its comment appended. Re-read
    // afterwards, deliberately: the reconciliation may have moved the very
    // record this decision is about.
    let reconciled = crate::draft_history::reconcile_locked(app, root, store, &draft_id)?;
    let (_, mut record) = find_proposal(root, proposal_id).ok_or(ERR_PROPOSAL_NOT_FOUND)?;
    let (file_rel, origin_kind) = locate_thread(store, &draft_id, &record.thread_id)?;
    // DCP-FR-17: decided once. A second decision arriving from a second window
    // finds it settled rather than overwriting what the first decided.
    if record.state != ProposalState::Pending {
        // …with one exception, and it is not a second decision at all. An
        // acceptance whose transaction committed and whose decision comment
        // could not be appended is reported as landed-but-incomplete
        // (DHS-FR-17), and the author's retry arrives here. The reconciliation
        // above has just appended the line that was owed, so this answers with
        // that comment rather than with `already_decided` — which would leave
        // the surface holding a settled proposal and no way to tell the agent
        // what was decided (DCR-FR-15, DCR-FR-16).
        if let crate::draft_history::Reconciliation::RolledForward {
            proposal_id: completed,
            comment_id,
        } = &reconciled
        {
            if completed == proposal_id && record.state == ProposalState::Accepted {
                return Ok(Decided {
                    proposal: record,
                    comment_id: Some(comment_id.clone()),
                    origin_kind,
                    comment_failed: None,
                });
            }
        }
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    let dir = drafts::draft_proposals_dir(root, &draft_id)?;
    // DCP-FR-QLMH: a legacy proposal is decided on the ordinary terms, so it
    // needs the one ledger row its one change is looked up by.
    backfill_legacy_ledger(root, &dir, &mut record, &prompt_of(root, &draft_id));

    // DCP-FR-17: a change is decided once, exactly as a proposal is.
    if let Some(hunk_id) = hunk {
        let row = record
            .ledger
            .iter()
            .find(|row| row.id == hunk_id)
            .ok_or(ERR_HUNK_NOT_FOUND)?;
        if !row.state.is_undecided() {
            return Err(ERR_HUNK_ALREADY_DECIDED.to_string());
        }
    }
    // DCP-FR-15: only the decision that leaves nothing undecided appends a
    // comment. One comment per change would bury the conversation and give an
    // agent seven turns to answer where one will do.
    let resolves = match hunk {
        None => true,
        Some(hunk_id) => !record
            .ledger
            .iter()
            .any(|row| row.id != hunk_id && row.state.is_undecided()),
    };

    // DCP-FR-18: the draft may have been renamed while the proposal stood, which
    // renames the one file it holds. A proposal replaces the prompt rather than
    // restoring one, so this refuses and leaves the proposal pending — declining
    // is how such a one is cleared. Checked for both decisions, because it is a
    // fact about the proposal rather than about the write.
    let prompt = drafts::require_prompt(root, &draft_id)?;
    if decision == Decision::Accept && record.path != prompt {
        return Err(ERR_PATH_MISSING.to_string());
    }
    let prompt_body = drafts::load_draft_file_impl(root, &draft_id, &prompt)
        .map(|contents| contents.body)
        .unwrap_or_default();

    // DCP-FR-15: the two conditions that would make the append impossible are
    // checked **before anything is written**, so a decision that cannot be
    // recorded is not taken and the prompt, the history, and the proposal's state
    // are all exactly what they were. The identity has already been resolved by
    // the command; the lock is this.
    if comments::thread_at(
        store,
        thread_ref(&draft_id, file_rel.as_ref()),
        &record.thread_id,
    )?
    .locked
    {
        return Err(comments::ERR_DISCUSSION_LOCKED.to_string());
    }

    if decision == Decision::Accept {
        let (doc, _legacy) = read_hunk_document(root, &dir, proposal_id, &prompt_body)?;
        // DCP-FR-12: one change applies to the range it resolves to and leaves
        // every other byte of the prompt as it was. A whole-proposal acceptance
        // walks them all.
        let candidate = match hunk {
            None => hunks::apply_all(&prompt_body, &doc).ok_or(ERR_ANCHOR_LOST)?,
            Some(hunk_id) => {
                let one = doc.find(hunk_id).ok_or(ERR_HUNK_NOT_FOUND)?;
                let before = anchors::normalise_newlines(one.before_text());
                let at = anchors::resolve(
                    &anchors::normalise_newlines(&prompt_body),
                    one.kind,
                    &before,
                    &one.anchor,
                );
                // DCP-FR-18: re-resolved here against disk rather than trusting
                // what the surface last showed.
                hunks::apply_hunk(&prompt_body, one, at).ok_or(ERR_ANCHOR_LOST)?
            }
        };
        // DCP-FR-11 / DCP-FR-21: this module writes neither into `files/` nor
        // into `history/`, and is not the writer that moves its own record to
        // `accepted` — the transaction it delegates is all three.
        let outcome = crate::draft_history::apply_acceptance_locked(
            app,
            root,
            store,
            crate::draft_history::Acceptance {
                proposal: &record,
                hunk_id: hunk.map(str::to_string),
                resolves,
                candidate: &candidate,
                comment_body: decision_body(
                    decision,
                    &record.path,
                    record.candidate_edited,
                    &counts_after(&record, decision, hunk),
                    feedback,
                ),
                comment_by: by,
            },
        )?;
        return Ok(Decided {
            proposal: outcome.proposal,
            comment_id: outcome.comment_id,
            origin_kind,
            comment_failed: None,
        });
    }

    // DCP-FR-14: a decline moves the record to `rejected` and writes nothing into
    // the draft — no prompt write, no history entry, no transaction begun — and
    // keeps the `.content` file, so a declined proposal can still be read back
    // afterwards as the candidate the author declined.
    //
    // The record moves before the comment is appended. Each can fail, and this
    // ordering makes both failures recoverable: a record that will not write
    // leaves a proposal still pending with nothing said, which the author's next
    // click repairs; a comment that will not append leaves the decision made and
    // the conversation one line short, which DCP-FR-15 contemplates and the
    // caller logs. The reverse order has a state neither recovers from — a
    // conversation saying the proposal was declined while the record still reads
    // pending, so the modal keeps offering a decision the thread says was made.
    // DCP-FR-14: a decline naming one change rejects that change; one naming
    // none rejects every change still undecided, which is the escape hatch a
    // draft needs when a change is held for discussion indefinitely.
    let now = now_rfc3339();
    for row in record.ledger.iter_mut() {
        let settles = match hunk {
            Some(hunk_id) => row.id == hunk_id,
            None => row.state.is_undecided(),
        };
        if settles {
            row.state = hunks::HunkState::Rejected;
            row.decided_at = Some(now.clone());
        }
    }
    if record.ledger.is_empty() {
        record.state = decision.state();
        record.decided_at = Some(now.clone());
    } else {
        resettle(&mut record);
    }
    root.write_toml_atomic(record_path(&dir, proposal_id), &record)
        .map_err(|e| e.to_string())?;
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);

    // DCP-FR-15: only the decision that leaves nothing undecided appends a
    // comment. A rejection of one change among several settles nothing the author
    // has finished thinking about, and a comment for it would dispatch a fresh
    // turn at the agent in the middle of its own pass (DCR-FR-15).
    if !resolves {
        return Ok(Decided {
            proposal: record,
            comment_id: None,
            origin_kind,
            comment_failed: None,
        });
    }

    // Minted here so the caller can name it as the fresh turn's trigger
    // (DCR-FR-15) without re-folding the thread to find what was just appended.
    let comment_id = new_note_id();
    let appended = comments::add_comment_with_id(
        store,
        thread_ref(&draft_id, file_rel.as_ref()),
        &record.thread_id,
        comment_id.clone(),
        decision_body(
            decision,
            &record.path,
            record.candidate_edited,
            &counts_after(&record, decision, hunk),
            feedback,
        ),
        by,
        &now_rfc3339(),
    );
    let comment_error = match appended {
        // CMS-FR-51: the decision's own comment reaches the conversation's
        // surface on exactly the terms the proposal's did. Announced here rather
        // than by the two commands, so the one place that knows whether an
        // append happened is the one place that announces it — and so it is
        // reachable from a test at all.
        Ok(thread) => {
            // AGC-FR-31 / CTA-FR-RUVS: the author's decision is a human-authored
            // comment, so it retires this conversation's offer to retry a failed
            // turn exactly as any other human comment does. Here as well as in
            // `comments::add_comment` because this path appends through
            // `add_comment_with_id` rather than through that command — the two
            // are the only human write paths there are, and an offer left
            // standing behind a decision would outlive the question it was about.
            crate::agent_conversations::retire_recoverable_for_human_comment(
                app,
                &record.thread_id,
            );
            comments::emit_discussion_changed(app, &thread);
            None
        }
        Err(error) => Some(error),
    };

    Ok(Decided {
        proposal: record,
        comment_id: comment_error.is_none().then_some(comment_id),
        origin_kind,
        comment_failed: comment_error,
    })
}

/// [`decide`] for a test in another module.
///
/// `crate::agent_conversations`' tests need an author to actually accept a
/// proposal, which is what dispatches the fresh turn AGC-FR-29 is about. The
/// commands that would do it resolve a GitHub identity through Tauri state that
/// harness does not manage, so this exposes the decision itself with the
/// participant supplied — exactly what the commands do once they have resolved
/// one.
#[cfg(test)]
pub fn decline_for_test<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    by: &Participant,
) -> Result<Decided, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide(app, root, store, proposal_id, Decision::Decline, None, None, by)
}

/// [`accept_draft_change_hunk`] for a test that holds no Tauri `State`.
///
/// Unlike [`accept_for_test`] this names the change, which is what the command
/// always does (DCP-FR-11) — so the ledger row moves and the record's counts
/// settle, exactly as they do for an author clicking Accept.
#[cfg(test)]
pub fn accept_hunk_for_test<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
    by: &Participant,
) -> Result<Decided, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    decide(app, root, store, proposal_id, Decision::Accept, Some(hunk_id), None, by)
}

/// [`decide`] for a test in another module.
#[cfg(test)]
pub fn accept_for_test<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    proposal_id: &str,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<Decided, String> {
    decide(app, root, store, proposal_id, Decision::Accept, None, feedback, by)
}

/// A decision that landed, and whether the conversation got to hear about it.
///
/// The two are separate because they fail separately (DCP-FR-15): the record
/// moving is what makes the decision final, and a comment that could not be
/// appended — a conversation locked between the author reading the diff and
/// deciding it, most plausibly — must not undo an acceptance whose file has
/// already been written. The caller logs the miss rather than surfacing it,
/// there being nothing the author could now do about it.
#[derive(Debug, Clone)]
pub struct Decided {
    pub proposal: DraftChangeProposal,
    /// DCR-FR-15: the decision's own comment, which is what addressed the agent
    /// and therefore what the fresh turn names as its trigger (AGC-FR-29).
    /// Absent when the append failed.
    pub comment_id: Option<String>,
    /// DCR-FR-15: which of the two draft origin kinds the decision landed in.
    pub origin_kind: &'static str,
    pub comment_failed: Option<String>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// DCP-FR-16: a proposal was recorded or decided.
///
/// Kebab-case for the reason every event in this application is: Tauri rejects
/// the rest of the character set at `emit`, leaving a channel that is silently
/// dead in production.
pub const PROPOSALS_CHANGED: &str = "draft-change-proposals-changed";

/// What the event carries (DCP-FR-16): the draft, and the whole proposal as it
/// then stood, so a consumer redraws from the payload without a read of its own.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalsChanged {
    pub draft_id: String,
    pub proposal: DraftChangeProposal,
}

/// DCP-FR-16 / DCP-FR-24: announce the change and record it.
///
/// Best-effort on the emit, for the reason every emit site here discards its
/// result: a proposal that was written must not be reported as failed because
/// the notification of it could not be delivered.
pub(crate) fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
    proposal: &DraftChangeProposal,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _ = app.emit(
        PROPOSALS_CHANGED,
        ProposalsChanged {
            draft_id: proposal.draft_id.clone(),
            proposal: proposal.clone(),
        },
    );
    // DCP-FR-WTKA: a proposal that reaches a **final decision** contributes
    // exactly one `proposal_decision` line to that draft's statistics log,
    // appended from the same point this event is emitted from — so an
    // acceptance contributes it after its transaction has committed, a
    // rolled-back acceptance contributes none, and a proposal that is still
    // `pending` contributes none. An acceptance counts even where it changed no
    // prompt byte and created no history entry: an accepted proposal is a
    // decision the author took. The append is asynchronous and cannot fail the
    // decision (DSS-FR-TUMX), and a line appended twice by a resumed operation
    // folds to one on the proposal's own identity (DSS-FR-PNUE).
    let decision = match proposal.state {
        ProposalState::Pending => None,
        ProposalState::Accepted => Some(crate::statistics::Decision::Accepted),
        ProposalState::Rejected => Some(crate::statistics::Decision::Rejected),
    };
    if let Some(decision) = decision {
        crate::statistics::record_statistics_event(
            app,
            &proposal.draft_id,
            crate::statistics::EventBody::ProposalDecision {
                proposal_id: proposal.id.clone(),
                decision,
            },
        );
    }
    // DCP-FR-24: the draft, the proposal, the path, and the outcome — and never
    // the proposed text, the file's current text, the rationale, or any feedback,
    // all of which are material a model or an author composed.
    logging::log_info(
        app,
        buffer,
        &[Domain::Backend],
        "draft change proposal changed",
        log_fields! {
            "draftId" => &proposal.draft_id,
            "proposalId" => &proposal.id,
            "path" => &proposal.path,
            "state" => proposal.state.as_str(),
        },
    );
}

/// DCP-FR-24: one `WARN` naming what refused and why.
fn log_refusal<R: tauri::Runtime>(app: &tauri::AppHandle<R>, operation: &'static str, reason: &str)
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "draft change proposal refused",
        log_fields! { "operation" => operation, "reason" => reason },
    );
}

#[cfg(test)]
mod tests;

/// Settling a record and the rows of its ledger — every write about where a
/// proposal and its changes stand, and nothing about the draft's own files.
mod ledger;
pub use ledger::*;

/// Reading what a draft's proposals are and what they hold. Nothing in there
/// writes anything, which is what lets a surface ask freely.
mod reads;
pub use reads::*;

/// How a proposal comes into existence — the one write path, and the checks it
/// refuses on before anything reaches disk.
mod recording;
pub use recording::*;

/// The record on disk and the shapes the frontend reads it as. Split out
/// because they are a vocabulary rather than behaviour: nothing in there does
/// anything, and everything here does.
mod record;
pub use record::*;

/// The `#[tauri::command]` surface — every operation the frontend invokes about
/// a draft's proposed changes, and nothing else. Split out so the decision
/// engine above is read without the argument-shuffling around it.
mod commands;
pub use commands::*;

pub mod anchors;
pub mod hunks;
