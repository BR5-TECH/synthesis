//! Tests for an agent's update of the open proposal (PCP-FR-RVSN,
//! PCP-FR-RKJI, PCP-FR-FDBD, PCP-FR-18, PCP-FR-17, PCP-FR-LQLT).

use super::hunks::hunk;
use super::per_hunk::two_hunks;
use super::*;
use crate::text_hunks::updates::UpdateEntry;

impl Fixture {
    fn update(&self, updates: &[UpdateEntry]) -> Result<PromptChangeProposal, UpdateRefusal> {
        update_prompt_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            PromptProposalUpdate {
                artifact_id: PROMPT,
                updates,
                rationale: "Asked for. <spoken>Changed.</spoken>",
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }
}

fn withdraw(id: &str) -> UpdateEntry {
    UpdateEntry::Withdraw { id: id.into() }
}

// PCP-FR-RVSN / PCP-FR-RKJI / PCP-FR-17: an update changes one hunk in place,
// withdraws another, adds a third, writes nothing into the file, appends one
// comment, and emits one event whose cause is the update.
#[test]
fn an_update_changes_withdraws_and_adds() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let before = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    let comments_before = f.discussion().comments.len();
    let seen = f.watch();

    let updated = f
        .update(&[
            UpdateEntry::Change { id: second.clone(), hunk: hunk("a note", "three notes", None) },
            withdraw(&first),
            UpdateEntry::Add(hunk("## Steps", "## The steps", None)),
        ])
        .expect("updated");
    let after = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(after.hunks.len(), 3);
    assert_eq!(after.hunks[0], before.hunks[0], "a withdrawn hunk keeps its text");
    assert_eq!(after.hunks[1].id, second, "the id a discussion names is kept");
    assert_eq!(after.hunks[1].after.as_deref(), Some("three notes"));
    assert_eq!(after.hunks[1].revision, 1);
    assert_eq!(updated.ledger[0].state, HunkState::Withdrawn);
    assert_eq!(updated.ledger[1].state, HunkState::Pending);
    assert_eq!(updated.ledger[2].state, HunkState::Pending);
    assert_eq!(updated.counts.withdrawn, 1);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert_eq!(f.discussion().comments.len(), comments_before + 1);
    assert_eq!(read_proposal(&f.root, &recorded.id).unwrap(), updated);
    let seen = seen.lock().expect("events");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["cause"], "updated");
}

// PCP-FR-RVSN: a rejected hunk is changed and undecided again; an accepted one,
// an unknown id, and a closed proposal are refused.
#[test]
fn a_rejected_hunk_is_updatable_and_an_accepted_one_is_not() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.reject_hunk(&recorded.id, &first).expect("rejected");
    let updated = f
        .update(&[UpdateEntry::Change {
            id: first.clone(),
            hunk: hunk("# Review", "# Review!", None),
        }])
        .expect("updated");
    assert_eq!(updated.ledger[0].state, HunkState::Pending, "undecided again");
    assert!(updated.ledger[0].decided_at.is_none());

    f.accept_hunk(&recorded.id, &first).expect("accepted");
    assert_eq!(
        f.update(&[withdraw(&first)]),
        Err(UpdateRefusal::HunkNotUpdatable { at: 1, of: 1 })
    );
    assert_eq!(
        f.update(&[withdraw(&second), withdraw("nosuchhunk")]),
        Err(UpdateRefusal::HunkNotFound { at: 2, of: 2 })
    );
    f.update(&[withdraw(&second)]).expect("updated");
    assert_eq!(f.update(&[withdraw(&second)]), Err(UpdateRefusal::NoOpenProposal));
}

// PCP-FR-RVSN: a change is placed against the file as a recording is, and a
// refusal writes nothing.
#[test]
fn an_update_refuses_text_the_file_does_not_hold_once() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    let before = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    let change = |h| UpdateEntry::Change { id: first.clone(), hunk: h };
    assert_eq!(
        f.update(&[change(hunk("Not there", "x", None))]),
        Err(UpdateRefusal::HunkAnchorLost { at: 1, of: 1 })
    );
    assert_eq!(
        f.update(&[change(hunk("file", "x", None))]),
        Err(UpdateRefusal::HunkAmbiguous { at: 1, of: 1 })
    );
    let long = "x".repeat(201);
    assert!(matches!(
        f.update(&[change(hunk("# Review", "x", Some(&long)))]),
        Err(UpdateRefusal::HunkContextTooLong { at: 1, of: 1, .. })
    ));
    assert_eq!(load_hunks_impl(&f.root, &recorded.id).unwrap(), before, "nothing was written");
}

// PCP-FR-18: a withdrawn hunk takes no decision, hold, or edit.
#[test]
fn a_withdrawn_hunk_takes_no_decision() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    f.update(&[withdraw(&first)]).expect("updated");
    assert_eq!(f.accept_hunk(&recorded.id, &first).unwrap_err(), ERR_HUNK_WITHDRAWN);
    assert_eq!(f.reject_hunk(&recorded.id, &first).unwrap_err(), ERR_HUNK_WITHDRAWN);
    assert_eq!(
        hold_hunk_impl(&f.root, &recorded.id, &first, true).unwrap_err(),
        ERR_HUNK_WITHDRAWN
    );
}

// PCP-FR-RKJI: a failed append restores both files.
#[test]
fn an_update_into_a_resolved_conversation_writes_nothing() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    let before = read_proposal(&f.root, &recorded.id).unwrap();
    f.resolve_conversation();
    assert_eq!(f.update(&[withdraw(&first)]), Err(UpdateRefusal::ThreadResolved));
    assert_eq!(read_proposal(&f.root, &recorded.id).unwrap(), before);
}

// PCP-FR-LQLT: one proposal's changes, with ids and states, for a turn's input.
#[test]
fn the_changes_of_a_proposal_read_with_their_states() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.update(&[withdraw(&first)]).expect("updated");
    let read = read_prompt_proposal_changes(&f.root, &recorded.id).expect("read");
    assert_eq!(read.path, PROMPT);
    assert_eq!(read.counts.withdrawn, 1);
    assert_eq!(read.changes[0].id, first);
    assert_eq!(read.changes[0].state, HunkState::Withdrawn);
    assert_eq!(read.changes[1].id, second);
    assert_eq!(read.changes[1].state, HunkState::Pending);
}

// PCP-FR-RSLV: an update that closes the proposal appends its own comment and
// no decision comment.
#[test]
fn an_update_that_closes_the_proposal_appends_no_decision_comment() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.accept_hunk(&recorded.id, &first).expect("accepted");
    let comments = f.discussion().comments.len();
    let closed = f.update(&[withdraw(&second)]).expect("updated");
    assert_eq!(closed.state, PromptProposalState::Accepted);
    assert_eq!(f.discussion().comments.len(), comments + 1, "the update's comment alone");
}

// PCP-FR-18: an edit of a withdrawn hunk is refused, and a decline leaves it
// withdrawn.
#[test]
fn an_edit_of_a_withdrawn_hunk_is_refused_and_a_decline_leaves_it() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.update(&[withdraw(&first)]).expect("updated");
    let checksum = load_hunks_impl(&f.root, &recorded.id).expect("hunks").checksum;
    assert_eq!(
        edit_hunk_impl(&f.root, &recorded.id, &first, "mine", &checksum).unwrap_err(),
        ERR_HUNK_WITHDRAWN
    );
    f.decline(&recorded.id).expect("declined");
    let after = read_proposal(&f.root, &recorded.id).unwrap();
    assert_eq!(after.ledger[0].state, HunkState::Withdrawn);
    assert_eq!(after.ledger[1].id, second);
    assert_eq!(after.ledger[1].state, HunkState::Rejected);
    assert_eq!(after.state, PromptProposalState::Rejected);
}

// PCP-FR-FDBD: a legacy proposal is updated by naming its one hunk `legacy`,
// and holds a `.hunks` file and a ledger from then on.
#[test]
fn a_legacy_proposal_is_updated_by_its_legacy_hunk() {
    let f = Fixture::new();
    let id = super::legacy::stage_legacy(&f, PROPOSED);
    let updated = f
        .update(&[UpdateEntry::Change {
            id: LEGACY_HUNK_ID.into(),
            hunk: hunk("# Review", "# Careful review", None),
        }])
        .expect("updated");
    assert!(!updated.legacy);
    assert_eq!(updated.ledger.len(), 1);
    assert_eq!(updated.ledger[0].id, LEGACY_HUNK_ID);
    assert_eq!(updated.ledger[0].revision, 1);
    assert!(f.dir().join(format!("{id}.hunks")).exists(), "a hunk document now");
    let hunks = load_hunks_impl(&f.root, &id).expect("hunks");
    assert!(!hunks.legacy);
    assert_eq!(hunks.hunks[0].after.as_deref(), Some("# Careful review"));
}
