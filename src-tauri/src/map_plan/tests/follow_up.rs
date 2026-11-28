//! MPR-FR-IANE, MPR-FR-LAAK, MPR-FR-LDJH, MPR-FR-BCKW, MPR-FR-JSJE,
//! MPR-FR-VNDV, MPR-FR-MVRX: the follow-up turn after the author decided every
//! row of a proposal.

use super::super::follow_up::{
    follow_up_body, request_deferred, reset_deferred_for_test, wants_follow_up, TurnRunsSeam,
};
use super::super::reprepare::TurnRequest;
use super::*;
use crate::comments::Comment;

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

fn synthesis_comments(plan: &Plan) -> Vec<Comment> {
    comments(plan).into_iter().filter(|c| c.author == Participant::synthesis()).collect()
}

/// A typed new issue, so the plan holds it once it is created.
fn typed_issue(title: &str) -> CreateIssueFields {
    let mut issue = new_issue(title);
    issue.issue_type = Some("Task".into());
    issue
}

/// A change to issue `number` whose accept fails with `changed_on_tracker`.
fn stale_row(key: &str, number: u64) -> MapPlanRow {
    let mut row = modify_row(key, number, vec![change("title", json!(format!("T{number}")))]);
    if let RowFields::ModifyIssue(f) = &mut row.fields {
        f.base_updated_at = "2025-12-31T00:00:00Z".into();
    }
    row
}

// MPR-FR-IANE, MPR-FR-LAAK, MPR-FR-LDJH: the last decision of a proposal with
// an accepted row appends one Synthesis comment and starts one turn of the
// agent that proposed, with the comment as its trigger.
#[test]
fn the_last_decision_starts_one_follow_up_turn() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(
        &plan,
        vec![
            create_row("a", typed_issue("Card capture")),
            create_row("b", typed_issue("Refunds")),
            draft_row("c", "Relay", "x"),
        ],
    );
    accept(&plan, &proposal, "a").unwrap();
    assert!(turns(&plan).is_empty(), "a row still waits");
    reject(&plan, &proposal, "b").unwrap();
    assert!(turns(&plan).is_empty(), "a row still waits");
    let decided = accept(&plan, &proposal, "c").unwrap();
    let asked = turns(&plan);
    assert_eq!(asked.len(), 1);
    let synthesis = synthesis_comments(&plan);
    assert_eq!(synthesis.len(), 1);
    assert_eq!(asked[0].trigger_comment_id, synthesis[0].id);
    assert_eq!(asked[0].discussion_id, plan.discussion);
    assert!(decided.follow_up_requested_at.is_some());
    let number = row_of(&decided, "a").result.as_ref().unwrap().issue_number.unwrap();
    let body = &synthesis[0].body;
    assert!(body.contains(&format!("Accepted: #{number} \"Card capture\"")), "{body}");
    assert!(body.contains("the new change \"Relay\""), "{body}");
    assert!(body.contains("Rejected: the new issue \"Refunds\"."), "{body}");
    assert!(body.contains("You do not have to propose."), "{body}");
    assert!(body.contains(&format!("@{}", asked[0].nickname)), "{body}");
}

// MPR-FR-LAAK: a failed row is named with its error code, and an accepted row
// with its URL.
#[test]
fn the_body_names_urls_and_error_codes() {
    let mut proposal = MapPlanProposal {
        id: "p".into(),
        discussion_id: "d".into(),
        owner: None,
        comment_id: "c".into(),
        created_at: AT.into(),
        reprepare_requested_at: None,
        follow_up_requested_at: None,
        rows: vec![create_row("a", new_issue("Epic")), modify_row("m", 7, Vec::new())],
    };
    proposal.rows[0].state = RowState::Accepted;
    proposal.rows[0].result = Some(RowResult {
        issue_number: Some(57),
        issue_url: Some("https://github.com/acme/widgets/issues/57".into()),
        draft_id: None,
        applied_changes: None,
    });
    proposal.rows[1].state = RowState::Failed;
    proposal.rows[1].error = Some(RowError { code: "label_unknown".into(), message: "m".into() });
    let body = follow_up_body(&proposal, "arch");
    assert!(body.contains("Accepted: #57 \"Epic\" (https://github.com/acme/widgets/issues/57)."));
    assert!(body.contains("Failed: #7 \"Issue 7\" (label_unknown)."), "{body}");
    assert!(body.ends_with("You do not have to propose."));
}

// MPR-FR-IANE: a proposal whose rows are all rejected gets no follow-up.
#[test]
fn an_all_rejected_proposal_gets_no_follow_up() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
    reject(&plan, &proposal, "a").unwrap();
    reject(&plan, &proposal, "b").unwrap();
    assert!(turns(&plan).is_empty());
    assert!(synthesis_comments(&plan).is_empty());
}

// MPR-FR-IANE: a row that failed with `changed_on_tracker` leaves the turn to
// the re-prepare request, so the decision starts exactly one turn.
#[test]
fn a_changed_on_tracker_failure_leaves_the_turn_to_the_re_prepare() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x"), stale_row("s", 2)]);
    accept(&plan, &proposal, "a").unwrap();
    let decided = accept(&plan, &proposal, "s").unwrap();
    assert_eq!(state_of(&decided, "s"), RowState::Failed);
    assert_eq!(turns(&plan).len(), 1, "one turn, the re-prepare");
    assert!(decided.reprepare_requested_at.is_some());
    assert!(decided.follow_up_requested_at.is_none());
}

// MPR-FR-JSJE, MPR-FR-MVRX: a reopen and a new decision after a follow-up do
// not ask again, and the request stays recorded.
#[test]
fn a_proposal_has_one_follow_up() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
    accept(&plan, &proposal, "a").unwrap();
    reject(&plan, &proposal, "b").unwrap();
    assert_eq!(turns(&plan).len(), 1);
    let reopened = reopen_row(&plan.app(), &plan.discussion, &proposal.id, "b").unwrap();
    assert!(reopened.follow_up_requested_at.is_some(), "the request stays");
    reject(&plan, &proposal, "b").unwrap();
    assert_eq!(turns(&plan).len(), 1, "no second follow-up");
}

// MPR-FR-LOUI, MPR-FR-LDJH: a dispatch that fails records no request, and the
// appended comment stays.
#[test]
fn a_failed_dispatch_records_no_request() {
    reset_deferred_for_test();
    let plan = setup();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_not_found".into());
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    assert_eq!(state_of(&decided, "a"), RowState::Accepted, "the decision stands");
    assert!(decided.follow_up_requested_at.is_none());
    assert_eq!(synthesis_comments(&plan).len(), 1, "the comment stays");
}

// MPR-FR-JSJE: a resolved discussion gets no follow-up turn.
#[test]
fn a_resolved_discussion_gets_no_follow_up() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &plan.discussion,
        true,
        &human(),
        AT,
    )
    .unwrap();
    let decided = accept(&plan, &proposal, "a").unwrap();
    assert!(decided.follow_up_requested_at.is_none());
    assert!(turns(&plan).is_empty());
}

// MPR-FR-IANE: the conditions of a follow-up, on a proposal in memory.
#[test]
fn a_follow_up_wants_a_decided_proposal_with_an_accepted_row() {
    let mut proposal = MapPlanProposal {
        id: "p".into(),
        discussion_id: "d".into(),
        owner: None,
        comment_id: "c".into(),
        created_at: AT.into(),
        reprepare_requested_at: None,
        follow_up_requested_at: None,
        rows: vec![draft_row("a", "A", "x")],
    };
    assert!(!wants_follow_up(&proposal), "a row waits");
    proposal.rows[0].state = RowState::Accepted;
    assert!(wants_follow_up(&proposal));
    proposal.reprepare_requested_at = Some(AT.into());
    assert!(!wants_follow_up(&proposal), "a re-prepare was asked");
    proposal.reprepare_requested_at = None;
    proposal.follow_up_requested_at = Some(AT.into());
    assert!(!wants_follow_up(&proposal), "a follow-up was asked");
}

/// A seam that says a turn of the agent runs while the flag is set.
fn turn_running(plan: &Plan) -> Arc<std::sync::atomic::AtomicBool> {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
    plan.h.handle().manage(TurnRunsSeam(Arc::clone(&flag)));
    flag
}

// MPR-FR-BCKW, MPR-FR-VNDV: a decision while a turn of the agent runs defers
// the follow-up. When that turn ends, the deferred follow-up is requested
// once, and a turn end in another discussion requests nothing.
#[test]
fn a_running_turn_defers_the_follow_up_to_its_end() {
    reset_deferred_for_test();
    let plan = setup();
    let running = turn_running(&plan);
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    assert!(decided.follow_up_requested_at.is_none(), "deferred");
    assert!(turns(&plan).is_empty());
    assert!(synthesis_comments(&plan).is_empty());

    running.store(false, std::sync::atomic::Ordering::SeqCst);
    request_deferred(&plan.app(), "another-discussion");
    assert!(turns(&plan).is_empty(), "another discussion requests nothing");
    request_deferred(&plan.app(), &plan.discussion);
    assert_eq!(turns(&plan).len(), 1);
    assert_eq!(synthesis_comments(&plan).len(), 1);
    let stored = crate::map_plan::proposals_in(&plan.store(), &plan.discussion);
    assert!(stored[0].follow_up_requested_at.is_some());
    request_deferred(&plan.app(), &plan.discussion);
    assert_eq!(turns(&plan).len(), 1, "the deferral is gone once requested");
}

// MPR-FR-BCKW: a turn that still runs at the check defers the follow-up again.
#[test]
fn a_turn_that_still_runs_defers_again() {
    reset_deferred_for_test();
    let plan = setup();
    let running = turn_running(&plan);
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    accept(&plan, &proposal, "a").unwrap();
    request_deferred(&plan.app(), &plan.discussion);
    assert!(turns(&plan).is_empty(), "still deferred");
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    request_deferred(&plan.app(), &plan.discussion);
    assert_eq!(turns(&plan).len(), 1);
}

// MPR-FR-JSJE: an update by the agent that leaves no row waiting requests no
// follow-up.
#[test]
fn an_update_by_the_agent_requests_no_follow_up() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
    accept(&plan, &proposal, "a").unwrap();
    let updated = update_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![MapPlanUpdate::Withdraw("b".into())],
        Some(comment()),
    )
    .unwrap();
    assert!(updated.follow_up_requested_at.is_none());
    assert!(turns(&plan).is_empty());
}

// MPR-FR-VNDV: a requested follow-up logs its outcome and the counts of the
// rows, and no title.
#[test]
fn a_follow_up_logs_its_outcome_and_counts() {
    reset_deferred_for_test();
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "ZEBRA-NAME", "x")]);
    accept(&plan, &proposal, "a").unwrap();
    let records = crate::logging::BUFFER
        .query(&crate::logging::LogFilter::default(), None, 1000)
        .unwrap()
        .records;
    let record = records
        .iter()
        .rev()
        .find(|r| r.message == "map plan follow-up")
        .expect("a follow-up record");
    assert_eq!(record.fields["outcome"], "requested");
    assert_eq!(record.fields["accepted"], 1);
    assert_eq!(record.fields["rejected"], 0);
    assert!(!serde_json::to_string(record).unwrap().contains("ZEBRA"));
}
