//! A rejected change while its proposal stands (DCP-FR-11, DCP-FR-13,
//! DCP-FR-15, DCP-FR-17, DCP-FR-XDRV).

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

/// A proposal of two independent changes to the seeded prompt, the first of
/// them rejected and the second still undecided, so the proposal stands.
fn first_rejected(f: &Fixture) -> (DraftChangeProposal, String, String) {
    let p = f
        .propose_hunks(FILE, &[replace("original", "revised"), replace("paragraphs", "sections")])
        .expect("recorded");
    let (first, second) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());
    f.reject_hunk(&p.id, &first).expect("rejected");
    (f.reread(&p.id), first, second)
}

// DCP-FR-11: a rejected change of a proposal that is not resolved can still be
// accepted. It lands in the prompt, and the proposal stays pending while
// another change is undecided, so no decision comment is appended.
#[test]
fn a_rejected_change_is_accepted_while_the_proposal_stands() {
    let f = Fixture::new();
    let (p, first, _) = first_rejected(&f);
    assert_eq!(p.state, ProposalState::Pending);
    let comments_before = f.discussion().comments.len();

    let decided = f.accept_hunk(&p.id, &first).expect("accepted");

    assert_eq!(decided.comment_id, None, "the proposal is not resolved by this");
    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Accepted);
    assert_eq!(after.ledger[1].state, HunkState::Pending);
    assert_eq!(after.state, ProposalState::Pending);
    assert_eq!((after.counts.accepted, after.counts.rejected), (1, 0));
    assert!(f.file_body().contains("revised"), "the change landed");
    assert_eq!(f.discussion().comments.len(), comments_before);
}

// DCP-FR-17: a rejection of a rejected or an accepted change, and an
// acceptance of an accepted one, is `hunk_already_decided` and writes nothing.
#[test]
fn a_change_is_rejected_once_and_accepted_once() {
    let f = Fixture::new();
    let (p, first, _) = first_rejected(&f);
    let body = f.file_body();
    assert_eq!(f.reject_hunk(&p.id, &first).expect_err("refused"), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.reread(&p.id), p, "the record is unchanged");

    f.accept_hunk(&p.id, &first).expect("accepted");
    let accepted = f.reread(&p.id);
    let body_after = f.file_body();
    assert_ne!(body, body_after);
    assert_eq!(f.accept_hunk(&p.id, &first).expect_err("refused"), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.reject_hunk(&p.id, &first).expect_err("refused"), ERR_HUNK_ALREADY_DECIDED);
    assert_eq!(f.reread(&p.id), accepted);
    assert_eq!(f.file_body(), body_after, "nothing was written");
}

// DCP-FR-17: once no change is undecided the proposal is resolved, and a
// rejected change of it can no longer be accepted.
#[test]
fn a_rejected_change_of_a_resolved_proposal_is_not_accepted() {
    let f = Fixture::new();
    let (p, first, second) = first_rejected(&f);
    f.reject_hunk(&p.id, &second).expect("rejected");
    assert_eq!(f.reread(&p.id).state, ProposalState::Rejected);
    assert_eq!(f.accept_hunk(&p.id, &first).expect_err("refused"), ERR_ALREADY_DECIDED);
}

// DCP-FR-15: the decision comment counts each change by the state it finally
// holds, so a change rejected and then accepted counts as accepted.
#[test]
fn the_decision_comment_counts_each_change_by_its_final_state() {
    let f = Fixture::new();
    let (p, first, second) = first_rejected(&f);
    f.accept_hunk(&p.id, &first).expect("accepted");
    let resolving = f.accept_hunk(&p.id, &second).expect("accepted");

    assert!(resolving.comment_id.is_some(), "the resolving decision appends one comment");
    let decision = f.discussion().comments.pop().expect("the decision comment");
    assert!(decision.body.contains("2 accepted, 0 rejected"), "{}", decision.body);
    assert_eq!(f.reread(&p.id).state, ProposalState::Accepted);
}

// DCP-FR-13 / DHS-FR-17: an acceptance of a rejected change that fails before
// the commit point leaves the prompt as it was and the change rejected, its
// `decided_at` kept.
#[test]
fn a_failed_acceptance_of_a_rejected_change_leaves_it_rejected() {
    // The read-only `files/` below binds every process except root.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_acceptance_of_a_rejected_change_leaves_it_rejected",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    let (p, first, _) = first_rejected(&f);
    let body = f.file_body();
    let files_dir = f.root.path().join(format!(".synthesis/drafts/{}/files", f.draft_id));
    let mut perms = std::fs::metadata(&files_dir).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&files_dir, perms).expect("chmod");

    let outcome = f.accept_hunk(&p.id, &first);

    let mut perms = std::fs::metadata(&files_dir).expect("meta").permissions();
    // Reason: the test restores write permission on its own temp directory
    // so that the directory can be deleted; no production code does this.
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&files_dir, perms).expect("chmod");

    assert_eq!(outcome.unwrap_err(), ERR_WRITE_FAILED);
    assert_eq!(f.file_body(), body, "byte-for-byte what it was");
    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Rejected, "it holds the state it held");
    assert_eq!(after.ledger[0].decided_at, p.ledger[0].decided_at);
    assert_eq!(after.state, ProposalState::Pending);
}

// DCP-FR-13 / DHS-FR-19: the reconciliation of an interrupted acceptance of a
// rejected change puts the change back to `rejected`, the state the journal
// recorded, rather than to `pending`.
#[test]
fn an_interrupted_acceptance_of_a_rejected_change_reconciles_to_rejected() {
    let f = Fixture::new();
    let (p, first, _) = first_rejected(&f);
    crate::draft_history::stage_hunk_prepared_for_test(&f.root, &f.draft_id, &p, &first, &human())
        .expect("staged");
    assert_eq!(f.reread(&p.id).ledger[0].state, HunkState::Accepted, "the crash left it moved");

    crate::draft_history::reconcile(&f.handle(), &f.root, &f.root, &f.draft_id)
        .expect("reconciled");

    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Rejected);
    assert_eq!(after.ledger[0].decided_at, p.ledger[0].decided_at);
    assert_eq!(after.ledger[1].state, HunkState::Pending, "the other change is untouched");
    assert_eq!(after.state, ProposalState::Pending);
}

// DCP-FR-XDRV / PDC-FR-GMWR: a rejected change is revised in place and is
// undecided after it, with `decided_at` and `edited` cleared and its revision
// bumped; it is a revision target, and an accepted change is not.
#[test]
fn a_rejected_change_is_revised_and_pending_again() {
    let f = Fixture::new();
    let (p, first, second) = first_rejected(&f);
    assert_eq!(revision_target(&f.root, &f.draft_id, &first), RevisionTarget::Revisable);

    let after = crate::draft_proposals::revise_hunk_impl(
        &f.root,
        &f.draft_id,
        &first,
        &replace("original", "first"),
    )
    .expect("revised");

    let row = &after.ledger[0];
    assert_eq!(row.id, first, "the id a reply names is kept");
    assert_eq!(row.state, HunkState::Pending);
    assert_eq!(row.decided_at, None);
    assert!(!row.edited);
    assert_eq!(row.revision, p.ledger[0].revision + 1);
    assert_eq!(after.counts.undecided(), 2);

    f.accept_hunk(&p.id, &second).expect("accepted");
    assert_eq!(revision_target(&f.root, &f.draft_id, &second), RevisionTarget::NoSuchChange);
    assert_eq!(
        crate::draft_proposals::revise_hunk_impl(
            &f.root,
            &f.draft_id,
            &second,
            &replace("sections", "chapters"),
        )
        .expect_err("refused"),
        ERR_HUNK_ALREADY_DECIDED,
    );
}
