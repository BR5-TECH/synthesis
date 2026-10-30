//! Tests for deciding a prompt change proposal hunk by hunk (PCP-FR-HKLD,
//! PCP-FR-LDHK, PCP-FR-ZQAH, PCP-FR-RJHN, PCP-FR-HLDQ, PCP-FR-RSLV).

use super::hunks::hunk;
use super::*;

/// A proposal of two hunks against `ORIGINAL`, and the ids of its hunks.
pub(super) fn two_hunks(f: &Fixture) -> (PromptChangeProposal, String, String) {
    let recorded = f
        .propose_hunks(&[
            hunk("# Review", "# Careful review", None),
            hunk("file a note", "file two notes", None),
        ])
        .expect("recorded");
    let first = recorded.ledger[0].id.clone();
    let second = recorded.ledger[1].id.clone();
    (recorded, first, second)
}

const FIRST_ONLY: &str =
    "# Careful review\n\n## Steps\n\nRead the file, summarise it and file a note.\n";
const BOTH: &str =
    "# Careful review\n\n## Steps\n\nRead the file, summarise it and file two notes.\n";
const SECOND_ONLY: &str =
    "# Review\n\n## Steps\n\nRead the file, summarise it and file two notes.\n";

// PCP-FR-HKLD: the record's wire shape carries the ledger, the counts, the
// hunk count, the legacy flag, and the base checksum, in camelCase.
#[test]
fn the_record_carries_its_ledger_and_counts_on_the_wire() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    let wire = serde_json::to_value(&recorded).expect("json");
    assert_eq!(wire["state"], "pending");
    assert_eq!(wire["legacy"], false);
    assert_eq!(wire["hunkCount"], 2);
    assert_eq!(
        wire["counts"],
        serde_json::json!({ "pending": 2, "accepted": 0, "rejected": 0, "discussing": 0 })
    );
    assert_eq!(wire["ledger"][0]["id"], first.as_str());
    assert_eq!(wire["ledger"][0]["kind"], "replace");
    assert_eq!(wire["ledger"][0]["state"], "pending");
    assert_eq!(wire["ledger"][0]["edited"], false);
    assert_eq!(wire["ledger"][0]["revision"], 0);
    assert_eq!(wire["candidateEdited"], false);
    assert_eq!(wire["commentOwed"], false);
    assert_eq!(wire["baseSha256"], fs::sha256_bytes(ORIGINAL.as_bytes()));
    // …and the record survives a round trip through the store unchanged.
    assert_eq!(read_proposal(&f.root, &recorded.id).unwrap(), recorded);
}

// PCP-FR-LDHK: the hunks come with their placement against the file as it
// stands now, the checksum of the hunk document, and the legacy flag.
#[test]
fn loaded_hunks_are_placed_against_the_file_as_it_stands() {
    let f = Fixture::new();
    let (recorded, _, _) = two_hunks(&f);
    let loaded = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert!(!loaded.legacy);
    assert_eq!(loaded.hunks.len(), 2);
    assert_eq!(loaded.resolutions[0], HunkPlacement::Resolved { start: 0, end: 8 });

    // The author adds a line above: the hunks follow their text.
    write_file(&f.root, PROMPT, &format!("Intro.\n{ORIGINAL}"));
    let moved = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(moved.resolutions[0], HunkPlacement::Resolved { start: 7, end: 15 });
    assert_eq!(moved.checksum, loaded.checksum, "the hunk document did not change");

    // The text of the second hunk goes: it is lost, and the first still stands.
    write_file(&f.root, PROMPT, &ORIGINAL.replace("file a note", "file nothing"));
    let lost = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert!(matches!(lost.resolutions[0], HunkPlacement::Resolved { .. }));
    assert_eq!(lost.resolutions[1], HunkPlacement::Lost);
    let wire = serde_json::to_value(&lost).expect("json");
    assert_eq!(wire["resolutions"][1], serde_json::json!({ "kind": "lost" }));
    assert_eq!(wire["resolutions"][0]["kind"], "resolved");
}

// PCP-FR-ZQAH, PCP-FR-RSLV: accepting one hunk writes that hunk alone into the
// file, leaves the other undecided, and appends no comment. The last decision
// resolves the proposal and appends the one decision comment.
#[test]
fn accepting_one_hunk_writes_that_hunk_alone_and_the_last_one_resolves() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let events = f.watch();
    let comments_before = f.discussion().comments.len();

    let outcome = f.accept_hunk(&recorded.id, &first).expect("accepted");
    assert!(!outcome.resolved);
    assert_eq!(outcome.comment_id, None, "an unresolving decision appends nothing");
    assert_eq!(outcome.proposal.state, PromptProposalState::Pending);
    assert_eq!(outcome.proposal.ledger[0].state, HunkState::Accepted);
    assert_eq!(outcome.proposal.ledger[1].state, HunkState::Pending);
    assert_eq!(outcome.proposal.counts.accepted, 1);
    assert!(!outcome.proposal.comment_owed);
    assert_eq!(f.artifact_body(), FIRST_ONLY);
    assert_eq!(f.discussion().comments.len(), comments_before);
    assert!(!f.dir().join(format!("{}.journal", recorded.id)).exists());
    assert!(!f.dir().join(format!("{}.prior", recorded.id)).exists());

    let last = f.accept_hunk(&recorded.id, &second).expect("accepted");
    assert!(last.resolved);
    assert_eq!(last.proposal.state, PromptProposalState::Accepted);
    assert_eq!(f.artifact_body(), BOTH);
    let thread = f.discussion();
    assert_eq!(thread.comments.len(), comments_before + 1, "one comment for the proposal");
    let decision = thread.comments.last().expect("decision");
    assert_eq!(Some(decision.id.clone()), last.comment_id);
    assert_eq!(
        decision.body,
        format!("Accepted the proposed change to `{PROMPT}`. 2 accepted, 0 rejected.")
    );
    // PCP-FR-17: one event per decision.
    assert_eq!(events.lock().unwrap().len(), 2);
}

// PCP-FR-ZQAH: a hunk is placed against the file as the author has since
// changed it, and the author's own change is kept.
#[test]
fn an_accepted_hunk_is_spliced_into_the_file_as_the_author_left_it() {
    let f = Fixture::new();
    let (recorded, _, second) = two_hunks(&f);
    write_file(&f.root, PROMPT, &ORIGINAL.replace("## Steps", "## The steps"));
    f.accept_hunk(&recorded.id, &second).expect("accepted");
    assert_eq!(f.artifact_body(), SECOND_ONLY.replace("## Steps", "## The steps"));
}

// PCP-FR-RJHN: rejecting a hunk writes nothing into the file and appends no
// comment while another hunk is undecided; a rejected hunk can still be
// accepted while the proposal is open; a decided hunk refuses a second
// decision of the same kind.
#[test]
fn a_rejected_hunk_writes_nothing_and_can_still_be_accepted() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let comments_before = f.discussion().comments.len();

    let rejected = f.reject_hunk(&recorded.id, &first).expect("rejected");
    assert!(!rejected.resolved);
    assert_eq!(rejected.comment_id, None);
    assert_eq!(rejected.proposal.ledger[0].state, HunkState::Rejected);
    assert!(rejected.proposal.ledger[0].decided_at.is_some());
    assert_eq!(f.artifact_body(), ORIGINAL, "a rejection writes nothing");
    assert_eq!(f.discussion().comments.len(), comments_before);
    assert_eq!(f.reject_hunk(&recorded.id, &first).unwrap_err(), ERR_HUNK_ALREADY_DECIDED);

    let accepted = f.accept_hunk(&recorded.id, &first).expect("a rejected hunk can be accepted");
    assert_eq!(accepted.proposal.ledger[0].state, HunkState::Accepted);
    assert_eq!(f.artifact_body(), FIRST_ONLY);
    assert_eq!(f.accept_hunk(&recorded.id, &first).unwrap_err(), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.reject_hunk(&recorded.id, &first).unwrap_err(), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.accept_hunk(&recorded.id, "nosuchhunk").unwrap_err(), ERR_HUNK_NOT_FOUND);
    assert_eq!(f.reject_hunk(&recorded.id, "nosuchhunk").unwrap_err(), ERR_HUNK_NOT_FOUND);

    // PCP-FR-RSLV: the rejection that resolves it appends the one comment,
    // counting each hunk by its final state.
    let last = f.reject_hunk(&recorded.id, &second).expect("rejected");
    assert!(last.resolved);
    assert_eq!(last.proposal.state, PromptProposalState::Accepted);
    assert_eq!(f.artifact_body(), FIRST_ONLY);
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert_eq!(Some(decision.id), last.comment_id);
    assert_eq!(
        decision.body,
        format!("Accepted the proposed change to `{PROMPT}`. 1 accepted, 1 rejected.")
    );
    assert_eq!(f.discussion().comments.len(), comments_before + 1);
}

// PCP-FR-RJHN, PCP-FR-RSLV: declining rejects every undecided hunk at once,
// leaves an accepted one standing, writes nothing, and appends the one
// comment with the author's feedback.
#[test]
fn declining_rejects_every_undecided_hunk_and_keeps_the_accepted_one() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.accept_hunk(&recorded.id, &first).expect("accepted");
    let outcome =
        f.decline_with(&recorded.id, Some("The rest is fine as it was.")).expect("declined");
    assert!(outcome.resolved);
    assert_eq!(outcome.proposal.state, PromptProposalState::Accepted);
    let row = |id: &str| outcome.proposal.ledger.iter().find(|r| r.id == id).unwrap().state;
    assert_eq!(row(&first), HunkState::Accepted);
    assert_eq!(row(&second), HunkState::Rejected);
    assert_eq!(f.artifact_body(), FIRST_ONLY, "the decline wrote nothing");
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert!(decision.body.contains("1 accepted, 1 rejected."), "{}", decision.body);
    assert!(decision.body.ends_with("The rest is fine as it was."));

    // A proposal with nothing accepted reads `rejected` once declined.
    let g = Fixture::new();
    let (other, _, _) = two_hunks(&g);
    let declined = g.decline(&other.id).expect("declined");
    assert_eq!(declined.proposal.state, PromptProposalState::Rejected);
    assert_eq!(declined.proposal.counts.rejected, 2);
    let body = g.discussion().comments.last().cloned().expect("decision").body;
    assert_eq!(
        body,
        format!("Declined the proposed change to `{PROMPT}`. 0 accepted, 2 rejected.")
    );
}

// PCP-FR-HLDQ: holding a hunk marks it `discussing` and decides nothing; the
// proposal stays pending, the file is untouched, and a held hunk can be
// released, accepted, or declined.
#[test]
fn holding_a_hunk_decides_nothing() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let comments_before = f.discussion().comments.len();

    let held = hold_hunk_impl(&f.root, &recorded.id, &first, true).expect("held");
    assert_eq!(held.ledger[0].state, HunkState::Discussing);
    assert_eq!(held.state, PromptProposalState::Pending);
    assert_eq!(held.counts.discussing, 1);
    assert_eq!(held.counts.pending, 1);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert_eq!(f.discussion().comments.len(), comments_before);
    assert!(pending_for(&f.root, PROMPT).is_some(), "it still holds the one slot");

    let released = hold_hunk_impl(&f.root, &recorded.id, &first, false).expect("released");
    assert_eq!(released.ledger[0].state, HunkState::Pending);

    hold_hunk_impl(&f.root, &recorded.id, &first, true).expect("held");
    f.accept_hunk(&recorded.id, &first).expect("a held hunk can be accepted");
    assert_eq!(
        hold_hunk_impl(&f.root, &recorded.id, &first, true).unwrap_err(),
        ERR_HUNK_ALREADY_DECIDED,
        "a decided hunk is not held",
    );
    assert_eq!(
        hold_hunk_impl(&f.root, &recorded.id, "nosuchhunk", true).unwrap_err(),
        ERR_HUNK_NOT_FOUND
    );

    hold_hunk_impl(&f.root, &recorded.id, &second, true).expect("held");
    let declined = f.decline(&recorded.id).expect("declined");
    assert_eq!(declined.proposal.ledger[1].state, HunkState::Rejected, "declining clears a hold");
}

// PCP-FR-ZQAH, PCP-FR-14: a rolled-back acceptance of one hunk restores that
// hunk's row alone and leaves an earlier decision standing.
#[test]
fn an_interrupted_hunk_acceptance_rolls_back_that_hunk_alone() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.reject_hunk(&recorded.id, &first).expect("rejected");
    let standing = read_proposal(&f.root, &recorded.id).expect("record");

    // Stage an acceptance of the second hunk that never reached its commit point.
    let dir = f.dir();
    let prior = std::fs::read(f.root.path().join(PROMPT)).expect("prior");
    let journal = Journal {
        operation_id: new_note_id(),
        proposal_id: recorded.id.clone(),
        artifact_id: PROMPT.into(),
        path: PROMPT.into(),
        prior_sha256: fs::sha256_bytes(&prior),
        candidate_sha256: fs::sha256_bytes(SECOND_ONLY.as_bytes()),
        comment_id: new_note_id(),
        event_id: new_note_id(),
        thread_id: f.thread_id.clone(),
        comment_body: "Accepted.".into(),
        created_at: now_rfc3339(),
        hunk_id: Some(second.clone()),
        prior_state: HunkState::Pending,
        prior_decided_at: None,
        resolves: true,
        comment_by: human(),
    };
    std::fs::write(dir.join(format!("{}.prior", recorded.id)), &prior).expect("prior");
    f.root.write_toml_atomic(dir.join(format!("{}.journal", recorded.id)), &journal).unwrap();
    let mut moved = standing.clone();
    moved.ledger[1].state = HunkState::Accepted;
    moved.comment_owed = true;
    ledger::resettle(&mut moved);
    f.root.write_toml_atomic(dir.join(format!("{}.toml", recorded.id)), &moved).unwrap();

    let listed = f.list();
    let record = listed.iter().find(|p| p.id == recorded.id).expect("listed");
    assert_eq!(record.ledger[0].state, HunkState::Rejected, "the earlier decision stands");
    assert_eq!(record.ledger[1].state, HunkState::Pending, "the interrupted one is undone");
    assert_eq!(record.state, PromptProposalState::Pending);
    assert!(!record.comment_owed);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert!(!dir.join(format!("{}.journal", recorded.id)).exists());
}
