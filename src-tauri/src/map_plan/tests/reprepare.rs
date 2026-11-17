//! MPR-FR-LJRO, MPR-FR-GIJN, MPR-FR-NWEZ, MPR-FR-QIJF, MPR-FR-LOUI,
//! CTA-FR-YWSU: the re-prepare request.

use super::super::reprepare::{request_body, TurnRequest};
use super::*;
use crate::comments::Comment;
use crate::delivery::github::tests::fake::listen;

/// A change to issue `number` whose `updated_at` is older than GitHub's, so
/// its accept fails with `changed_on_tracker`.
fn stale_row(key: &str, number: u64) -> MapPlanRow {
    let mut row = modify_row(key, number, vec![change("title", json!(format!("T{number}")))]);
    if let RowFields::ModifyIssue(f) = &mut row.fields {
        f.base_updated_at = "2025-12-31T00:00:00Z".into();
    }
    row
}

/// A change whose accept fails with `status_unknown`.
fn bad_status_row(key: &str, number: u64) -> MapPlanRow {
    modify_row(key, number, vec![change("status", json!("Limbo"))])
}

fn turns(plan: &Plan) -> Vec<TurnRequest> {
    plan.turns.lock().unwrap().clone()
}

fn comments(plan: &Plan) -> Vec<Comment> {
    crate::comments::fold_map_discussions(&plan.store())
        .into_iter()
        .find(|d| d.id == plan.discussion)
        .unwrap()
        .comments
}

fn request(plan: &Plan, proposal: &MapPlanProposal) -> Result<MapPlanProposal, MapPlanError> {
    request_reprepare(&plan.app(), &plan.discussion, &proposal.id)
}

/// MPR-FR-LJRO, MPR-FR-NWEZ, MPR-FR-QIJF, CTA-FR-YWSU: the decision of the last
/// waiting row, where a row failed with `changed_on_tracker`, appends one
/// Synthesis comment that names each failed issue and the agent, dispatches
/// one turn of the agent that proposed with that comment as its trigger, and
/// records `reprepareRequestedAt`. No request is made while a row waits.
#[test]
fn the_last_decision_requests_a_reprepare_once() {
    let plan = setup();
    let seen = listen(&plan.app(), MAP_PLAN_PROPOSALS_CHANGED);
    let proposal =
        record(&plan, vec![stale_row("m", 2), stale_row("n", 3), draft_row("d", "D", "x")]);
    let before = comments(&plan).len();
    let first = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&first, "m").error.as_ref().unwrap().code, "changed_on_tracker");
    accept(&plan, &proposal, "n").unwrap();
    assert!(turns(&plan).is_empty(), "a row still waits");
    assert_eq!(comments(&plan).len(), before);
    let last = reject(&plan, &proposal, "d").unwrap();
    let turn = turns(&plan);
    assert_eq!(turn.len(), 1);
    assert_eq!(turn[0].nickname, "planner");
    assert_eq!(turn[0].discussion_id, plan.discussion);
    let all = comments(&plan);
    assert_eq!(all.len(), before + 1);
    let asked = all.last().unwrap();
    assert_eq!(asked.id, turn[0].trigger_comment_id);
    assert_eq!(asked.author, crate::comments::Participant::synthesis());
    assert_eq!(
        asked.body,
        "#2 and #3 changed on GitHub after the proposal, so nothing was written. \
         @planner, read these issues again and propose the changes again."
    );
    assert!(last.reprepare_requested_at.is_some());
    assert_eq!(listed(&plan)[0], last);
    let wire = serde_json::to_value(&last).unwrap();
    assert_eq!(wire["reprepareRequestedAt"], json!(last.reprepare_requested_at));
    crate::delivery::github::tests::wait_until("four events", || seen.lock().unwrap().len() >= 4);
}

/// MPR-FR-LJRO: a proposal that records a request asks no second time, also
/// after a reopen and a new decision.
#[test]
fn a_recorded_request_is_not_made_again() {
    let plan = setup();
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    accept(&plan, &proposal, "m").unwrap();
    assert_eq!(turns(&plan).len(), 1);
    reopen_row(&plan.app(), &plan.discussion, &proposal.id, "d").unwrap();
    let again = reject(&plan, &proposal, "d").unwrap();
    assert_eq!(turns(&plan).len(), 1);
    assert!(again.reprepare_requested_at.is_some());
}

/// MPR-FR-LJRO: a failure other than `changed_on_tracker`, and a proposal with
/// no failed row, request nothing on their own.
#[test]
fn other_failures_request_nothing_on_their_own() {
    let plan = setup();
    let proposal = record(&plan, vec![bad_status_row("m", 2), draft_row("d", "D", "x")]);
    accept(&plan, &proposal, "m").unwrap();
    let decided = reject(&plan, &proposal, "d").unwrap();
    assert!(turns(&plan).is_empty());
    assert!(decided.reprepare_requested_at.is_none());
}

/// MPR-FR-LOUI, MPR-FR-LJRO: a turn that cannot be dispatched records no
/// request and leaves the decision a success. The comment that was appended
/// stays. A later decision may ask again.
#[test]
fn a_failed_dispatch_records_no_request() {
    let plan = setup();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_not_found".into());
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    let before = comments(&plan).len();
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Failed);
    assert!(decided.reprepare_requested_at.is_none());
    assert!(listed(&plan)[0].reprepare_requested_at.is_none());
    assert_eq!(turns(&plan).len(), 1, "the dispatch was tried");
    let all = comments(&plan);
    assert_eq!(all.len(), before + 1, "the appended comment stays");
    assert_eq!(all.last().unwrap().author, crate::comments::Participant::synthesis());
    *plan.dispatch_failure.lock().unwrap() = None;
    assert!(request(&plan, &proposal).unwrap().reprepare_requested_at.is_some());
}

/// MPR-FR-LOUI: a comment that cannot be appended starts no turn and records
/// no request. The decision stays a success.
#[test]
fn a_failed_comment_starts_no_turn() {
    let plan = setup();
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &plan.discussion,
        true,
        &human(),
        AT,
    )
    .unwrap();
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Failed);
    assert!(decided.reprepare_requested_at.is_none());
    assert!(turns(&plan).is_empty());
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "discussion_resolved");
    assert!(turns(&plan).is_empty());
}

/// MPR-FR-GIJN, MPR-FR-NWEZ: the author can ask for any failed row. The
/// comment names a row that could not be applied, and the agent.
#[test]
fn the_author_requests_a_reprepare() {
    let plan = setup();
    let proposal = record(&plan, vec![bad_status_row("m", 2)]);
    accept(&plan, &proposal, "m").unwrap();
    assert!(turns(&plan).is_empty());
    let asked = request(&plan, &proposal).unwrap();
    assert!(asked.reprepare_requested_at.is_some());
    assert_eq!(listed(&plan)[0], asked);
    let turn = turns(&plan);
    assert_eq!(turn.len(), 1);
    let last = comments(&plan).pop().unwrap();
    assert_eq!(last.id, turn[0].trigger_comment_id);
    assert_eq!(
        last.body,
        "#2 could not be applied. @planner, read this issue again and propose the change again."
    );
}

/// MPR-FR-GIJN: a request is refused with `nothing_failed`, `proposal_open`, or
/// `reprepare_requested`, and a refusal appends nothing and dispatches nothing.
#[test]
fn a_request_of_the_author_is_refused() {
    let plan = setup();
    let none_failed = record(&plan, vec![draft_row("a", "A", "x")]);
    reject(&plan, &none_failed, "a").unwrap();
    assert_eq!(request(&plan, &none_failed).unwrap_err().code, "nothing_failed");
    let open = record(&plan, vec![bad_status_row("m", 2), draft_row("d", "D", "x")]);
    accept(&plan, &open, "m").unwrap();
    let before = comments(&plan).len();
    assert_eq!(request(&plan, &open).unwrap_err().code, "proposal_open");
    reject(&plan, &open, "d").unwrap();
    request(&plan, &open).unwrap();
    let after = comments(&plan).len();
    assert_eq!(request(&plan, &open).unwrap_err().code, "reprepare_requested");
    assert_eq!(after, before + 1);
    assert_eq!(comments(&plan).len(), after);
    assert_eq!(turns(&plan).len(), 1);
    let app = plan.app();
    assert_eq!(
        request_reprepare(&app, &plan.discussion, "nope").unwrap_err().code,
        "proposal_not_found"
    );
    plan.h.close();
    assert_eq!(request(&plan, &open).unwrap_err().code, "no_project_open");
}

/// MPR-FR-GIJN, MPR-FR-LOUI: a request of the author whose turn cannot be
/// dispatched is refused with the typed error of the dispatch and records no
/// request.
#[test]
fn a_failed_dispatch_refuses_the_request_of_the_author() {
    let plan = setup();
    let proposal = record(&plan, vec![bad_status_row("m", 2)]);
    accept(&plan, &proposal, "m").unwrap();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_unavailable".into());
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "agent_unavailable");
    assert!(listed(&plan)[0].reprepare_requested_at.is_none());
}

/// MPR-FR-NWEZ: the text names each failed row, the changed ones first, and a
/// row that makes something new by its name.
#[test]
fn the_request_names_each_failed_row() {
    let plan = setup();
    let mut proposal = record(&plan, vec![stale_row("m", 18), stale_row("n", 11)]);
    let fail = |row: &mut MapPlanRow, code: &str| {
        row.state = RowState::Failed;
        row.error = Some(crate::map_plan::RowError { code: code.into(), message: String::new() });
    };
    let mut created = create_row("c", new_issue("Docs"));
    fail(&mut created, "label_unknown");
    let mut drafted = draft_row("d", "Relay", "x");
    fail(&mut drafted, "draft_create_failed");
    let mut other = bad_status_row("o", 5);
    fail(&mut other, "status_unknown");
    proposal.rows.iter_mut().for_each(|r| fail(r, "changed_on_tracker"));
    proposal.rows.extend([created, drafted, other, draft_row("w", "Waits", "y")]);
    assert_eq!(
        request_body(&proposal, "planner"),
        "#18 and #11 changed on GitHub after the proposal, so nothing was written. \
         The new issue \"Docs\", the new draft \"Relay\", and #5 could not be applied. \
         @planner, read these issues again and propose the changes again."
    );
}

/// MPR-FR-QUYY: the field is absent on the wire where no request was made, and
/// a stored proposal without it reads as no request.
#[test]
fn a_proposal_without_a_request_has_no_field() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    let wire = serde_json::to_value(&proposal).unwrap();
    assert!(wire.get("reprepareRequestedAt").is_none());
    let back: MapPlanProposal = serde_json::from_value(wire).unwrap();
    assert!(back.reprepare_requested_at.is_none());
}
