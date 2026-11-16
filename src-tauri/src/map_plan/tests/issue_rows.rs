//! MPR-FR-YCHW, MPR-FR-FBSH, MPR-FR-RTGH, MPR-FR-EQAX, MPR-FR-NGSK, MPR-FR-WBKZ,
//! MPR-FR-GOHN, MPR-FR-SGYA: the rows that write to GitHub.

use super::*;
use crate::delivery::github::tests::fake::listen;

fn cached_issue(
    plan: &Plan,
    number: u64,
) -> Option<crate::delivery::github::adapter::TrackerIssue> {
    plan.h.cache().and_then(|read| read.issue(number).cloned())
}

/// MPR-FR-YCHW, MPR-FR-SGYA: accepting a `create_issue` row creates the issue
/// with its Type, milestone, parent, and labels, adds it to the Project, holds
/// the number and the URL, and updates the read cache.
#[test]
fn accepting_a_new_issue_creates_it() {
    let plan = setup();
    let fields = CreateIssueFields {
        title: "Build the thing".into(),
        body: Some("Details".into()),
        issue_type: Some("Task".into()),
        milestone: Some(MilestoneRef { number: 1, title: "Milestone 1".into() }),
        parent: Some(ParentRef::Issue(1)),
        labels: vec!["BUG".into()],
    };
    let proposal = record(&plan, vec![create_row("a", fields)]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    let row = row_of(&decided, "a");
    assert_eq!(row.state, RowState::Accepted);
    let result = row.result.clone().unwrap();
    assert_eq!(result.issue_number, Some(101));
    assert_eq!(result.issue_url.as_deref(), Some("https://github.com/acme/widgets/issues/101"));
    let writes = plan.h.fake.writes();
    assert_eq!(
        writes,
        ["create_issue", "add_project_item PVT_delivery1 I_101", "set_parent 101 Some(1)"]
    );
    let made = plan.h.fake.lock().issues.iter().find(|i| i.number == 101).cloned().unwrap();
    assert_eq!(
        (made.title.as_str(), made.body.as_str(), made.issue_type.as_deref(), made.milestone),
        ("Build the thing", "Details", Some("Task"), Some(1))
    );
    assert_eq!(plan.h.fake.lock().issue_labels[&101], ["bug"], "the repository spelling");
    assert!(cached_issue(&plan, 101).is_some(), "the read cache holds the new issue");
    assert_eq!(listed(&plan)[0], decided);
}

/// MPR-FR-YLKL, MPR-FR-EQAX: a child waits for its parent row. Once the parent
/// is accepted, the child is created with the real number of the parent issue.
#[test]
fn a_child_row_takes_the_number_of_its_parent() {
    let plan = setup();
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("parent".into()));
    let proposal =
        record(&plan, vec![create_row("child", child), create_row("parent", new_issue("Parent"))]);
    let refused = accept(&plan, &proposal, "child").unwrap_err();
    assert_eq!(refused.code, "parent_not_accepted");
    assert!(refused.message.contains("waiting"));
    assert!(plan.h.fake.writes().is_empty(), "a refusal writes nothing");
    assert_eq!(state_of(&listed(&plan)[0], "child"), RowState::Waiting);
    let parent = accept(&plan, &proposal, "parent").unwrap();
    let parent_number = row_of(&parent, "parent").result.as_ref().unwrap().issue_number.unwrap();
    assert_eq!(row_of(&parent, "child").blocked_by, None, "the child is free");
    let done = accept(&plan, &proposal, "child").unwrap();
    let child_number = row_of(&done, "child").result.as_ref().unwrap().issue_number.unwrap();
    let writes = plan.h.fake.writes();
    assert!(writes.contains(&format!("set_parent {child_number} Some({parent_number})")));
}

/// MPR-FR-YLKL, MPR-FR-EQAX, MPR-FR-SKLA: a child of a rejected parent stays
/// blocked for good, with the reason, and can still be rejected.
#[test]
fn a_child_of_a_rejected_parent_stays_blocked() {
    let plan = setup();
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("parent".into()));
    let proposal =
        record(&plan, vec![create_row("parent", new_issue("Parent")), create_row("child", child)]);
    reject(&plan, &proposal, "parent").unwrap();
    let refused = accept(&plan, &proposal, "child").unwrap_err();
    assert_eq!(refused.code, "parent_not_accepted");
    assert!(refused.message.contains("rejected"));
    let rejected = reject(&plan, &proposal, "child").unwrap();
    assert_eq!(state_of(&rejected, "child"), RowState::Rejected);
    assert!(plan.h.fake.calls().is_empty(), "no request was made");
}

/// MPR-FR-YLKL: a child of a failed parent is blocked with the reason `failed`.
#[test]
fn a_child_of_a_failed_parent_is_blocked() {
    let plan = setup();
    let mut parent = new_issue("Parent");
    parent.labels = vec!["nope".into()];
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("parent".into()));
    let proposal = record(&plan, vec![create_row("parent", parent), create_row("child", child)]);
    let failed = accept(&plan, &proposal, "parent").unwrap();
    assert_eq!(state_of(&failed, "parent"), RowState::Failed);
    assert_eq!(
        row_of(&failed, "child").blocked_by,
        Some(RowBlock { row_key: "parent".into(), reason: RowState::Failed })
    );
    let refused = accept(&plan, &proposal, "child").unwrap_err();
    assert!(refused.message.contains("failed"));
}

/// MPR-FR-NGSK, MPR-FR-QUYY, MPR-FR-MQEQ: a label that is not in the repository
/// fails the row with `label_unknown` at the accept, and nothing is written. A
/// failed row is final, and it leaves the other rows as they were.
#[test]
fn an_unknown_label_fails_the_row_for_good() {
    let plan = setup();
    let mut fields = new_issue("Labelled");
    fields.labels = vec!["no-such-label".into()];
    let proposal = record(&plan, vec![create_row("a", fields), draft_row("b", "Other", "x")]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    let row = row_of(&decided, "a");
    assert_eq!(row.state, RowState::Failed);
    assert_eq!(row.error.as_ref().unwrap().code, "label_unknown");
    assert!(plan.h.fake.writes().is_empty());
    assert_eq!(state_of(&decided, "b"), RowState::Waiting);
    let again = accept(&plan, &proposal, "a").unwrap_err();
    assert_eq!(again.code, "row_decided");
    let again = reject(&plan, &proposal, "a").unwrap_err();
    assert_eq!(again.code, "row_decided");
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Failed);
    let other = accept(&plan, &proposal, "b").unwrap();
    assert_eq!(state_of(&other, "b"), RowState::Accepted, "a failed row does not stop a later one");
}

/// MPR-FR-YCHW: a Type that the repository does not offer fails the row with
/// `issue_type_unavailable`.
#[test]
fn an_unknown_type_fails_the_row() {
    let plan = setup();
    let mut fields = new_issue("Typed");
    fields.issue_type = Some("Saga".into());
    let proposal = record(&plan, vec![create_row("a", fields)]);
    let decided = accept(&plan, &proposal, "a").unwrap();
    assert_eq!(row_of(&decided, "a").error.as_ref().unwrap().code, "issue_type_unavailable");
}

/// MPR-FR-FBSH, MPR-FR-WBKZ, MPR-FR-GOHN, MPR-FR-GKOL: accepting a change writes
/// each changed field in the order title, body, Type, milestone, parent, labels,
/// Status, close, with the close last and its reason.
#[test]
fn a_change_writes_each_field_in_order() {
    let plan = setup();
    let mut parent = change("parent", json!(1));
    parent.before = json!(null);
    let changes = vec![
        close_change("not_planned"),
        change("status", json!("Done")),
        label_change(&["docs"], &["bug"]),
        parent,
        change("milestone", json!({ "number": 1, "title": "Milestone 1" })),
        change("type", json!("Bug")),
        change("body", json!("New body")),
        change("title", json!("New title")),
    ];
    plan.h.fake.lock().issue_labels.insert(2, vec!["bug".into()]);
    let proposal = record(&plan, vec![modify_row("m", 2, changes)]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    let row = row_of(&decided, "m");
    assert_eq!(row.state, RowState::Accepted, "{:?}", row.error);
    let result = row.result.clone().unwrap();
    assert_eq!(result.issue_number, Some(2));
    assert_eq!(
        result.applied_changes.unwrap(),
        ["title", "body", "type", "milestone", "parent", "labels", "status", "close"]
    );
    let writes = plan.h.fake.writes();
    let order: Vec<&str> = writes.iter().map(|w| w.split(' ').next().unwrap()).collect();
    assert_eq!(order[0], "update_issue");
    assert_eq!(order.last(), Some(&"close_issue"));
    assert_eq!(writes.last().unwrap(), "close_issue 2 NotPlanned");
    assert!(writes.contains(&"set_milestone 2 Some(1)".to_string()));
    let issue = cached_issue(&plan, 2).unwrap();
    assert_eq!((issue.title.as_str(), issue.state.as_str()), ("New title", "closed"));
    assert_eq!(issue.issue_type.as_deref(), Some("Bug"));
}

/// MPR-FR-WBKZ: the close reason `completed` closes with that reason.
#[test]
fn a_change_closes_as_completed() {
    let plan = setup();
    let proposal = record(&plan, vec![modify_row("m", 3, vec![close_change("completed")])]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Accepted);
    assert_eq!(plan.h.fake.writes(), ["close_issue 3 Completed"]);
}

/// MPR-FR-FBSH: the first failed write stops the row. The row is `failed` with
/// `partial_edit`, the writes that happened stay, and the later fields are not
/// written.
#[test]
fn the_first_failed_write_stops_the_row() {
    let plan = setup();
    let changes = vec![
        change("title", json!("New title")),
        change("body", json!("New body")),
        change("status", json!("Done")),
    ];
    let proposal = record(&plan, vec![modify_row("m", 2, changes), draft_row("d", "Other", "x")]);
    plan.h.fake.lock().writes_before_failure = Some(1);
    let decided = accept(&plan, &proposal, "m").unwrap();
    let row = row_of(&decided, "m");
    assert_eq!(row.state, RowState::Failed);
    assert_eq!(row.error.as_ref().unwrap().code, "partial_edit");
    assert_eq!(plan.h.fake.writes(), ["update_issue 2", "update_issue 2"]);
    assert_eq!(cached_issue(&plan, 2).unwrap().title, "New title", "the cache holds the write");
    assert_eq!(state_of(&decided, "d"), RowState::Waiting);
}

/// MPR-FR-FBSH: where the first write fails, the row carries the error of that
/// write.
#[test]
fn a_first_write_that_fails_has_its_own_error() {
    let plan = setup();
    let proposal = record(&plan, vec![modify_row("m", 2, vec![change("title", json!("T"))])]);
    plan.h.fake.lock().writes_before_failure = Some(0);
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&decided, "m").error.as_ref().unwrap().code, "github_request_failed");
}

/// MPR-FR-RTGH: an issue that changed on GitHub since the proposal fails the
/// row with `changed_on_tracker`, writes nothing, and changes no other row.
#[test]
fn a_changed_issue_fails_the_row() {
    let plan = setup();
    let mut stale = modify_row("m", 2, vec![change("title", json!("T"))]);
    if let RowFields::ModifyIssue(f) = &mut stale.fields {
        f.base_updated_at = "2025-12-31T00:00:00Z".into();
    }
    let proposal =
        record(&plan, vec![stale, modify_row("n", 3, vec![change("title", json!("U"))])]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    let row = row_of(&decided, "m");
    assert_eq!(row.state, RowState::Failed);
    assert_eq!(row.error.as_ref().unwrap().code, "changed_on_tracker");
    assert!(plan.h.fake.writes().is_empty());
    assert_eq!(state_of(&decided, "n"), RowState::Waiting);
    let next = accept(&plan, &proposal, "n").unwrap();
    assert_eq!(state_of(&next, "n"), RowState::Accepted);
}

/// MPR-FR-NGSK, MPR-FR-FBSH: the label check of a change runs again at the
/// accept.
#[test]
fn a_change_rechecks_its_labels() {
    let plan = setup();
    let proposal = record(&plan, vec![modify_row("m", 2, vec![label_change(&["ghost"], &[])])]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&decided, "m").error.as_ref().unwrap().code, "label_unknown");
    assert!(plan.h.fake.writes().is_empty());
}

/// MPR-FR-GOHN: a Status that the Project does not offer fails the row with
/// `status_unknown`.
#[test]
fn an_unknown_status_fails_the_row() {
    let plan = setup();
    let proposal = record(&plan, vec![modify_row("m", 2, vec![change("status", json!("Limbo"))])]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&decided, "m").error.as_ref().unwrap().code, "status_unknown");
    assert!(plan.h.fake.writes().is_empty());
}

/// MPR-FR-FBSH: a stored change that cannot be applied fails the row.
#[test]
fn an_unreadable_change_fails_the_row() {
    let plan = setup();
    let proposal = record(&plan, vec![modify_row("m", 2, vec![change("title", json!(5))])]);
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&decided, "m").error.as_ref().unwrap().code, "row_invalid");
    assert!(plan.h.fake.calls().is_empty());
}

/// MPR-FR-SGYA: each recording and each decision emits the event with the
/// discussion id.
#[test]
fn each_change_emits_the_event() {
    let plan = setup();
    let seen = listen(&plan.app(), MAP_PLAN_PROPOSALS_CHANGED);
    assert_eq!(MAP_PLAN_PROPOSALS_CHANGED, "map-plan-proposals-changed");
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), draft_row("b", "Two", "y")]);
    accept(&plan, &proposal, "a").unwrap();
    reject(&plan, &proposal, "b").unwrap();
    crate::delivery::github::tests::wait_until("three events", || seen.lock().unwrap().len() >= 3);
    let events = seen.lock().unwrap().clone();
    assert!(events.iter().all(|e| *e == format!("{{\"discussionId\":\"{}\"}}", plan.discussion)));
}
