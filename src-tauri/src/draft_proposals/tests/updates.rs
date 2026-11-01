//! An update of the open proposal at the store (DCP-FR-XDRV, DCP-FR-TQBO,
//! DCP-FR-IAFV, DCP-FR-BFDR, DCP-FR-FKMH, DCP-FR-PHUB, DCP-FR-VTYX,
//! DCP-FR-MKIM, DCP-FR-PWSF, DCP-FR-16).

use super::*;
use crate::draft_proposals::anchors::HunkKind;
use crate::draft_proposals::hunks::{HunkState, ProposedHunk};
use crate::text_hunks::updates::UpdateEntry;

fn replace(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    }
}

fn two_changes(f: &Fixture) -> (DraftChangeProposal, String, String) {
    let p = f
        .propose_hunks(FILE, &[replace("original", "revised"), replace("paragraphs", "sections")])
        .expect("recorded");
    let (first, second) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());
    (p, first, second)
}

fn update(f: &Fixture, updates: &[UpdateEntry]) -> Result<DraftChangeProposal, UpdateRefusal> {
    update_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        ProposalUpdate {
            draft_id: &f.draft_id,
            updates,
            rationale: "Asked for. <spoken>Changed.</spoken>",
            agent: &agent(),
            target: f.target(),
            thread_id: &f.thread_id,
        },
    )
}

fn withdraw(id: &str) -> UpdateEntry {
    UpdateEntry::Withdraw { id: id.into() }
}

// DCP-FR-XDRV: no open proposal, nothing to update; an empty list is refused.
#[test]
fn only_the_open_proposal_is_updated() {
    let f = Fixture::new();
    assert_eq!(update(&f, &[withdraw("x")]), Err(UpdateRefusal::NoOpenProposal));
    let (p, first, second) = two_changes(&f);
    assert_eq!(update(&f, &[]), Err(UpdateRefusal::NoUpdates));
    f.decline(&p.id).expect("declined");
    assert_eq!(update(&f, &[withdraw(&first)]), Err(UpdateRefusal::NoOpenProposal));
    assert_eq!(update(&f, &[withdraw(&second)]), Err(UpdateRefusal::NoOpenProposal));
}

// DCP-FR-TQBO / DCP-FR-IAFV / DCP-FR-PWSF: a change keeps the id and resets a
// rejected hunk to pending; a withdraw keeps the hunk as `withdrawn`; an add
// goes last; counts carry `withdrawn`.
#[test]
fn an_update_changes_withdraws_and_adds() {
    let f = Fixture::new();
    let (p, first, second) = two_changes(&f);
    f.reject_hunk(&p.id, &first).expect("rejected");
    let after = update(
        &f,
        &[
            UpdateEntry::Change { id: first.clone(), hunk: replace("original", "first") },
            withdraw(&second),
            UpdateEntry::Add(replace("Spec", "Specification")),
        ],
    )
    .expect("updated");
    assert_eq!(after.ledger.len(), 3);
    assert_eq!(after.ledger[0].id, first);
    assert_eq!(after.ledger[0].state, HunkState::Pending, "a rejected hunk is pending again");
    assert_eq!(after.ledger[0].decided_at, None);
    assert_eq!(after.ledger[0].revision, 1);
    assert_eq!(after.ledger[1].state, HunkState::Withdrawn);
    assert_eq!(after.ledger[2].state, HunkState::Pending);
    assert_eq!(after.counts.withdrawn, 1);
    assert_eq!(after.counts.undecided(), 2);
    assert_eq!(after.hunk_count, 3);
    assert_eq!(f.reread(&p.id), after, "the record on disk is the one returned");
}

// DCP-FR-BFDR / DCP-FR-FKMH: an accepted hunk is not updatable, the refusal
// carries the place, and the proposal is left as it was.
#[test]
fn an_accepted_hunk_is_not_updatable() {
    let f = Fixture::new();
    let (p, first, second) = two_changes(&f);
    f.accept_hunk(&p.id, &first).expect("accepted");
    let before = f.reread(&p.id);
    assert_eq!(
        update(&f, &[withdraw(&second), withdraw(&first)]),
        Err(UpdateRefusal::HunkNotUpdatable { at: 2, of: 2 })
    );
    assert_eq!(f.reread(&p.id), before);
}

// DCP-FR-MKIM: no decision, hold, or edit reaches a withdrawn hunk, and a
// decline leaves it withdrawn.
#[test]
fn a_withdrawn_hunk_takes_no_decision() {
    let f = Fixture::new();
    let (p, first, second) = two_changes(&f);
    update(&f, &[withdraw(&second)]).expect("updated");
    assert_eq!(f.accept_hunk(&p.id, &second).expect_err("refused"), ERR_HUNK_WITHDRAWN);
    assert_eq!(f.reject_hunk(&p.id, &second).expect_err("refused"), ERR_HUNK_WITHDRAWN);
    let checksum = load_hunks_impl(&f.root, &p.id).expect("hunks").checksum;
    assert_eq!(
        f.edit_hunk(&p.id, &second, "mine", &checksum).expect_err("refused"),
        ERR_HUNK_WITHDRAWN
    );
    f.decline(&p.id).expect("declined");
    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].id, first);
    assert_eq!(after.ledger[0].state, HunkState::Rejected);
    assert_eq!(after.ledger[1].state, HunkState::Withdrawn);
    assert_eq!(after.state, ProposalState::Rejected);
}

// DCP-FR-PWSF / DCP-FR-15 / DCP-FR-16: an update that leaves no undecided hunk
// closes the proposal, appends its own comment and no decision comment, and the
// event names the update as its cause.
#[test]
fn an_update_that_closes_the_proposal_appends_its_comment_alone() {
    let f = Fixture::new();
    let (p, first, second) = two_changes(&f);
    f.accept_hunk(&p.id, &first).expect("accepted");
    let seen = f.watch_proposals();
    let comments = f.discussion().comments.len();
    let after = update(&f, &[withdraw(&second)]).expect("updated");
    assert_eq!(after.state, ProposalState::Accepted);
    assert!(after.decided_at.is_some());
    let thread = f.discussion();
    assert_eq!(thread.comments.len(), comments + 1, "the update's own comment only");
    assert!(matches!(
        thread.comments.last().expect("comment").attachments[0],
        comments::Attachment::Proposal { update: Some(_), .. }
    ));
    let seen = seen.lock().expect("events");
    assert_eq!(seen.len(), 1, "one event per update");
    assert_eq!(seen[0]["cause"], "updated");
    assert_eq!(seen[0]["proposal"]["state"], "accepted");
}

// DCP-FR-VTYX: an update whose comment cannot be appended leaves both files as
// they were.
#[test]
fn an_update_that_cannot_append_its_comment_writes_nothing() {
    let f = Fixture::new();
    let (p, first, _) = two_changes(&f);
    let hunks_before = load_hunks_impl(&f.root, &p.id).expect("hunks");
    f.resolve_conversation();
    assert_eq!(
        update(&f, &[UpdateEntry::Change { id: first, hunk: replace("original", "x") }]),
        Err(UpdateRefusal::ThreadResolved)
    );
    assert_eq!(f.reread(&p.id), p);
    assert_eq!(load_hunks_impl(&f.root, &p.id).expect("hunks"), hunks_before);
}

/// Give the fixture's app a project state pointing at its root, which the
/// commands and the statistics recorder resolve against.
fn with_project(f: &Fixture) {
    use tauri::Manager;
    f.app.manage(crate::project::ProjectState::default());
    f.app.state::<crate::project::ProjectState>().set_root(f.root.path().to_path_buf());
}

// DCP-FR-MKIM / DCP-FR-16: a hold of a withdrawn hunk is refused and emits
// nothing; a hold of a live hunk emits `held`, and a recording `recorded`.
#[test]
fn a_hold_of_a_withdrawn_hunk_is_refused_and_the_causes_are_named() {
    use tauri::Manager;
    let f = Fixture::new();
    with_project(&f);
    let seen = f.watch_proposals();
    let (p, first, second) = two_changes(&f);
    update(&f, &[withdraw(&second)]).expect("updated");
    let hold = |hunk: &str| {
        set_draft_change_hunk_discussing(
            f.handle(),
            p.id.clone(),
            hunk.to_string(),
            true,
            f.app.state::<crate::project::ProjectState>(),
        )
    };
    let before = f.reread(&p.id);
    assert_eq!(hold(&second).expect_err("refused"), ERR_HUNK_WITHDRAWN);
    assert_eq!(f.reread(&p.id), before, "nothing was written");
    hold(&first).expect("held");
    let causes: Vec<String> = seen
        .lock()
        .expect("events")
        .iter()
        .map(|event| event["cause"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(causes, ["recorded", "updated", "held"]);
}

// DCP-FR-WTKA: an update that closes the proposal contributes its one
// statistics line; an update that leaves it open contributes none.
#[test]
fn an_update_that_closes_the_proposal_contributes_its_statistics_line() {
    let f = Fixture::new();
    with_project(&f);
    let (p, first, second) = two_changes(&f);
    update(&f, &[withdraw(&second)]).expect("updated");
    crate::statistics::wait_for_writer();
    let open = crate::statistics::read_statistics_impl(&f.root, &f.draft_id).expect("stats");
    assert_eq!(open.totals.rejected_proposals.value.unwrap_or(0), 0);
    assert_eq!(open.totals.accepted_proposals.value.unwrap_or(0), 0);

    update(&f, &[withdraw(&first)]).expect("updated");
    assert_eq!(f.reread(&p.id).state, ProposalState::Rejected);
    crate::statistics::wait_for_writer();
    let closed = crate::statistics::read_statistics_impl(&f.root, &f.draft_id).expect("stats");
    assert_eq!(closed.totals.rejected_proposals.value, Some(1));
}

// DCP-FR-TQBO: a change clears the author's own edit of that hunk, and the
// `.hunks` file holds the agent's new text.
#[test]
fn a_change_clears_the_authors_edit() {
    let f = Fixture::new();
    let (p, first, _) = two_changes(&f);
    let checksum = load_hunks_impl(&f.root, &p.id).expect("hunks").checksum;
    f.edit_hunk(&p.id, &first, "mine", &checksum).expect("edited");
    assert!(f.reread(&p.id).ledger[0].edited);
    let after = update(
        &f,
        &[UpdateEntry::Change { id: first.clone(), hunk: replace("original", "theirs") }],
    )
    .expect("updated");
    assert!(!after.ledger[0].edited);
    assert!(!after.candidate_edited);
    let hunks = load_hunks_impl(&f.root, &p.id).expect("hunks").hunks;
    assert_eq!(hunks[0].id, first);
    assert_eq!(hunks[0].after.as_deref(), Some("theirs"));
}

// DCP-FR-IAFV: an added hunk takes a new id, and the `.hunks` file keeps a
// withdrawn hunk.
#[test]
fn an_add_mints_a_new_id_and_a_withdrawn_hunk_stays_in_the_file() {
    let f = Fixture::new();
    let (p, first, second) = two_changes(&f);
    let after =
        update(&f, &[withdraw(&second), UpdateEntry::Add(replace("Spec", "Specification"))])
            .expect("updated");
    let added = &after.ledger[2].id;
    assert!(added != &first && added != &second, "a new id");
    let hunks = load_hunks_impl(&f.root, &p.id).expect("hunks").hunks;
    assert_eq!(hunks.len(), 3);
    assert_eq!(hunks[1].id, second, "the withdrawn hunk is kept");
    assert_eq!(&hunks[2].id, added);
}
