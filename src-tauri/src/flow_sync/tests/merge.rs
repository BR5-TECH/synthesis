//! FTS-FR-PNMA, FTS-FR-HLVJ, FTS-FR-AIJM, FTS-FR-VYOV, FTS-FR-VRRJ,
//! FTS-FR-CYJJ: the field-by-field merge.

use super::*;
use crate::flow_validation::model::EdgeType;

/// FTS-FR-HLVJ / FTS-FR-VFNM: a plan in sync with its tracker makes no
/// tracker write and leaves the Flow as it was, so nothing is written.
#[test]
fn a_plan_in_sync_writes_nothing_on_either_side() {
    let (tracker, d) = plan();
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!out.doc_changed);
    assert_eq!(out.result, SyncResult::default());
}

/// FTS-FR-AIJM: a name only the Flow changed is written to the tracker, and
/// the base takes it.
#[test]
fn a_name_only_the_flow_changed_is_written_to_the_tracker() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["update_issue 3 title"]);
    assert_eq!(tracker.issue(3).title, "Tokenize cards");
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize cards");
    assert_eq!((out.result.updated, out.result.changed), (1, 0));
}

/// FTS-FR-AIJM: a name only the tracker changed is written to the Flow, and
/// nothing is written to the tracker.
#[test]
fn a_name_only_the_tracker_changed_is_written_to_the_flow() {
    let (tracker, d) = plan();
    tracker.edit(3, |i| i.title = "Tokenize PANs".into());
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(find(&out.doc, "t1").name, "Tokenize PANs");
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize PANs");
    assert_eq!((out.result.updated, out.result.changed), (0, 1));
    assert!(out.doc_changed);
}

/// FTS-FR-AIJM: a field both sides changed to the same value only updates
/// the base.
#[test]
fn a_field_both_changed_alike_only_updates_the_base() {
    let (tracker, mut d) = plan();
    tracker.edit(3, |i| i.title = "Tokenize all".into());
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize all".into();
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert!(out.result.conflicts.is_empty());
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize all");
    assert_eq!(out.result.changed, 0);
}

/// FTS-FR-VRRJ: descriptions compare with LF line endings and no trailing
/// whitespace, so CRLF and a trailing newline on GitHub are no change.
#[test]
fn descriptions_compare_with_normalized_line_endings() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().description = Some("a\nb".into());
    d.sync.as_mut().unwrap().links[3].base.description = "a\nb".into();
    tracker.edit(3, |i| i.body = "a\r\nb\r\n  \n".into());
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(find(&out.doc, "t1").description.as_deref(), Some("a\nb"));
    assert_eq!(out.result.changed, 0);
}

/// FTS-FR-VRRJ / FTS-FR-AIJM: a real change of the tracker body comes to the
/// Flow normalized, and an empty one makes the description absent.
#[test]
fn a_tracker_description_comes_to_the_flow_normalized() {
    let (tracker, d) = plan();
    tracker.edit(3, |i| i.body = "Use the vault.\r\n".into());
    let out = sync(&tracker, d);
    assert_eq!(find(&out.doc, "t1").description.as_deref(), Some("Use the vault."));
    assert_eq!(link_of(&out.doc, "t1").base.description, "Use the vault.\r\n");

    tracker.edit(3, |i| i.body = String::new());
    let again = sync(&tracker, out.doc);
    assert_eq!(find(&again.doc, "t1").description, None);
}

/// FTS-FR-AIJM: a description only the Flow changed is written as the body.
#[test]
fn a_description_only_the_flow_changed_is_written_as_the_body() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "f1").unwrap().description = Some("3DS too".into());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["update_issue 2 body"]);
    assert_eq!(tracker.issue(2).body, "3DS too");
    assert_eq!(link_of(&out.doc, "f1").base.description, "3DS too");
}

/// FTS-FR-CYJJ: a closed issue is stored as `closed` in the base, and no state
/// is ever written.
#[test]
fn the_tracker_state_is_stored_in_the_base_and_never_written() {
    let (tracker, d) = plan();
    tracker.edit(3, |i| i.state = "closed".into());
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(link_of(&out.doc, "t1").base.state, "closed");
    assert!(out.result.conflicts.is_empty());
    assert_eq!(out.result.changed, 0);
}

/// FTS-FR-PNMA: a milestone link syncs its name, its description, and its
/// due date, each in its own direction.
#[test]
fn a_milestone_link_syncs_name_description_and_due() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "m1").unwrap().due = Some("2027-03-31".into());
    tracker.state.borrow_mut().milestones.get_mut(&5).unwrap().title = "Beta 1".into();
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["update_milestone 5 due"]);
    assert_eq!(tracker.state.borrow().milestones[&5].due.as_deref(), Some("2027-03-31"));
    assert_eq!(find(&out.doc, "m1").name, "Beta 1");
    let base = &link_of(&out.doc, "m1").base;
    assert_eq!(base.due, Some(Some("2027-03-31".into())));
    assert_eq!(base.name, "Beta 1");
    assert_valid(&out.doc);
}

/// FTS-FR-PNMA: a due date only the tracker changed comes to the Flow.
#[test]
fn a_due_date_only_the_tracker_changed_comes_to_the_flow() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().milestones.get_mut(&5).unwrap().due = Some("2027-06-30".into());
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(find(&out.doc, "m1").due.as_deref(), Some("2027-06-30"));
}

/// FTS-FR-HLVJ: an ancestor with no link is passed over: the parent is the
/// nearest linked ancestor that is not a milestone, and the milestone is the
/// nearest linked milestone.
#[test]
fn an_unlinked_ancestor_is_passed_over() {
    let tracker = FakeTracker::new()
        .with_issue(child(1, "Card payments", "Epic", None, Some(5)))
        .with_issue(child(3, "Tokenize", "Task", Some(1), Some(5)))
        .with_milestone(milestone(5, "Beta"));
    let mut feature = item("f1", "feature", "Card capture", Some("ep1"));
    feature["localOnly"] = json!(true);
    let d = doc(
        vec![
            item("m1", "milestone", "Beta", None),
            item("ep1", "epic", "Card payments", Some("m1")),
            feature,
            item("t1", "task", "Tokenize", Some("f1")),
        ],
        vec![],
        vec![
            link("m1", "milestone", 5, milestone_base("Beta", None)),
            link("ep1", "issue", 1, issue_base("Card payments", Some("Epic"), None, Some(5), &[])),
            link("t1", "issue", 3, issue_base("Tokenize", Some("Task"), Some(1), Some(5), &[])),
        ],
    );
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!out.doc_changed);
}

/// FTS-FR-HLVJ: an epic or a feature directly in a milestone has no parent
/// issue, since a milestone is never a parent. A feature the Flow moves from
/// an epic into its milestone loses its parent on the tracker.
#[test]
fn an_item_directly_in_a_milestone_has_no_parent_issue() {
    let (tracker, mut d) = plan();
    let f1 = d.items.iter_mut().find(|i| i.id == "f1").unwrap();
    f1.parent_id = Some("m1".into());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["set_parent 2 1->none"]);
    assert_eq!(tracker.issue(2).parent, None);
    assert_eq!(tracker.issue(2).milestone, Some(5));
    assert_eq!(link_of(&out.doc, "f1").base.parent, Some(None));
    assert_eq!(link_of(&out.doc, "ep1").base.parent, Some(None));
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-AIJM / FTS-FR-LMBE: a changed Type setting is a Flow change of the
/// `type` field, written to the tracker in the repository's spelling.
#[test]
fn a_changed_type_setting_is_written_to_the_tracker() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().types.push("Chore".into());
    let settings = FlowTrackerSettings { task_type: "chore".into(), ..settings() };
    let out = super::super::engine::run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap();
    assert_eq!(tracker.writes(), vec!["update_issue 3 type"]);
    assert_eq!(tracker.issue(3).issue_type.as_deref(), Some("Chore"));
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Chore".into())));
    // The comparison ignores case, so the next sync finds nothing to do.
    tracker.clear_calls();
    let again = super::super::engine::run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: out.doc, settings: &settings, resolutions: &[] },
    )
    .unwrap();
    assert!(tracker.writes().is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-VYOV: a `blocks` edge only the Flow added becomes a blocked-by
/// relation, and the base takes it.
#[test]
fn a_blocks_edge_the_flow_added_is_added_on_the_tracker() {
    let (tracker, mut d) = plan_with_two_tasks();
    d.edges.push(serde_json::from_value(blocks("e1", "t2", "t1")).unwrap());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["add_blocked_by 3 4"]);
    assert_eq!(tracker.issue(3).blocked_by.iter().map(|k| k.number).collect::<Vec<_>>(), vec![4]);
    let base = link_of(&out.doc, "t1").base.blocked_by.clone().unwrap();
    assert_eq!(base.iter().map(|r| r.number).collect::<Vec<_>>(), vec![4]);
}

/// FTS-FR-VYOV: a relation only the tracker added becomes a `blocks` edge.
#[test]
fn a_relation_the_tracker_added_becomes_an_edge() {
    let (tracker, d) = plan_with_two_tasks();
    tracker.edit(3, |i| i.blocked_by.push(key(4)));
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    let edge = out.doc.edges.iter().find(|e| e.to == "t1").expect("an edge");
    assert_eq!((edge.from.as_str(), edge.edge_type), ("t2", EdgeType::Blocks));
    assert_eq!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().len(), 1);
    assert_eq!(out.result.changed, 1);
    assert_valid(&out.doc);
}

/// FTS-FR-VYOV: an edge the Flow removed is removed on the tracker, and the
/// base drops the entry once the removal is made.
#[test]
fn an_edge_the_flow_removed_is_removed_on_the_tracker() {
    let (tracker, mut d) = plan_with_two_tasks();
    tracker.edit(3, |i| i.blocked_by.push(key(4)));
    let links = &mut d.sync.as_mut().unwrap().links;
    let t1 = links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.base.blocked_by = Some(vec![serde_json::from_value(ref_json("issue", 4)).unwrap()]);
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["remove_blocked_by 3 4"]);
    assert!(tracker.issue(3).blocked_by.is_empty());
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());
}

/// FTS-FR-VYOV: a relation the tracker removed removes the edge from the
/// Flow, and never yields a conflict.
#[test]
fn a_relation_the_tracker_removed_removes_the_edge() {
    let (tracker, mut d) = plan_with_two_tasks();
    d.edges.push(serde_json::from_value(blocks("e1", "t2", "t1")).unwrap());
    let links = &mut d.sync.as_mut().unwrap().links;
    let t1 = links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.base.blocked_by = Some(vec![serde_json::from_value(ref_json("issue", 4)).unwrap()]);
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert!(out.doc.edges.is_empty());
    assert!(out.result.conflicts.is_empty());
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());
}

/// FTS-FR-VYOV: a relation to an issue the Flow does not hold, and one the
/// plan cannot hold as an edge, stay on the tracker and out of the base, so
/// the next sync removes nothing.
#[test]
fn a_relation_the_plan_cannot_hold_stays_on_the_tracker() {
    let (tracker, d) = plan();
    let tracker = tracker.with_issue(issue(9, "Elsewhere", Some("Task")));
    // Issue 9 is in no link; issue 2 is the parent of t1, an edge within its
    // lineage (FGV-FR-KBLT).
    tracker.edit(3, |i| i.blocked_by = vec![key(9), key(2)]);
    let out = sync(&tracker, d);
    assert!(out.doc.edges.is_empty());
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty());
    assert_eq!(tracker.issue(3).blocked_by.len(), 2);
    assert!(!again.doc_changed);
}

/// FTS-FR-VRRJ: a change inside the text, at its start, or in a name counts;
/// only line endings and whitespace at the end of a description do not.
#[test]
fn only_line_endings_and_trailing_whitespace_are_ignored() {
    for (tracker_body, is_change) in
        [("a\rb", false), ("a\r\nb\n\n", false), (" a\nb", true), ("a  \nb", true)]
    {
        let (tracker, mut d) = plan();
        d.items.iter_mut().find(|i| i.id == "t1").unwrap().description = Some("a\nb".into());
        d.sync.as_mut().unwrap().links[3].base.description = "a\nb".into();
        tracker.edit(3, |i| i.body = tracker_body.into());
        let out = sync(&tracker, d);
        assert_eq!(out.result.changed == 1, is_change, "{tracker_body:?}");
    }
    let (tracker, d) = plan();
    tracker.edit(3, |i| i.title = "Tokenize ".into());
    assert_eq!(find(&sync(&tracker, d).doc, "t1").name, "Tokenize ");
}

/// FTS-FR-VYOV: a relation the plan cannot hold as an edge, because a
/// `relates` edge already joins the two items or because it would make a
/// dependency cycle, adds no edge and no base entry.
#[test]
fn a_relation_that_breaks_an_edge_rule_adds_nothing() {
    // A `relates` edge already joins t1 and t2.
    let (tracker, mut d) = plan_with_two_tasks();
    d.edges.push(
        serde_json::from_value(json!({ "id": "e1", "from": "t2", "to": "t1", "type": "relates" }))
            .unwrap(),
    );
    tracker.edit(3, |i| i.blocked_by.push(key(4)));
    let out = sync(&tracker, d);
    assert_eq!(out.doc.edges.len(), 1);
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());

    // t1 blocks t2 already; t2 blocking t1 would be a cycle.
    let (tracker, mut d) = plan_with_two_tasks();
    d.edges.push(serde_json::from_value(blocks("e1", "t1", "t2")).unwrap());
    let links = &mut d.sync.as_mut().unwrap().links;
    let t2 = links.iter_mut().find(|l| l.item_id == "t2").unwrap();
    t2.base.blocked_by = Some(vec![serde_json::from_value(ref_json("issue", 3)).unwrap()]);
    tracker.edit(4, |i| i.blocked_by.push(key(3)));
    tracker.edit(3, |i| i.blocked_by.push(key(4)));
    let out = sync(&tracker, d);
    assert_eq!(out.doc.edges.len(), 1);
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
}

/// FTS-FR-VEEG: values that differ only in line endings give the same
/// conflict id, and another field gives another id.
#[test]
fn the_conflict_id_reads_the_normalized_values() {
    let run_with = |tracker_body: &str| {
        let (tracker, mut d) = plan();
        d.items.iter_mut().find(|i| i.id == "t1").unwrap().description = Some("flow".into());
        tracker.edit(3, |i| i.body = tracker_body.into());
        sync(&tracker, d).result.conflicts[0].clone()
    };
    let a = run_with("tracker\n");
    let b = run_with("tracker\r\n");
    assert_eq!(a.field, Some("description"));
    assert_eq!(a.id, b.id);
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "flow".into();
    tracker.edit(3, |i| i.title = "tracker".into());
    assert_ne!(sync(&tracker, d).result.conflicts[0].id, a.id);
}

/// FTS-FR-VEEG / FTS-FR-PBQC: a due date both sides changed differently is a
/// field conflict of `due`.
#[test]
fn a_due_date_both_changed_is_a_conflict() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "m1").unwrap().due = Some("2027-03-31".into());
    tracker.state.borrow_mut().milestones.get_mut(&5).unwrap().due = Some("2027-06-30".into());
    let out = sync(&tracker, d.clone());
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field, c.item_id.as_str()), (ConflictKind::Field, Some("due"), "m1"));
    assert_eq!(
        (c.flow.clone(), c.tracker.clone(), c.base.clone()),
        (json!("2027-03-31"), json!("2027-06-30"), Value::Null)
    );
    assert_eq!(c.url.as_deref(), Some(milestone_url(5).as_str()));
    let out = resolve(&tracker, d, &[choose(c, Choice::KeepFlow)]);
    assert_eq!(tracker.state.borrow().milestones[&5].due.as_deref(), Some("2027-03-31"));
    assert!(out.result.conflicts.is_empty());
}

/// FTS-FR-VYOV / FTS-FR-WANV: a blocker in another repository is ignored: no
/// edge, no base entry, and no write, in this sync and the next.
#[test]
fn a_blocker_of_another_repository_is_ignored() {
    let (tracker, d) = plan();
    let mut upstream = key(4);
    upstream.repo = "upstream".into();
    tracker.edit(3, |i| i.blocked_by.push(upstream));
    let out = sync(&tracker, d);
    assert!(out.doc.edges.is_empty());
    assert!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().is_empty());
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert!(!out.doc_changed);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty(), "{:?}", again.result.conflicts);
    assert_eq!(tracker.issue(3).blocked_by.len(), 1);
    assert!(!again.doc_changed);
}

/// FTS-FR-VFNM: an `update_issue` that fails during the merge
/// stops the writes after it and keeps the base of its field, so the next
/// sync writes it again without a conflict.
#[test]
fn a_failed_update_during_the_merge_keeps_the_base() {
    let (tracker, mut d) = plan_with_two_tasks();
    let tracker = tracker.failing("update_issue", 0);
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    d.items.iter_mut().find(|i| i.id == "t2").unwrap().name = "Card form v2".into();
    let out = sync(&tracker, d);
    assert_eq!(out.write_error.as_deref(), Some(ERR_GITHUB_UNREACHABLE));
    assert_eq!(tracker.writes(), vec!["update_issue 3 title FAILED"]);
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize");
    assert_eq!(link_of(&out.doc, "t2").base.name, "Card form");
    assert_eq!(out.result.updated, 0);

    tracker.state.borrow_mut().fail = None;
    tracker.clear_calls();
    let next = sync(&tracker, out.doc);
    assert!(next.write_error.is_none());
    assert!(next.result.conflicts.is_empty());
    let mut writes = tracker.writes();
    writes.sort();
    assert_eq!(writes, vec!["update_issue 3 title", "update_issue 4 title"]);
    assert_eq!(tracker.issue(3).title, "Tokenize cards");
}
