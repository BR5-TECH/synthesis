//! MPR-FR-QUYY, MPR-FR-MVRX: the reopen of a rejected row.

use super::*;
use crate::delivery::github::tests::fake::listen;

fn reopen(
    plan: &Plan,
    proposal: &MapPlanProposal,
    key: &str,
) -> Result<MapPlanProposal, MapPlanError> {
    reopen_row(&plan.app(), &plan.discussion, &proposal.id, key)
}

/// MPR-FR-MVRX, MPR-FR-QUYY, MPR-FR-SGYA: a rejected row goes back to
/// `waiting`, writes the file, emits the event, and makes no request. The row
/// can then be decided again.
#[test]
fn a_rejected_row_is_reopened() {
    let plan = setup();
    let seen = listen(&plan.app(), MAP_PLAN_PROPOSALS_CHANGED);
    let proposal = record(
        &plan,
        vec![modify_row("m", 2, vec![change("title", json!("T"))]), draft_row("d", "One", "x")],
    );
    reject(&plan, &proposal, "m").unwrap();
    plan.h.fake.lock().calls.clear();
    let reopened = reopen(&plan, &proposal, "m").unwrap();
    let row = row_of(&reopened, "m");
    assert_eq!((row.state, &row.error, &row.result), (RowState::Waiting, &None, &None));
    assert_eq!(state_of(&reopened, "d"), RowState::Waiting);
    assert!(plan.h.fake.calls().is_empty(), "a reopen makes no request");
    assert_eq!(listed(&plan)[0], reopened);
    crate::delivery::github::tests::wait_until("three events", || seen.lock().unwrap().len() >= 3);
    let accepted = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&accepted, "m"), RowState::Accepted);
}

/// MPR-FR-MVRX: a row that is `waiting`, `accepted`, or `failed` is refused
/// with `row_not_rejected`, and nothing changes.
#[test]
fn only_a_rejected_row_is_reopened() {
    let plan = setup();
    let proposal = record(
        &plan,
        vec![
            draft_row("w", "Waits", "x"),
            draft_row("a", "Accepted", "y"),
            modify_row("f", 2, vec![change("status", json!("Limbo"))]),
        ],
    );
    accept(&plan, &proposal, "a").unwrap();
    accept(&plan, &proposal, "f").unwrap();
    let before = listed(&plan);
    for key in ["w", "a", "f"] {
        assert_eq!(reopen(&plan, &proposal, key).unwrap_err().code, "row_not_rejected", "{key}");
    }
    assert_eq!(listed(&plan), before);
}

/// MPR-FR-MVRX, MPR-FR-SBYA: where another proposal of the discussion has an
/// open row, a reopen is refused with `proposal_open`, and nothing changes.
#[test]
fn another_open_proposal_refuses_a_reopen() {
    let plan = setup();
    let first = record(&plan, vec![draft_row("a", "One", "x")]);
    reject(&plan, &first, "a").unwrap();
    let second = record(&plan, vec![draft_row("b", "Two", "y")]);
    assert_eq!(reopen(&plan, &first, "a").unwrap_err().code, "proposal_open");
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Rejected);
    reject(&plan, &second, "b").unwrap();
    assert_eq!(state_of(&reopen(&plan, &first, "a").unwrap(), "a"), RowState::Waiting);
}

/// MPR-FR-MVRX, MPR-FR-SBYA: a row that waits in the same proposal does not
/// refuse a reopen.
#[test]
fn a_row_that_waits_in_the_same_proposal_allows_a_reopen() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), draft_row("b", "Two", "y")]);
    reject(&plan, &proposal, "a").unwrap();
    assert_eq!(state_of(&reopen(&plan, &proposal, "a").unwrap(), "a"), RowState::Waiting);
}

/// MPR-FR-MVRX, MPR-FR-RVTT: an unknown proposal, an unknown row, and no open
/// project are refused with their typed errors.
#[test]
fn a_reopen_of_an_unknown_target_is_refused() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x")]);
    reject(&plan, &proposal, "a").unwrap();
    let app = plan.app();
    assert_eq!(
        reopen_row(&app, &plan.discussion, "nope", "a").unwrap_err().code,
        "proposal_not_found"
    );
    assert_eq!(
        reopen_row(&app, "other", &proposal.id, "a").unwrap_err().code,
        "proposal_not_found"
    );
    assert_eq!(reopen(&plan, &proposal, "z").unwrap_err().code, "row_not_found");
    plan.h.close();
    assert_eq!(reopen(&plan, &proposal, "a").unwrap_err().code, "no_project_open");
}
