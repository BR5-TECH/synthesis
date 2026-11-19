//! MPR-FR-TMTI to MPR-FR-AESZ: the update of the open proposal.

use super::*;
use crate::comments::{Attachment, ProposalUpdateCounts, ERR_DISCUSSION_RESOLVED};

fn update(plan: &Plan, updates: Vec<MapPlanUpdate>) -> Result<MapPlanProposal, MapPlanError> {
    update_map_plan_proposal(&plan.app(), &plan.discussion, updates, Some(comment()))
}

fn comments_count(plan: &Plan) -> usize {
    crate::comments::find_map_discussion_in(&plan.store(), None).unwrap().comments.len()
}

fn keys(proposal: &MapPlanProposal) -> Vec<String> {
    proposal.rows.iter().map(|r| r.key.clone()).collect()
}

/// MPR-FR-TMTI, MPR-FR-EAAJ, MPR-FR-HQXX: one update changes, adds, and
/// withdraws rows of the open proposal. The proposal keeps its identity and the
/// order of its rows, and an added row goes at the end.
#[test]
fn an_update_changes_adds_and_withdraws_rows() {
    let plan = setup();
    let proposal = record(
        &plan,
        vec![
            create_row("a", new_issue("First")),
            create_row("b", new_issue("Second")),
            draft_row("c", "Draft", "Body"),
        ],
    );
    let updated = update(
        &plan,
        vec![
            MapPlanUpdate::Change(create_row("a", new_issue("First, shorter"))),
            MapPlanUpdate::Withdraw("b".into()),
            MapPlanUpdate::Add(draft_row("d", "Other", "Prompt")),
        ],
    )
    .unwrap();
    assert_eq!(updated.id, proposal.id);
    assert_eq!(updated.comment_id, proposal.comment_id);
    assert_eq!(updated.created_at, proposal.created_at);
    assert_eq!(keys(&updated), ["a", "c", "d"]);
    let RowFields::CreateIssue(fields) = &row_of(&updated, "a").fields else { panic!() };
    assert_eq!(fields.title, "First, shorter");
    assert!(updated.rows.iter().all(|r| r.state == RowState::Waiting));
    assert_eq!(listed(&plan), [updated]);
}

/// MPR-FR-EAAJ, CMS-FR-BSQB: the update appends one comment of the agent. Its
/// one attachment names the proposal and carries the counts of the update.
#[test]
fn an_update_appends_one_comment_with_its_counts() {
    let plan = setup();
    let proposal =
        record(&plan, vec![create_row("a", new_issue("A")), create_row("b", new_issue("B"))]);
    let before = comments_count(&plan);
    update(
        &plan,
        vec![MapPlanUpdate::Withdraw("b".into()), MapPlanUpdate::Add(draft_row("c", "C", "x"))],
    )
    .unwrap();
    let thread = crate::comments::find_map_discussion_in(&plan.store(), None).unwrap();
    assert_eq!(thread.comments.len(), before + 1);
    let last = thread.comments.last().unwrap();
    assert_ne!(last.id, proposal.comment_id);
    assert_eq!(last.author, agent());
    assert_eq!(
        last.attachments,
        [Attachment::MapPlanProposal {
            proposal_id: proposal.id.clone(),
            update: Some(ProposalUpdateCounts { changed: 0, added: 1, withdrawn: 1 }),
        }]
    );
}

/// MPR-FR-KSRG, MPR-FR-QUYY: a change of a rejected row makes it `waiting`
/// again, with no error and no result.
#[test]
fn a_changed_rejected_row_waits_again() {
    let plan = setup();
    let proposal =
        record(&plan, vec![create_row("a", new_issue("A")), create_row("b", new_issue("B"))]);
    reject(&plan, &proposal, "a").unwrap();
    let updated =
        update(&plan, vec![MapPlanUpdate::Change(create_row("a", new_issue("A again")))]).unwrap();
    let row = row_of(&updated, "a");
    assert_eq!(row.state, RowState::Waiting);
    assert_eq!(row.error, None);
    assert_eq!(row.result, None);
}

/// MPR-FR-KSRG: a rejected row can be withdrawn.
#[test]
fn a_rejected_row_can_be_withdrawn() {
    let plan = setup();
    let proposal =
        record(&plan, vec![create_row("a", new_issue("A")), create_row("b", new_issue("B"))]);
    reject(&plan, &proposal, "a").unwrap();
    let updated = update(&plan, vec![MapPlanUpdate::Withdraw("a".into())]).unwrap();
    assert_eq!(keys(&updated), ["b"]);
}

/// MPR-FR-KSRG: an accepted or a failed row can not be changed or withdrawn,
/// and the refusal changes nothing.
#[test]
fn a_decided_row_is_refused() {
    let plan = setup();
    let mut stored = record(
        &plan,
        vec![draft_row("a", "A", "x"), draft_row("b", "B", "y"), draft_row("c", "C", "z")],
    );
    accept(&plan, &stored, "a").unwrap();
    stored = listed(&plan).remove(0);
    stored.rows[1].state = RowState::Failed;
    store::write(&plan.store(), &stored).unwrap();
    let before = listed(&plan);
    let count = comments_count(&plan);
    for refused in [
        MapPlanUpdate::Change(draft_row("a", "A2", "x")),
        MapPlanUpdate::Withdraw("a".into()),
        MapPlanUpdate::Change(draft_row("b", "B2", "y")),
        MapPlanUpdate::Withdraw("b".into()),
    ] {
        assert_eq!(update(&plan, vec![refused]).unwrap_err().code, "row_decided");
    }
    assert_eq!(listed(&plan), before);
    assert_eq!(comments_count(&plan), count);
}

/// MPR-FR-KSRG: a key that names no row, and a change of another kind, are
/// refused.
#[test]
fn an_unknown_key_and_a_new_kind_are_refused() {
    let plan = setup();
    record(&plan, vec![create_row("a", new_issue("A"))]);
    let missing = update(&plan, vec![MapPlanUpdate::Change(create_row("z", new_issue("Z")))]);
    assert_eq!(missing.unwrap_err().code, "row_not_found");
    let missing = update(&plan, vec![MapPlanUpdate::Withdraw("z".into())]);
    assert_eq!(missing.unwrap_err().code, "row_not_found");
    let kind = update(&plan, vec![MapPlanUpdate::Change(draft_row("a", "A", "x"))]);
    assert_eq!(kind.unwrap_err().code, "kind_changed");
}

/// MPR-FR-NSEP: a discussion with no open row has nothing to update.
#[test]
fn no_open_proposal_is_refused() {
    let plan = setup();
    let none = update(&plan, vec![MapPlanUpdate::Add(draft_row("a", "A", "x"))]);
    assert_eq!(none.unwrap_err().code, "no_open_proposal");
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    reject(&plan, &proposal, "a").unwrap();
    let decided = update(&plan, vec![MapPlanUpdate::Change(draft_row("a", "A2", "x"))]);
    assert_eq!(decided.unwrap_err().code, "no_open_proposal");
    assert_eq!(state_of(&listed(&plan)[0], "a"), RowState::Rejected);
}

/// MPR-FR-NSEP, MPR-FR-SBYA: the update reaches the open proposal and never an
/// older one.
#[test]
fn the_update_reaches_the_open_proposal_only() {
    let plan = setup();
    let first = record(&plan, vec![draft_row("a", "A", "x")]);
    reject(&plan, &first, "a").unwrap();
    let second = record(&plan, vec![draft_row("b", "B", "y")]);
    let updated = update(&plan, vec![MapPlanUpdate::Add(draft_row("a", "Again", "z"))]).unwrap();
    assert_eq!(updated.id, second.id);
    let all = listed(&plan);
    assert_eq!(keys(&all[0]), ["a"]);
    assert_eq!(keys(&all[1]), ["b", "a"]);
}

/// MPR-FR-HQXX: an added key that a row holds, an update that leaves no row,
/// and a withdrawn parent of a remaining row are refused.
#[test]
fn the_rules_of_the_rows_left_hold() {
    let plan = setup();
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("p".into()));
    record(&plan, vec![create_row("p", new_issue("Parent")), create_row("c", child)]);
    let taken = update(&plan, vec![MapPlanUpdate::Add(draft_row("p", "P", "x"))]);
    assert_eq!(taken.unwrap_err().code, "key_taken");
    let readd = update(
        &plan,
        vec![
            MapPlanUpdate::Withdraw("p".into()),
            MapPlanUpdate::Add(create_row("p", new_issue("P"))),
        ],
    );
    assert_eq!(readd.unwrap_err().code, "key_taken");
    let orphan = update(&plan, vec![MapPlanUpdate::Withdraw("p".into())]);
    assert_eq!(orphan.unwrap_err().code, "parent_withdrawn");
    let empty = update(
        &plan,
        vec![MapPlanUpdate::Withdraw("c".into()), MapPlanUpdate::Withdraw("p".into())],
    );
    assert_eq!(empty.unwrap_err().code, "no_row_left");
    let both = update(
        &plan,
        vec![
            MapPlanUpdate::Withdraw("c".into()),
            MapPlanUpdate::Add(draft_row("d", "D", "x")),
            MapPlanUpdate::Withdraw("p".into()),
        ],
    )
    .unwrap();
    assert_eq!(keys(&both), ["d"]);
}

/// MPR-FR-EAAJ: an update is all or nothing. A refused update after a good one
/// writes nothing and appends no comment.
#[test]
fn an_update_is_all_or_nothing() {
    let plan = setup();
    record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
    let before = listed(&plan);
    let count = comments_count(&plan);
    let refused = update(
        &plan,
        vec![MapPlanUpdate::Withdraw("a".into()), MapPlanUpdate::Withdraw("z".into())],
    );
    assert_eq!(refused.unwrap_err().code, "row_not_found");
    assert_eq!(listed(&plan), before);
    assert_eq!(comments_count(&plan), count);
}

/// MPR-FR-EAAJ: where the comment can not be appended, the update is taken
/// back.
#[test]
fn a_comment_that_fails_takes_the_update_back() {
    let plan = setup();
    record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
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
    let refused = update(&plan, vec![MapPlanUpdate::Withdraw("a".into())]);
    assert_eq!(refused.unwrap_err().code, ERR_DISCUSSION_RESOLVED);
    assert_eq!(listed(&plan), before);
}

/// MPR-FR-YLKL, MPR-FR-HQXX: a decision after an update sees the new rows, and
/// an added child is blocked by its parent as any row is.
#[test]
fn a_decision_after_an_update_sees_the_new_rows() {
    let plan = setup();
    let proposal = record(&plan, vec![create_row("p", new_issue("Parent"))]);
    let mut child = new_issue("Child");
    child.parent = Some(ParentRef::Row("p".into()));
    let updated = update(&plan, vec![MapPlanUpdate::Add(create_row("c", child))]).unwrap();
    assert_eq!(
        row_of(&updated, "c").blocked_by,
        Some(RowBlock { row_key: "p".into(), reason: RowState::Waiting })
    );
    let refused = accept(&plan, &proposal, "c").unwrap_err();
    assert_eq!(refused.code, "parent_not_accepted");
    let rejected = reject(&plan, &proposal, "c").unwrap();
    assert_eq!(state_of(&rejected, "c"), RowState::Rejected);
}

/// MPR-FR-KSRG: a changed or added row comes in `waiting`, with no error and
/// no result, whatever state the caller gave it.
#[test]
fn changed_and_added_rows_come_in_waiting() {
    let stale = |key: &str| {
        let mut row = draft_row(key, "R", "x");
        row.state = RowState::Accepted;
        row.error = Some(RowError { code: "x".into(), message: "y".into() });
        row.result = Some(RowResult { draft_id: Some("d".into()), ..RowResult::default() });
        row
    };
    let mut rejected = draft_row("a", "A", "x");
    rejected.state = RowState::Rejected;
    rejected.error = Some(RowError { code: "x".into(), message: "y".into() });
    let proposal = MapPlanProposal {
        id: "p".into(),
        discussion_id: "d".into(),
        comment_id: "c".into(),
        created_at: AT.into(),
        reprepare_requested_at: None,
        follow_up_requested_at: None,
        rows: vec![rejected, draft_row("b", "B", "y")],
    };
    let (after, counts) = apply_updates(
        &proposal,
        vec![MapPlanUpdate::Change(stale("a")), MapPlanUpdate::Add(stale("c"))],
    )
    .unwrap();
    for key in ["a", "c"] {
        let row = row_of(&after, key);
        assert_eq!((row.state, &row.error, &row.result), (RowState::Waiting, &None, &None));
    }
    assert_eq!(counts, ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 0 });
}

/// Wait until each event emitted before this call is seen. Tauri delivers a
/// queued emit in order on the next emit.
fn fenced(plan: &Plan) {
    use tauri::{Emitter, Listener};
    const FENCE: &str = "updating-test-fence";
    let seen = Arc::new(Mutex::new(false));
    let sink = Arc::clone(&seen);
    plan.app().listen(FENCE, move |_| *sink.lock().unwrap() = true);
    plan.app().emit(FENCE, ()).unwrap();
    crate::delivery::github::tests::wait_until("the fence event", || *seen.lock().unwrap());
}

/// MPR-FR-AESZ: a successful update emits `"map plan proposals changed"` and
/// the change of the discussion, with the discussion id. A refused update
/// emits neither.
#[test]
fn an_update_emits_its_events_and_a_refusal_none() {
    use crate::delivery::github::tests::fake::listen;
    let plan = setup();
    record(&plan, vec![draft_row("a", "A", "x"), draft_row("b", "B", "y")]);
    fenced(&plan);
    let proposals = listen(&plan.app(), MAP_PLAN_PROPOSALS_CHANGED);
    let discussions = listen(&plan.app(), crate::comments::DISCUSSION_CHANGED);
    update(&plan, vec![MapPlanUpdate::Withdraw("z".into())]).unwrap_err();
    fenced(&plan);
    assert!(proposals.lock().unwrap().is_empty());
    assert!(discussions.lock().unwrap().is_empty());
    update(&plan, vec![MapPlanUpdate::Withdraw("a".into())]).unwrap();
    fenced(&plan);
    let seen = proposals.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let payload: Value = serde_json::from_str(&seen[0]).unwrap();
    assert_eq!(payload["discussionId"], json!(plan.discussion));
    assert_eq!(discussions.lock().unwrap().len(), 1);
}
