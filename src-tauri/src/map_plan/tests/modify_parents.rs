//! MPR-FR-YLKL, MPR-FR-EQAX, MPR-FR-HQXX, MPR-FR-TMTI, MPR-FR-EAAJ: a changed
//! issue that takes a new issue of the same proposal as its parent, and an
//! update with no comment.

use super::*;

/// A change that moves issue 2 under the row `epic`.
fn moved_under(key: &str) -> MapPlanRow {
    let mut parent = change("parent", json!({ "row": key }));
    parent.before = json!({ "issue": 1 });
    modify_row("move", 2, vec![parent])
}

/// A new epic, with a Type that the plan maps, so the plan holds it once it is
/// created.
fn new_epic() -> CreateIssueFields {
    let mut epic = new_issue("Epic");
    epic.issue_type = Some("Epic".into());
    epic
}

fn comments_count(plan: &Plan) -> usize {
    crate::comments::find_map_discussion_in(&plan.store(), None).unwrap().comments.len()
}

/// MPR-FR-YLKL, MPR-FR-EQAX: the change waits for its parent row. Once the
/// parent is accepted, the change writes the real number of the new issue as
/// the parent of issue 2.
#[test]
fn a_changed_issue_takes_the_number_of_its_new_parent() {
    let plan = setup();
    let proposal = record(&plan, vec![create_row("epic", new_epic()), moved_under("epic")]);
    assert_eq!(
        row_of(&listed(&plan)[0], "move").blocked_by,
        Some(RowBlock { row_key: "epic".into(), reason: RowState::Waiting })
    );
    let refused = accept(&plan, &proposal, "move").unwrap_err();
    assert_eq!(refused.code, "parent_not_accepted");
    assert!(plan.h.fake.writes().is_empty());
    let decided = accept(&plan, &proposal, "epic").unwrap();
    let number = row_of(&decided, "epic").result.as_ref().unwrap().issue_number.unwrap();
    let decided = accept(&plan, &proposal, "move").unwrap();
    assert_eq!(
        state_of(&decided, "move"),
        RowState::Accepted,
        "{:?}",
        row_of(&decided, "move").error
    );
    assert!(plan.h.fake.writes().contains(&format!("set_parent 2 Some({number})")));
}

/// MPR-FR-YLKL: a change whose parent row is rejected stays blocked.
#[test]
fn a_change_under_a_rejected_parent_stays_blocked() {
    let plan = setup();
    let proposal = record(&plan, vec![create_row("epic", new_epic()), moved_under("epic")]);
    reject(&plan, &proposal, "epic").unwrap();
    assert_eq!(accept(&plan, &proposal, "move").unwrap_err().code, "parent_not_accepted");
    reject(&plan, &proposal, "move").unwrap();
    assert_eq!(state_of(&listed(&plan)[0], "move"), RowState::Rejected);
}

/// MPR-FR-HQXX: an update that withdraws the parent row of a kept change is
/// refused with `parent_withdrawn`.
#[test]
fn the_parent_row_of_a_change_can_not_be_withdrawn() {
    let plan = setup();
    record(&plan, vec![create_row("epic", new_epic()), moved_under("epic")]);
    let refused = update_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![MapPlanUpdate::Withdraw("epic".into())],
        Some(comment()),
    )
    .unwrap_err();
    assert_eq!(refused.code, "parent_withdrawn");
}

/// MPR-FR-TMTI, MPR-FR-EAAJ: an update with no comment changes the proposal
/// and appends nothing.
#[test]
fn an_update_with_no_comment_appends_nothing() {
    let plan = setup();
    record(&plan, vec![draft_row("a", "A", "a")]);
    let before = comments_count(&plan);
    let after = update_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![MapPlanUpdate::Add(draft_row("b", "B", "b"))],
        None,
    )
    .unwrap();
    assert_eq!(after.rows.len(), 2);
    assert_eq!(comments_count(&plan), before);
}

/// MPR-FR-NSEP, MPR-FR-EAAJ: an update with no comment is refused on a
/// resolved discussion, and the proposal stays as it was.
#[test]
fn a_resolved_discussion_takes_no_update_with_no_comment() {
    let plan = setup();
    record(&plan, vec![draft_row("a", "A", "a")]);
    let before = listed(&plan);
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &plan.discussion,
        true,
        &human(),
        AT,
    )
    .unwrap();
    let refused = update_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![MapPlanUpdate::Add(draft_row("b", "B", "b"))],
        None,
    )
    .unwrap_err();
    assert_eq!(refused.code, crate::comments::ERR_DISCUSSION_RESOLVED);
    assert_eq!(listed(&plan), before);
}
