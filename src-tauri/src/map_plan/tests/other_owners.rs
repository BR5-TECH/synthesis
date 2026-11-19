//! MPR-FR-TQVZ, MPR-FR-RVRN, MPR-FR-KDTQ: plan proposals of discussions that
//! are not Map threads: a passage thread and a whole discussion of a file.

use super::*;
use crate::agent_conversations::ConversationOrigin;
use crate::comments::{Attachment, Discussion, DiscussionTarget, FragmentTarget};

const FILE: &str = "spec.md";

/// A discussion of the file `spec.md`: a passage thread where `passage` is
/// true, else the whole-file discussion.
fn open_file_discussion(plan: &Plan, passage: bool) -> ConversationOrigin {
    let target = DiscussionTarget::Artifact { artifact_id: FILE.into() };
    let fragment = passage.then(|| FragmentTarget::in_artifact(FILE, 0, 4, "Spec"));
    let thread = crate::comments::open_discussion_unchecked(
        &plan.store(),
        &target,
        fragment,
        "File an issue for this".into(),
        Vec::new(),
        &human(),
        AT,
    )
    .unwrap();
    ConversationOrigin::of(&thread)
}

fn thread_of(plan: &Plan, origin: &ConversationOrigin) -> Discussion {
    crate::comments::locate_discussion_of(&plan.store(), &origin.target, origin.discussion_id())
        .expect("the discussion")
}

fn map_comments(plan: &Plan) -> usize {
    crate::comments::fold_map_discussions(&plan.store())
        .into_iter()
        .find(|d| d.id == plan.discussion)
        .map_or(0, |d| d.comments.len())
}

// MPR-FR-TQVZ, MPR-FR-RVRN, MPR-FR-KDTQ, MPR-FR-SBYA: a passage thread of a
// file records a proposal. The record holds the owner and the file of the
// fragment, the comment goes to the passage thread and not to a Map thread,
// and the open row of that discussion refuses a second recording.
#[test]
fn a_passage_thread_records_a_proposal_in_its_own_log() {
    let plan = setup();
    let origin = open_file_discussion(&plan, true);
    let before = map_comments(&plan);
    let proposal =
        record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("a", "One", "x")], comment())
            .unwrap();
    let owner = proposal.owner.clone().expect("an owner");
    assert_eq!(owner.target, DiscussionTarget::Artifact { artifact_id: FILE.into() });
    assert_eq!(owner.fragment_file.as_deref(), Some(FILE));
    let stored = crate::map_plan::proposals_in(&plan.store(), origin.discussion_id());
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].owner, Some(owner));
    let thread = thread_of(&plan, &origin);
    let last = thread.comments.last().unwrap();
    assert_eq!(last.id, proposal.comment_id);
    assert!(last.attachments.iter().any(|a| matches!(
        a,
        Attachment::MapPlanProposal { proposal_id, update: None } if *proposal_id == proposal.id
    )));
    assert_eq!(map_comments(&plan), before, "no Map thread gets the comment");
    assert!(open_row_exists(&plan.app(), origin.discussion_id()));
    let refused =
        record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("b", "Two", "y")], comment())
            .unwrap_err();
    assert_eq!(refused.code, "proposal_open");
}

// MPR-FR-TMTI, MPR-FR-KDTQ: an update from a turn of the whole-file
// discussion changes its open proposal and appends its comment there.
#[test]
fn an_update_appends_its_comment_to_the_discussion_of_the_proposal() {
    let plan = setup();
    let origin = open_file_discussion(&plan, false);
    record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("a", "One", "x")], comment())
        .unwrap();
    let comments = thread_of(&plan, &origin).comments.len();
    let updated = update_map_plan_proposal(
        &plan.app(),
        &origin,
        vec![MapPlanUpdate::Add(draft_row("b", "Two", "y"))],
        Some(comment()),
    )
    .unwrap();
    assert_eq!(updated.rows.len(), 2);
    let thread = thread_of(&plan, &origin);
    assert_eq!(thread.comments.len(), comments + 1);
    assert!(thread.comments.last().unwrap().attachments.iter().any(|a| matches!(
        a,
        Attachment::MapPlanProposal { update: Some(counts), .. } if counts.added == 1
    )));
}

// MPR-FR-KDTQ, MPR-FR-LAAK, MPR-FR-LDJH: the follow-up of a proposal of a file
// discussion appends its Synthesis comment to that discussion and dispatches
// the turn there, on the owner target of the discussion.
#[test]
fn the_follow_up_goes_to_the_discussion_of_the_proposal() {
    let plan = setup();
    let origin = open_file_discussion(&plan, false);
    let proposal =
        record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("a", "One", "x")], comment())
            .unwrap();
    let before = map_comments(&plan);
    accept_row(&plan.app(), origin.discussion_id(), &proposal.id, "a").unwrap();
    let turns = plan.turns.lock().unwrap().clone();
    assert_eq!(turns.len(), 1, "one follow-up turn");
    assert_eq!(turns[0].discussion_id, origin.discussion_id());
    assert_eq!(turns[0].target, origin.target);
    let thread = thread_of(&plan, &origin);
    let last = thread.comments.last().unwrap();
    assert_eq!(last.id, turns[0].trigger_comment_id);
    assert_eq!(last.author, Participant::synthesis());
    assert_eq!(map_comments(&plan), before);
}

// MPR-FR-RVRN: a record of an older build holds no owner and reads as a
// proposal of a Map thread.
#[test]
fn a_record_with_no_owner_belongs_to_a_map_thread() {
    let text = json!({
        "id": "p", "discussionId": "d", "commentId": "c",
        "createdAt": AT, "rows": []
    });
    let proposal: MapPlanProposal = serde_json::from_value(text).unwrap();
    assert_eq!(proposal.owner, None);
    let written = serde_json::to_value(&proposal).unwrap();
    assert!(written.get("owner").is_none(), "a Map record writes no owner");
}

fn stale_row(key: &str, number: u64) -> MapPlanRow {
    let mut row = modify_row(key, number, vec![change("title", json!(format!("T{number}")))]);
    if let RowFields::ModifyIssue(f) = &mut row.fields {
        f.base_updated_at = "2025-12-31T00:00:00Z".into();
    }
    row
}

// MPR-FR-KDTQ, MPR-FR-NWEZ, MPR-FR-LJRO: a row of a passage-thread proposal
// that failed with `changed_on_tracker` asks for a re-prepare: the Synthesis
// comment goes to the passage thread, and the turn is dispatched on its owner
// target.
#[test]
fn a_reprepare_request_goes_to_the_passage_thread_of_the_proposal() {
    let plan = setup();
    let origin = open_file_discussion(&plan, true);
    let proposal =
        record_map_plan_proposal(&plan.app(), &origin, vec![stale_row("m", 2)], comment()).unwrap();
    let before = map_comments(&plan);
    let decided = accept_row(&plan.app(), origin.discussion_id(), &proposal.id, "m").unwrap();
    assert_eq!(decided.rows[0].error.as_ref().unwrap().code, "changed_on_tracker");
    let turns = plan.turns.lock().unwrap().clone();
    assert_eq!(turns.len(), 1, "one re-prepare turn");
    assert_eq!(turns[0].discussion_id, origin.discussion_id());
    assert_eq!(turns[0].target, origin.target);
    let last = thread_of(&plan, &origin).comments.last().unwrap().clone();
    assert_eq!(last.id, turns[0].trigger_comment_id);
    assert_eq!(last.author, Participant::synthesis());
    assert!(decided.reprepare_requested_at.is_some());
    assert_eq!(map_comments(&plan), before, "no Map thread gets the comment");
}

// MPR-FR-RVRN, MPR-FR-KDTQ: a resolved file discussion refuses a recording,
// and nothing stays in the store.
#[test]
fn a_resolved_file_discussion_records_nothing() {
    let plan = setup();
    let origin = open_file_discussion(&plan, false);
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::log_ref_for(&origin.target, None),
        origin.discussion_id(),
        true,
        &human(),
        AT,
    )
    .unwrap();
    let refused =
        record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("a", "One", "x")], comment())
            .unwrap_err();
    assert_eq!(refused.code, crate::comments::ERR_DISCUSSION_RESOLVED);
    assert!(crate::map_plan::proposals_in(&plan.store(), origin.discussion_id()).is_empty());
}

// MPR-FR-SBYA: the open row is of one discussion. A Map thread with a waiting
// row does not stop a file discussion from recording, and the reverse.
#[test]
fn an_open_row_blocks_only_its_own_discussion() {
    let plan = setup();
    record_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![draft_row("a", "One", "x")],
        comment(),
    )
    .unwrap();
    let origin = open_file_discussion(&plan, false);
    assert!(!open_row_exists(&plan.app(), origin.discussion_id()));
    record_map_plan_proposal(&plan.app(), &origin, vec![draft_row("b", "Two", "y")], comment())
        .expect("the file discussion records");
    assert!(open_row_exists(&plan.app(), &plan.discussion));
    assert!(open_row_exists(&plan.app(), origin.discussion_id()));
}
