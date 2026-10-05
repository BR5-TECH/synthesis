//! Deciding one change at a time (DCP-FR-11, DCP-FR-14, DCP-FR-15, DCP-FR-PWSF).

use super::*;
use crate::draft_proposals::anchors::HunkKind;
use crate::draft_proposals::hunks::{HunkState, ProposedHunk};

fn replace(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        note: None,
    }
}

/// A proposal of two independent changes to the seeded prompt.
fn two_changes(f: &Fixture) -> DraftChangeProposal {
    f.propose_hunks(FILE, &[replace("original", "revised"), replace("paragraphs", "sections")])
        .expect("recorded")
}

// DCP-FR-11 / DCP-FR-12: accepting one change applies that change and leaves
// every other change exactly as it was.
#[test]
fn accepting_one_change_leaves_the_others_undecided() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let (first, second) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());

    f.accept_hunk(&p.id, &first).expect("accepted");

    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Accepted);
    assert_eq!(after.ledger[1].state, HunkState::Pending, "the other is untouched");
    assert_eq!(after.state, ProposalState::Pending, "the proposal is not resolved");
    assert_eq!(after.counts.accepted, 1);
    assert_eq!(after.counts.undecided(), 1);
    assert!(f.file_body().contains("revised"));
    assert!(f.file_body().contains("paragraphs"), "the second change has not landed");

    f.accept_hunk(&p.id, &second).expect("accepted");
    assert!(f.file_body().contains("sections"));
    assert_eq!(f.reread(&p.id).state, ProposalState::Accepted);
}

// DCP-FR-VZTK: the order the author decides in does not change what lands.
#[test]
fn deciding_in_either_order_reaches_the_same_prompt() {
    let forward = {
        let f = Fixture::new();
        let p = two_changes(&f);
        f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
        f.accept_hunk(&p.id, &p.ledger[1].id.clone()).expect("accepted");
        f.file_body()
    };
    let backward = {
        let f = Fixture::new();
        let p = two_changes(&f);
        f.accept_hunk(&p.id, &p.ledger[1].id.clone()).expect("accepted");
        f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
        f.file_body()
    };
    assert_eq!(forward, backward);
}

// DCP-FR-15: only the decision that leaves nothing undecided appends a comment,
// so an agent gets one turn to answer rather than one per change.
#[test]
fn only_the_resolving_decision_appends_a_comment() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let before = f.discussion().comments.len();

    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(
        f.discussion().comments.len(),
        before,
        "an unresolving decision says nothing in the conversation",
    );

    let last = f.reject_hunk(&p.id, &p.ledger[1].id.clone()).expect("rejected");
    assert_eq!(f.discussion().comments.len(), before + 1);
    assert!(last.comment_id.is_some(), "the resolving decision carries its comment");
}

// DCP-FR-17: a change is decided once.
#[test]
fn a_change_already_decided_is_refused() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let first = p.ledger[0].id.clone();
    f.accept_hunk(&p.id, &first).expect("accepted");
    assert_eq!(f.accept_hunk(&p.id, &first).expect_err("refused"), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.reject_hunk(&p.id, &first).expect_err("refused"), ERR_HUNK_ALREADY_DECIDED);
}

// DCP-FR-22: a change id naming nothing is a typed refusal.
#[test]
fn a_change_that_does_not_exist_is_refused() {
    let f = Fixture::new();
    let p = two_changes(&f);
    assert_eq!(f.accept_hunk(&p.id, "no-such-change").expect_err("refused"), ERR_HUNK_NOT_FOUND);
}

// DCP-FR-18: accepting a change whose text the author rewrote is refused and
// writes nothing; rejecting it is how it is cleared.
#[test]
fn a_lost_change_refuses_acceptance_and_clears_by_rejection() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let first = p.ledger[0].id.clone();
    let rewritten = "# Spec\n\nCompletely different prose now.\n";
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, rewritten).expect("edit");

    assert_eq!(f.accept_hunk(&p.id, &first).expect_err("refused"), ERR_ANCHOR_LOST);
    assert_eq!(f.file_body(), rewritten, "nothing was written");

    f.reject_hunk(&p.id, &first).expect("rejecting clears it");
    assert_eq!(f.reread(&p.id).ledger[0].state, HunkState::Rejected);
}

// DCP-FR-14: a decline naming no change rejects every change still undecided,
// which is what clears a proposal holding one indefinitely.
#[test]
fn declining_the_proposal_rejects_every_undecided_change() {
    let f = Fixture::new();
    let p = two_changes(&f);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");

    f.decline(&p.id).expect("declined");

    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Accepted, "a settled change is not re-decided");
    assert_eq!(after.ledger[1].state, HunkState::Rejected);
    assert_eq!(after.state, ProposalState::Accepted, "one change was accepted");
    assert_eq!(after.counts.undecided(), 0);
}

// DHS-FR-08: one version per accepted change, so the history holds what the
// prompt actually became at each step.
#[test]
fn each_accepted_change_records_one_version() {
    let f = Fixture::new();
    let p = two_changes(&f);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    let after_first = crate::draft_history::list_impl(&f.root, &f.draft_id).expect("history");
    // The first acceptance settles the `Original` it superseded as well.
    assert_eq!(after_first.entries.len(), 2);

    f.accept_hunk(&p.id, &p.ledger[1].id.clone()).expect("accepted");
    let after_second = crate::draft_history::list_impl(&f.root, &f.draft_id).expect("history");
    assert_eq!(after_second.entries.len(), 3);

    let sources: Vec<Option<String>> = after_second
        .entries
        .iter()
        .map(|e| match &e.source {
            crate::draft_history::DraftHistorySource::ProposalAccepted { hunk_id, .. } => {
                hunk_id.clone()
            }
            crate::draft_history::DraftHistorySource::Original => None,
        })
        .collect();
    assert_eq!(
        sources[1..].to_vec(),
        vec![Some(p.ledger[0].id.clone()), Some(p.ledger[1].id.clone())],
        "each version names the change that produced it",
    );
}

// DCP-FR-XDRV / PDC-FR-GMWR: a revision replaces one change in place, keeping
// the id every reply addressed to it names, and leaves it undecided.
#[test]
fn revising_a_change_keeps_its_identity_and_leaves_it_undecided() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let held = p.ledger[1].id.clone();

    let after = crate::draft_proposals::revise_hunk_impl(
        &f.root,
        &f.draft_id,
        &held,
        &replace("paragraphs", "chapters"),
    )
    .expect("revised");

    assert_eq!(after.ledger[1].id, held, "the id a reply names is kept");
    assert_eq!(after.ledger[1].state, HunkState::Pending, "it stays undecided");
    assert_eq!(after.ledger[1].revision, 1);
    assert_eq!(after.hunk_count, 2, "the number of changes does not change");

    let read = crate::draft_proposals::load_hunks_impl(&f.root, &p.id).expect("hunks");
    assert_eq!(read.hunks[1].after_text(), "chapters");
}

// DCP-FR-XDRV: a change already decided is not revised.
#[test]
fn a_decided_change_is_not_revised() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let first = p.ledger[0].id.clone();
    f.accept_hunk(&p.id, &first).expect("accepted");
    assert_eq!(
        crate::draft_proposals::revise_hunk_impl(
            &f.root,
            &f.draft_id,
            &first,
            &replace("revised", "again"),
        )
        .expect_err("refused"),
        ERR_HUNK_ALREADY_DECIDED,
    );
}

// DCP-FR-NKTB: every write against one draft's proposals is serialised, so two
// windows accepting two changes at once both land and neither is lost.
#[test]
fn two_windows_accepting_two_changes_at_once_both_land() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let (a, b) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());
    let app = f.handle();
    let root = &f.root;
    let id = &p.id;

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let _ = decide(&app, root, root, id, Decision::Accept, Some(&a), None, &human());
        });
        scope.spawn(|| {
            let _ = decide(&app, root, root, id, Decision::Accept, Some(&b), None, &human());
        });
    });

    let after = f.reread(&p.id);
    assert_eq!(after.counts.accepted, 2, "neither acceptance was lost to the other");
    let body = f.file_body();
    assert!(body.contains("revised") && body.contains("sections"));
}

// DCP-FR-NKTB: a change held for discussion is read against the ledger as it
// stands, so it cannot put a proposal another window has already settled back
// to `pending`.
#[test]
fn holding_a_change_of_a_settled_proposal_is_refused() {
    let f = Fixture::new();
    let p = two_changes(&f);
    let (first, second) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());

    // Both decided, so the proposal is settled and its prompt is written.
    f.accept_hunk(&p.id, &first).expect("accepted");
    f.reject_hunk(&p.id, &second).expect("rejected");
    let settled = f.reread(&p.id);
    assert_eq!(settled.state, ProposalState::Accepted);

    // The window that was still showing the review now asks to hold the second
    // change for discussion. Taken, it would clear `decided_at` and read as
    // pending again — with the prompt already rewritten and a version already
    // in History for the acceptance.
    let refused = crate::draft_proposals::set_hunk_state(
        &f.root,
        &f.draft_id,
        &p.id,
        &second,
        HunkState::Discussing,
    );
    assert!(refused.is_err(), "a settled change is not put back to undecided");
    let after = f.reread(&p.id);
    assert_eq!(after.state, ProposalState::Accepted, "the proposal stays settled");
    assert!(after.decided_at.is_some(), "and stays decided");
}

// DCP-FR-JGCD: an insertion that names no text to follow says nothing about
// where it goes, so it is refused rather than placed at the head of the prompt.
#[test]
fn an_insertion_with_no_context_is_refused_unless_the_prompt_is_empty() {
    let f = Fixture::new();
    let floating = ProposedHunk {
        kind: HunkKind::Add,
        before: None,
        after: Some("A new paragraph.".into()),
        after_text: None,
        note: None,
    };

    // A model that gave no context has proposed a change nobody can place. It
    // is told while it can still fix it, rather than the text appearing at the
    // top of a prompt it was never meant for.
    assert!(f.propose_hunks(FILE, std::slice::from_ref(&floating)).is_err());

    // An empty prompt has exactly one place the text could go, so there it is
    // allowed — which is what lets an agent write the first draft of a prompt.
    let planned = crate::draft_proposals::hunks::plan("", std::slice::from_ref(&floating), || {
        "h-1".to_string()
    })
    .expect("an empty prompt takes an insertion with no context");
    assert_eq!(planned.hunks.len(), 1);
}

// DCP-FR-VZTK: three adjacent changes are decidable in **any** order.
//
// Two changes hide the failure this test is for: each keeps one clean side, so
// step 3 of the resolution saves it. A change with a neighbour on each side has
// no clean side at all unless its context stops short of both — accept the two
// outer changes first and the middle one is lost, leaving the author able only
// to reject the very change they were keeping for last.
#[test]
fn three_adjacent_changes_are_decidable_in_every_order() {
    let orders: [[usize; 3]; 6] =
        [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let mut settled: Vec<String> = Vec::new();

    for order in orders {
        let f = Fixture::new();
        let p = f
            .propose_hunks(
                FILE,
                &[
                    replace("original", "first"),
                    replace("three", "second"),
                    replace("paragraphs", "third"),
                ],
            )
            .expect("recorded");
        let ids: Vec<String> = p.ledger.iter().map(|row| row.id.clone()).collect();

        for at in order {
            f.accept_hunk(&p.id, &ids[at])
                .unwrap_or_else(|e| panic!("order {order:?} lost change {at}: {e}"));
        }

        let body = f.file_body();
        assert!(body.contains("first"), "order {order:?}");
        assert!(body.contains("second"), "order {order:?}");
        assert!(body.contains("third"), "order {order:?}");
        settled.push(body);
    }

    // And every order reaches the same prompt, byte for byte.
    for body in &settled[1..] {
        assert_eq!(body, &settled[0], "the order the author took decides nothing");
    }
}

// DCP-FR-15 — a rejection that leaves changes undecided resolves nothing, so it
// appends no comment and starts no agent turn
#[test]
fn a_rejection_that_resolves_nothing_appends_no_comment() {
    let f = Fixture::new();
    let proposal = two_changes(&f);
    let before = f.discussion().comments.len();

    f.reject_hunk(&proposal.id, &proposal.ledger[0].id).expect("rejected");

    assert_eq!(
        f.discussion().comments.len(),
        before,
        "one change of two is still undecided, so the proposal is not resolved",
    );
}

// DCP-FR-15 — the resolving decision's tally counts the whole proposal, not the
// one change that resolved it
#[test]
fn the_resolving_decision_reports_the_whole_proposals_tally() {
    let f = Fixture::new();
    let proposal = two_changes(&f);
    // The first change is refused, and the second resolves the proposal.
    f.reject_hunk(&proposal.id, &proposal.ledger[0].id).expect("rejected");
    f.accept_hunk(&proposal.id, &proposal.ledger[1].id).expect("accepted");

    let comments = f.discussion().comments;
    let decision = comments.last().expect("the decision comment");
    assert!(
        decision.body.contains("1 accepted, 1 rejected"),
        "the tally counts every change the author decided, not the last one: {}",
        decision.body,
    );
    assert_eq!(comments.len(), 3, "seed, proposal, and the one resolving decision");
}

// DCP-FR-15 — declining after a partial accept reports what was already taken
#[test]
fn declining_the_rest_still_reports_what_was_accepted() {
    let f = Fixture::new();
    let proposal = two_changes(&f);
    f.accept_hunk(&proposal.id, &proposal.ledger[0].id).expect("accepted");

    decline_for_test(&f.handle(), &f.root, &f.root, &proposal.id, &human())
        .expect("declined the rest");

    let decision = f.discussion().comments.pop().expect("the decision comment");
    assert!(
        decision.body.contains("1 accepted, 1 rejected"),
        "a decline does not erase what the author already took: {}",
        decision.body,
    );
}
