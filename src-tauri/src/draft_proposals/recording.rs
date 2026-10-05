//! Recording a proposal — the **only** path by which one comes into existence
//! (`DCP-draft-change-proposals.md` DCP-FR-05, DCP-FR-JGCD).
//!
//! Everything a proposal has to be true about is decided here, before a byte
//! reaches disk: the draft holds no other undecided proposal, the file is one
//! the draft holds, the changes name text the prompt actually contains, none of
//! them is ambiguous, and no two of them overlap. A refusal at this point is a
//! sentence the agent can act on; the same mistake caught later is a rewrite
//! the author has to notice dropped their words.

use super::*;

/// DCP-FR-05: the **only** path by which a proposal comes into existence.
///
/// Internal by construction — no Tauri command creates one, so no frontend call
/// can record a proposal, forge one against a draft, or attribute one to an
/// agent, exactly as none can attribute a comment to one (CMS-FR-41). The one
/// caller is `crate::tools::propose_draft_changes`.
///
/// DCP-FR-06: the two writes and the append succeed together or leave nothing
/// behind. The content and the record reach disk before the comment is appended,
/// so no folded comment ever names a proposal that is not there; and a failure to
/// append removes both files before returning, so no candidate sits in the folder
/// that no conversation refers to. A record whose comment could not be appended
/// would be a proposal the author is never told about and the agent believes was
/// made — the one state this module refuses to hold.
pub fn record_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    // CMS-FR-01: the candidate is a draft file and lives in the worktree; the
    // comment announcing it is a conversation and lives in the repository
    // machine store.
    store: &fs::RootFs,
    new: NewProposal<'_>,
) -> Result<DraftChangeProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    // DCP-FR-04: the one-pending check below is a read followed by a write, and
    // AGC-FR-03 puts several turns in flight at once — two agents asked about one
    // draft can reach it together, both find no pending proposal, and both
    // record one. The draft would then hold two undecided proposals, of which
    // `pending_for` surfaces exactly one: the other occupies the slot from the
    // tool's point of view while being unreachable from every surface.
    //
    // Serialised here rather than by an exclusive create on the record file,
    // because the check spans three files and a conversation append, not one.
    // The lock is held for the whole of that, and this is the only writer.
    let _guard = draft_lock(new.draft_id);

    drafts::read_draft_record(root, new.draft_id).map_err(|_| RecordRefusal::NotRecorded)?;

    // DCP-FR-07, checked in the order a caller can act on. The pending check is
    // first because it is the one refusal that is about the draft rather than
    // about the arguments, and re-proposing a different file would not help.
    if pending_for(root, new.draft_id).is_some() {
        return Err(RecordRefusal::ProposalPending);
    }
    // DCP-FR-07 / PDC-FR-05: `path` names the draft's own prompt and nothing
    // else — a proposal replaces the prompt and never creates a file, and a
    // draft holds exactly one (DRS-FR-11).
    let current = drafts::load_draft_file_impl(root, new.draft_id, new.path)
        .map_err(|_| RecordRefusal::PathMissing)?;
    // DCP-FR-07: a proposal that changes nothing is nothing to decide. The
    // comparison is against the **normalised** candidate — what the save path
    // would actually put on disk (DRS-FR-12) — because a candidate differing
    // from the prompt only in its line endings would change no byte, and
    // offering the author a diff with nothing in it wastes the one decision a
    // draft may hold at a time.
    // DCP-FR-JGCD: every change is placed against the prompt **before anything
    // is written**, so a model that invented the text it claims to be changing
    // is told while it can still correct it.
    // The count travels with the place, so a refusal names the change as the
    // caller counted it — "change 3 of 8" and not "change 3".
    let of = new.hunks.len();
    let document = hunks::plan(&current.body, new.hunks, new_note_id).map_err(|e| match e {
        hunks::PlanError::NoHunks => RecordRefusal::NoHunks,
        hunks::PlanError::AnchorLost(at) => RecordRefusal::HunkAnchorLost { at, of },
        hunks::PlanError::Ambiguous(at) => RecordRefusal::HunkAmbiguous { at, of },
        hunks::PlanError::Overlap(at) => RecordRefusal::HunkOverlap { at, of },
    })?;
    // DCP-FR-07: changes that together leave the text as it is are nothing to
    // decide, and would spend the draft's one slot on an empty review. The
    // comparison is against the **normalised** result — what the save path would
    // actually put on disk (DRS-FR-12).
    // Not an anchor a caller can be told to correct: `plan` placed every change
    // a moment ago, so a re-application that fails here is an inconsistency of
    // this module rather than a mistake in the arguments. Reported as one, and
    // **without a position** — naming a change would name the first one
    // whatever failed, which is the very thing a caller cannot act on and the
    // reason the places are carried at all.
    let composed =
        hunks::apply_all(&current.body, &document).ok_or(RecordRefusal::NotRecorded)?;
    if current.body == drafts::normalise_for_write(root, &composed) {
        return Err(RecordRefusal::NoChange);
    }

    let dir = drafts::draft_proposals_dir(root, new.draft_id).map_err(|_| RecordRefusal::NotRecorded)?;
    let id = new_note_id();
    let now = now_rfc3339();

    // DCP-FR-08: the hunk bodies exactly as the agent composed them. The bodies
    // live beside the record rather than in it, because listing reads records and
    // never a document (DCP-FR-09).
    root.write_text_atomic(&hunks_path(&dir, &id), &document.to_toml())
        .map_err(|_| RecordRefusal::NotRecorded)?;
    // DCP-FR-25: the agent's own text, so `edited` describes the hunk rather than
    // the history of getting to it — an author who rewrites a hunk and then puts
    // it back letter for letter has an unedited hunk again.
    let origin_checksum = crate::fs::sha256_bytes(composed.as_bytes());
    let base_sha256 = crate::fs::sha256_bytes(current.body.as_bytes());
    let ledger: Vec<hunks::HunkLedgerRow> =
        document.hunks.iter().map(hunks::HunkLedgerRow::new).collect();
    let counts = hunks::HunkCounts::of(ledger.iter().map(|row| row.state));

    // DCP-FR-06: the comment id is minted **here**, before either write, so the
    // record can name the comment it will be announced by without the comment
    // having to exist first. That is what lets both files reach disk ahead of an
    // append that is append-only and cannot be taken back (CMS-FR-04) — the one
    // ordering under which no folded comment can ever name a proposal that is
    // not there.
    let comment_id = new_note_id();
    let record = DraftChangeProposal {
        id: id.clone(),
        draft_id: new.draft_id.to_string(),
        path: new.path.to_string(),
        agent: new.agent.clone(),
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: comment_id.clone(),
        state: ProposalState::Pending,
        origin_checksum: Some(origin_checksum),
        candidate_edited: false,
        legacy: false,
        base_sha256: Some(base_sha256),
        hunk_count: ledger.len() as u32,
        counts,
        ledger,
        created_at: now.clone(),
        decided_at: None,
    };

    let cleanup = || {
        let _ = root.delete_under(&dir, format!("{id}.{HUNKS_EXT}"), false);
        let _ = root.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
    };

    if root.write_toml_atomic(&record_path(&dir, &id), &record).is_err() {
        cleanup();
        return Err(RecordRefusal::NotRecorded);
    }
    // DRS-FR-WYIN / DRS-FR-JDRY: a proposal is private draft storage. The
    // ignore file keeps it out of Git, and no commit names it.
    drafts::git_storage::ensure_private_ignored(root);

    let attachment = Attachment::Proposal {
        proposal_id: id.clone(),
        draft_id: new.draft_id.to_string(),
        path: new.path.to_string(),
    };
    let appended = match comments::append_agent_comment_with(
        store,
        new.target,
        new.thread_id,
        comment_id,
        new.rationale.to_string(),
        vec![attachment],
        new.agent,
        &now,
    ) {
        Ok(thread) => thread,
        Err(error) => {
            // DCP-FR-06: a failure to append removes **both** files, so no
            // candidate sits in the folder that no conversation refers to — and
            // the draft's one pending slot is free again rather than held by a
            // proposal the author was never told about.
            cleanup();
            return Err(if error == comments::ERR_DISCUSSION_LOCKED {
                RecordRefusal::ThreadLocked
            } else {
                RecordRefusal::NotRecorded
            });
        }
    };

    // CMS-FR-51 / DCP-FR-06: the conversation announces itself the moment the
    // offer is in it, rather than at the moment it is answered. An author reading
    // the thread while an agent is working sees the proposal arrive there as they
    // would see any other comment — waiting for the decision to redraw it would
    // leave the one surface that holds the whole exchange showing a conversation
    // that stops before its most recent turn.
    comments::emit_discussion_changed(app, &appended);

    // DCP-FR-16: emitted when a proposal is **recorded**, not only when one is
    // decided. This is what the tab's pending marker, the rail's control, the
    // Drafts panel's row and the shell's notification all redraw from
    // (NAW-FR-35, CMT-FR-67, DRP-FR-19, DCR-FR-18); without it a proposal made
    // during a session reaches no surface until the project is reopened.
    announce(app, &BUFFER, &record);
    Ok(record)
}

/// DCP-FR-04 / DHS-FR-23: **serialises every operation that decides, records, or
/// reconciles one draft's proposals and history.**
///
/// It is one lock rather than two because the operations it guards are not
/// independent, and two of the races are only visible once that is said out loud:
///
/// - two agents recording against one draft can otherwise both pass the
///   one-pending check and leave the draft holding two undecided proposals, of
///   which only one is ever reachable (DCP-FR-04);
/// - two windows deciding one proposal can otherwise both pass the
///   already-decided check (DCP-FR-17) and land two different outcomes over each
///   other — and two concurrent *acceptances* would mint two `event_id`s and
///   append two decision comments for one decision, which the conversation's
///   fold cannot collapse (CMS-FR-06);
/// - and, worst of the three, a **reconciliation** running while an acceptance
///   is between its `prepared` journal and its commit point reads that journal
///   as an abandoned operation and rolls it back — deleting the entry and
///   restoring the prompt underneath a transaction that is still writing. A
///   journal on disk says where an operation got to and nothing at all about
///   whether the process that wrote it is still alive, so the only thing that
///   can tell "crashed" from "still running" is this lock. Reconciliation runs
///   on the ordinary read path (`list_draft_history`, and every command of this
///   module), so the race needs no error condition to reproduce: a second window
///   merely opening the draft is enough.
///
/// One map for the application, entries never removed — a draft's mutex is two
/// words and a session holds a handful of drafts, where reference-counting the
/// entries would cost more than it saves and reintroduce the very race this
/// exists to close.
pub(crate) fn draft_lock(draft_id: &str) -> std::sync::MutexGuard<'static, ()> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mutex = {
        let mut guard = locks.lock().unwrap_or_else(|e| e.into_inner());
        *guard
            .entry(draft_id.to_string())
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Deciding (DCP-FR-11 through DCP-FR-15)
// ---------------------------------------------------------------------------


