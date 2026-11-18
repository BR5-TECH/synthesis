//! MPR-FR-TQVZ, MPR-FR-RVRN, MPR-FR-SBYA, MPR-FR-YLKL: recording a proposal.

use super::*;
use crate::comments::{Attachment, ERR_DISCUSSION_RESOLVED};

fn proposal_files(plan: &Plan) -> Vec<String> {
    let dir = crate::comments::map_proposals_dir(&plan.store()).unwrap();
    plan.store().list_dir(&dir).map(|l| l.into_iter().map(|e| e.name).collect()).unwrap_or_default()
}

/// MPR-FR-TQVZ, MPR-FR-QUYY, MPR-FR-RVRN: a proposal is stored beside the Map
/// log with its rows `waiting`, and its one agent comment carries the
/// reference.
#[test]
fn a_proposal_is_stored_with_its_comment() {
    let plan = setup();
    let mut stale = create_row("a", new_issue("First"));
    stale.state = RowState::Failed;
    let proposal = record(&plan, vec![stale, draft_row("b", "Draft", "Body")]);
    assert_eq!(proposal.discussion_id, plan.discussion);
    assert!(proposal.rows.iter().all(|r| r.state == RowState::Waiting));
    let files = proposal_files(&plan);
    assert_eq!(files, [format!("{}.json", proposal.id)]);
    let thread = crate::comments::find_map_discussion_in(&plan.store(), None).unwrap();
    let last = thread.comments.last().unwrap();
    assert_eq!(last.id, proposal.comment_id);
    assert_eq!(last.author, agent());
    assert_eq!(last.spoken_original.as_deref(), Some("Here is a step."));
    assert!(last.body.starts_with("I propose"));
    assert_eq!(
        last.attachments,
        [Attachment::MapPlanProposal { proposal_id: proposal.id.clone(), update: None }]
    );
    assert_eq!(listed(&plan), [proposal]);
}

/// MPR-FR-RVRN (non-functional): a proposal of 20 rows is one file write.
#[test]
fn twenty_rows_are_one_file() {
    let plan = setup();
    let rows = (0..20).map(|n| create_row(&format!("r{n}"), new_issue("Row"))).collect();
    let proposal = record(&plan, rows);
    assert_eq!(proposal_files(&plan).len(), 1);
    assert_eq!(listed(&plan)[0].rows.len(), 20);
    assert_eq!(proposal.rows.len(), 20);
}

/// MPR-FR-TQVZ: the proposals are listed oldest first.
#[test]
fn proposals_are_listed_oldest_first() {
    let plan = setup();
    let first = record(&plan, vec![draft_row("a", "One", "x")]);
    reject(&plan, &first, "a").unwrap();
    let second = record(&plan, vec![draft_row("a", "Two", "y")]);
    let ids: Vec<String> = listed(&plan).into_iter().map(|p| p.id).collect();
    assert_eq!(ids, [first.id, second.id]);
}

/// MPR-FR-SBYA: a discussion with a `waiting` row refuses a new proposal and
/// records nothing. A row that is decided is no open row.
#[test]
fn an_open_row_refuses_a_new_proposal() {
    let plan = setup();
    assert!(!open_row_exists(&plan.app(), &plan.discussion));
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), draft_row("b", "Two", "y")]);
    assert!(open_row_exists(&plan.app(), &plan.discussion));
    let refused = record_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![draft_row("c", "Three", "z")],
        comment(),
    )
    .unwrap_err();
    assert_eq!(refused.code, "proposal_open");
    assert_eq!(proposal_files(&plan).len(), 1);
    let thread = crate::comments::find_map_discussion_in(&plan.store(), None).unwrap();
    assert_eq!(thread.comments.len(), 2, "the refusal appended no comment");
    reject(&plan, &proposal, "a").unwrap();
    assert!(open_row_exists(&plan.app(), &plan.discussion));
    reject(&plan, &proposal, "b").unwrap();
    assert!(!open_row_exists(&plan.app(), &plan.discussion));
    record(&plan, vec![draft_row("c", "Three", "z")]);
}

/// MPR-FR-SBYA: a failed row and an accepted row are no open row.
#[test]
fn accepted_and_failed_rows_are_not_open() {
    let plan = setup();
    let mut bad = create_row("b", new_issue("Bad"));
    if let RowFields::CreateIssue(f) = &mut bad.fields {
        f.labels = vec!["nope".into()];
    }
    let proposal = record(&plan, vec![draft_row("a", "One", "x"), bad]);
    accept(&plan, &proposal, "a").unwrap();
    assert!(open_row_exists(&plan.app(), &plan.discussion));
    let decided = accept(&plan, &proposal, "b").unwrap();
    assert_eq!(state_of(&decided, "b"), RowState::Failed);
    assert!(!open_row_exists(&plan.app(), &plan.discussion));
}

/// MPR-FR-RVRN: a missing discussion, and an id that is no Map thread, refuse
/// with `discussion_not_found` and record nothing.
#[test]
fn a_missing_discussion_is_refused() {
    let plan = setup();
    for id in ["missing", "../escape"] {
        let refused =
            record_map_plan_proposal(&plan.app(), id, vec![draft_row("a", "One", "x")], comment())
                .unwrap_err();
        assert_eq!(refused.code, "discussion_not_found", "{id}");
    }
    assert!(proposal_files(&plan).is_empty());
}

/// MPR-FR-RVRN: where the comment cannot be appended, no record stays.
#[test]
fn a_failed_comment_leaves_no_record() {
    let plan = setup();
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &plan.discussion,
        true,
        &human(),
        AT,
    )
    .unwrap();
    let refused = record_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![draft_row("a", "One", "x")],
        comment(),
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_DISCUSSION_RESOLVED);
    assert!(proposal_files(&plan).is_empty());
    assert!(listed(&plan).is_empty());
}

/// MPR-FR-YLKL: `blocked_by` is computed at each read and never stored.
#[test]
fn blocked_by_is_computed_and_not_stored() {
    let plan = setup();
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("parent".into()));
    let proposal =
        record(&plan, vec![create_row("parent", new_issue("Parent")), create_row("child", child)]);
    let row = row_of(&proposal, "child");
    assert_eq!(
        row.blocked_by,
        Some(RowBlock { row_key: "parent".into(), reason: RowState::Waiting })
    );
    let dir = crate::comments::map_proposals_dir(&plan.store()).unwrap();
    let text = plan.store().read_text(dir.join(format!("{}.json", proposal.id))).unwrap();
    let stored: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(stored["rows"][1]["blockedBy"], Value::Null);
    assert_eq!(row_of(&proposal, "parent").blocked_by, None);
    reject(&plan, &proposal, "parent").unwrap();
    let after = &listed(&plan)[0];
    assert_eq!(
        row_of(after, "child").blocked_by,
        Some(RowBlock { row_key: "parent".into(), reason: RowState::Rejected })
    );
}

/// MPR-FR-TQVZ: the wire shape of a proposal.
#[test]
fn a_proposal_has_the_wire_shape() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "One", "x")]);
    let value = serde_json::to_value(&proposal).unwrap();
    for key in ["id", "discussionId", "commentId", "createdAt", "rows"] {
        assert!(value.get(key).is_some(), "{key}");
    }
    let row = &value["rows"][0];
    assert_eq!(row["kind"], "new_draft");
    assert_eq!(row["state"], "waiting");
    assert_eq!(row["fields"], json!({ "name": "One", "body": "x" }));
    assert_eq!(row["blockedBy"], Value::Null);
}
