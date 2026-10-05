//! Tests for the recording of a proposal and its refusals.

use super::*;
use crate::draft_proposals::HunkPlacement;

// ---------------------------------------------------------------------------
// DCP-FR-02, DCP-FR-08 — the candidate, beside the draft and not in it (DCP-FR-01, FR-02, FR-08)
// ---------------------------------------------------------------------------

#[test]
fn recording_writes_a_candidate_and_leaves_the_draft_file_untouched() {
    let f = Fixture::new();
    let proposal = f.pending();

    let dir = f.proposals_dir();
    assert!(
        dir.join(format!("{}.toml", proposal.id)).is_file(),
        "the record is one file (DCP-FR-01)",
    );
    let document = std::fs::read_to_string(dir.join(format!("{}.hunks", proposal.id)))
        .expect("hunk document");
    assert!(
        document.contains(PROPOSED.trim_end()),
        "the proposed text, verbatim, in the hunk document beside the record (DCP-FR-08)",
    );
    assert_eq!(
        f.file_body(),
        ORIGINAL,
        "recording changes no file of the draft (DCP-FR-01)",
    );
    assert_eq!(proposal.state, ProposalState::Pending);
    assert_eq!(proposal.decided_at, None);
    assert_eq!(proposal.path, FILE);
    assert_eq!(proposal.thread_id, f.thread_id);
}

#[test]
fn a_filed_drafts_candidate_lands_inside_that_draft_wherever_it_is_filed() {
    // DCP-FR-01: the proposals folder is inside the draft's own directory,
    // obtained from the drafts module rather than composed from the draft's id
    // (DRS-FR-36) — so filing the draft moves its candidates with it and leaves
    // nothing at the path an id alone would have named.
    let f = Fixture::in_folder("UI/Components");
    let proposal = f.pending();

    let filed = f
        .root
        .join(".synthesis/drafts/UI/Components")
        .join(&f.draft_id)
        .join("proposals");
    assert_eq!(f.proposals_dir(), filed);
    assert!(filed.join(format!("{}.toml", proposal.id)).is_file());
    assert!(filed.join(format!("{}.hunks", proposal.id)).is_file());
    assert!(
        !f.root.join(".synthesis/drafts").join(&f.draft_id).exists(),
        "no directory named for the draft's id sits at the drafts root",
    );
    assert_eq!(f.file_body(), ORIGINAL, "and no file of the draft changed");
}

// ---------------------------------------------------------------------------
// DCP-FR-04 — one pending per draft (DCP-FR-04, DCP-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn a_second_proposal_is_refused_while_one_is_undecided_and_allowed_once_it_is_decided() {
    let f = Fixture::new();
    let first = f.pending();

    assert_eq!(
        f.propose(FILE, "# Spec\n\nA third opening.\n").unwrap_err(),
        RecordRefusal::ProposalPending,
        "the same file is refused while one stands",
    );
    // A proposal naming anything but the draft's one prompt is refused for a
    // reason of its own (DCP-FR-07), and the pending slot is checked first
    // because it is about the draft rather than about the arguments (DCP-FR-04).
    assert_eq!(
        f.propose("notes.md", "other").unwrap_err(),
        RecordRefusal::ProposalPending,
    );
    assert_eq!(
        list_proposals_impl(&f.root, &f.draft_id).expect("list").len(),
        1,
        "neither refusal left a candidate behind",
    );

    f.decline_with(&first.id, None).expect("declined");
    assert!(
        f.propose(FILE, "# Spec\n\nA third opening.\n").is_ok(),
        "a decided proposal frees the slot (DCP-FR-04)",
    );
}

// ---------------------------------------------------------------------------
// CMS-FR-41, CMS-FR-60 — the comment and its reference (DCP-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn recording_appends_one_agent_comment_carrying_the_proposal_reference() {
    let f = Fixture::new();
    let proposal = f.pending();

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), 2, "the seed comment and the proposal");
    let posted = &thread.comments[1];
    assert_eq!(posted.id, proposal.comment_id, "the record names its comment");
    assert_eq!(
        posted.body, "The opening buries the point.",
        "the body is the rationale and nothing else (PDC-FR-12)",
    );
    match &posted.author {
        Participant::Agent { handle, .. } => assert_eq!(handle, "arch"),
        other => panic!("expected the agent participant, got {other:?}"),
    }
    assert_eq!(posted.attachments.len(), 1, "exactly one (CMS-FR-60)");
    match &posted.attachments[0] {
        comments::Attachment::Proposal {
            proposal_id,
            draft_id,
            path,
        } => {
            assert_eq!(proposal_id, &proposal.id);
            assert_eq!(draft_id, &f.draft_id);
            assert_eq!(path, FILE);
        }
        other => panic!("expected a proposal reference, got {other:?}"),
    }
}

#[test]
fn the_conversation_is_announced_when_the_proposal_is_recorded() {
    // DCP-FR-15 / DCP-FR-06, CMS-FR-51. The offer reaches an open conversation
    // the moment it is made rather than when it is answered: an author reading
    // the thread while an agent works sees the proposal arrive there as they
    // would any other comment.
    let f = Fixture::new();
    let threads = f.watch_threads();
    let proposal = f.pending();

    let seen = threads.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(seen.len(), 1, "one announcement for one append");
    let announced = &seen[0];
    assert_eq!(announced["id"], f.thread_id, "the conversation it landed in");
    let comments = announced["comments"]
        .as_array()
        .expect("the folded thread carries its comments");
    assert_eq!(comments.len(), 2, "the seed comment and the proposal");
    // The payload is the thread as it now stands, so a surface redraws from it
    // without a read of its own.
    assert_eq!(comments[1]["id"], proposal.comment_id);
    assert_eq!(comments[1]["body"], "The opening buries the point.");

    // A refusal announces nothing: nothing was appended for a surface to redraw.
    let before = threads.lock().unwrap_or_else(|e| e.into_inner()).len();
    assert_eq!(
        f.propose(FILE, "a second, refused proposal\n"),
        Err(RecordRefusal::ProposalPending),
    );
    assert_eq!(
        threads.lock().unwrap_or_else(|e| e.into_inner()).len(),
        before,
        "a refusal appends nothing, so it announces nothing",
    );
}

#[test]
fn a_decision_announces_the_conversation_once_more() {
    // DCP-FR-06 / DCP-FR-15, CMS-FR-51. The decision's own comment reaches the
    // conversation's surface on exactly the terms the proposal's did.
    let f = Fixture::new();
    let pending = f.pending();
    // Registered after the recording, so what it sees is the decision alone.
    let threads = f.watch_threads();
    let accepted = f.accept(&pending.id).expect("accepted");

    let seen = threads.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(seen.len(), 1, "one announcement for one append");
    assert_eq!(seen[0]["id"], f.thread_id);
    let comments = seen[0]["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 3, "seed, proposal, decision");
    assert_eq!(comments[2]["id"], accepted.comment_id.expect("appended"));
    assert!(
        comments[2]["body"]
            .as_str()
            .expect("body")
            .contains("Accepted"),
        "the announced thread carries the decision, not a stale fold",
    );

    // A decline announces on the same terms.
    let f = Fixture::new();
    let pending = f.pending();
    let threads = f.watch_threads();
    f.decline_with(&pending.id, Some("Not this time.")).expect("declined");
    assert_eq!(threads.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
}

#[test]
fn a_decision_refused_by_a_locked_conversation_announces_nothing() {
    // DCP-FR-06, CMS-FR-51 / DCP-FR-15. A conversation locked between the author reading
    // the diff and deciding it is refused **before anything is written**, so
    // the decision is not taken and there is nothing appended for a surface to
    // redraw either.
    let f = Fixture::new();
    let pending = f.pending();
    f.lock();
    let threads = f.watch_threads();

    assert_eq!(
        f.decline_with(&pending.id, None).unwrap_err(),
        crate::comments::ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(
        find_proposal(&f.root, &pending.id).expect("still there").1.state,
        ProposalState::Pending,
        "the proposal stays pending for the author to decide once the lock lifts",
    );
    assert!(threads.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
}

#[test]
fn a_recording_refused_after_the_append_was_attempted_announces_nothing() {
    // DCP-FR-15, CMS-FR-51 / DCP-FR-06. `ProposalPending` refuses before anything is
    // written; a locked conversation is the refusal that gets as far as trying
    // to append, having already put both files on disk. That is the path a stray
    // announcement would come from, and the one worth watching.
    let f = Fixture::new();
    f.lock();
    let threads = f.watch_threads();

    assert_eq!(
        f.propose(FILE, PROPOSED),
        Err(RecordRefusal::ThreadLocked),
    );
    assert!(threads.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    assert!(f.proposals_folder_files().is_empty(), "and nothing was kept");
}

// ---------------------------------------------------------------------------
// DCP-FR-06 — all or nothing (DCP-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_locked_conversation_leaves_no_candidate_behind() {
    let f = Fixture::new();
    f.lock();

    assert_eq!(f.propose(FILE, PROPOSED).unwrap_err(), RecordRefusal::ThreadLocked);
    let dir = f.proposals_dir();
    let left: Vec<String> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        left.is_empty(),
        "the `.content` file written before the append is cleaned up (DCP-FR-06): {left:?}",
    );
    assert_eq!(f.discussion().comments.len(), 1, "the log gained no line");
    assert!(
        list_proposals_impl(&f.root, &f.draft_id).expect("list").is_empty(),
        "nothing lists as a proposal",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-07 — the recording refusals, each distinct (DCP-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn recording_refuses_a_missing_path_and_a_no_op_change_distinctly() {
    let f = Fixture::new();

    assert_eq!(
        f.propose("missing.md", PROPOSED).unwrap_err(),
        RecordRefusal::PathMissing,
    );
    assert_eq!(
        f.propose(FILE, ORIGINAL).unwrap_err(),
        RecordRefusal::NoChange,
        "a proposal identical to the file is nothing to decide",
    );
    assert!(
        list_proposals_impl(&f.root, &f.draft_id).expect("list").is_empty(),
        "neither refusal wrote anything",
    );
    assert_eq!(f.discussion().comments.len(), 1, "and neither appended");
}


// ---------------------------------------------------------------------------
// DCP-FR-JGCD — every change is placed before anything is written
// ---------------------------------------------------------------------------

fn replace(before: &str, after: &str) -> hunks::ProposedHunk {
    hunks::ProposedHunk {
        kind: crate::draft_proposals::anchors::HunkKind::Replace,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        note: None,
    }
}

// DCP-FR-07: a proposal with no changes has nothing for the author to decide.
#[test]
fn a_proposal_carrying_no_changes_is_refused() {
    let f = Fixture::new();
    assert_eq!(f.propose_hunks(FILE, &[]), Err(RecordRefusal::NoHunks));
    assert!(f.proposals_folder_files().is_empty(), "nothing was written");
}

// DCP-FR-JGCD: a model that invented the text it claims to be changing is told
// while it can still correct it.
#[test]
fn a_change_naming_text_the_prompt_does_not_hold_is_refused() {
    let f = Fixture::new();
    assert_eq!(
        f.propose_hunks(FILE, &[replace("text that is not in the prompt", "x")]),
        Err(RecordRefusal::HunkAnchorLost { at: 1, of: 1 }),
    );
    assert!(f.proposals_folder_files().is_empty(), "nothing was written");
}

// DCP-FR-JGCD: text occurring twice names no single place, and the refusal is a
// distinct one because the correction is distinct — include more context.
#[test]
fn a_change_naming_text_that_occurs_twice_is_refused_as_ambiguous() {
    let f = Fixture::new();
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, "a REPEAT b REPEAT c")
        .expect("prompt with a repeated phrase");
    assert_eq!(
        f.propose_hunks(FILE, &[replace("REPEAT", "x")]),
        Err(RecordRefusal::HunkAmbiguous { at: 1, of: 1 }),
    );
    assert!(f.proposals_folder_files().is_empty(), "nothing was written");
}

// DCP-FR-JGCD: two changes covering the same text would not be decidable in any
// order, which is the property the whole anchor model rests on.
#[test]
fn two_changes_covering_the_same_text_are_refused() {
    let f = Fixture::new();
    assert_eq!(
        f.propose_hunks(
            FILE,
            &[replace("original three", "x"), replace("three paragraphs", "y")],
        ),
        Err(RecordRefusal::HunkOverlap { at: 2, of: 2 }),
    );
    assert!(f.proposals_folder_files().is_empty(), "nothing was written");
}

// DCP-FR-01 / DCP-FR-PWSF: several changes are recorded in order, each with its
// own ledger row, and the proposal is pending while any of them is undecided.
#[test]
fn several_changes_are_recorded_in_order_each_with_its_own_ledger_row() {
    let f = Fixture::new();
    let proposal = f
        .propose_hunks(FILE, &[replace("original", "revised"), replace("paragraphs", "sections")])
        .expect("recorded");

    assert_eq!(proposal.hunk_count, 2);
    assert_eq!(proposal.ledger.len(), 2);
    assert_eq!(proposal.counts.pending, 2);
    assert_eq!(proposal.state, ProposalState::Pending);
    assert_ne!(
        proposal.ledger[0].id, proposal.ledger[1].id,
        "each change carries its own identity, which a reply can name",
    );

    let read = crate::draft_proposals::load_hunks_impl(&f.root, &proposal.id).expect("hunks");
    assert_eq!(read.hunks.len(), 2);
    assert!(!read.legacy);
    assert!(
        read.resolutions.iter().all(|r| !matches!(r, HunkPlacement::Lost)),
        "both changes place against the prompt they were composed against",
    );
}

// DCP-FR-07: changes that together leave the text as it is are nothing to
// decide, and would spend the draft's one slot on an empty review.
#[test]
fn changes_that_leave_the_text_as_it_is_are_refused() {
    let f = Fixture::new();
    assert_eq!(
        f.propose_hunks(FILE, &[replace("original", "original")]),
        Err(RecordRefusal::NoChange),
    );
}
