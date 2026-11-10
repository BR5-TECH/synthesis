//! FTS-FR-AONJ, FTS-FR-BMWI, FTS-FR-VXMN, FTS-FR-VYOV, FTS-FR-EGJL: what a
//! sync leaves alone while a removal waits for the author, and the order of
//! the work around a removal.

use super::*;
use crate::flow_validation::model::SyncLink;

fn blocked_t1(d: &mut FlowDocument) {
    d.edges.push(serde_json::from_value(blocks("e1", "t2", "t1")).unwrap());
    let links = &mut d.sync.as_mut().unwrap().links;
    let t1 = links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.base.blocked_by = Some(vec![serde_json::from_value(ref_json("issue", 4)).unwrap()]);
}

/// FTS-FR-EGJL / FTS-FR-BMWI: a pulled item never takes the id that the link
/// of an item gone from the Flow still names, so the plan stays valid.
#[test]
fn a_pulled_item_does_not_take_the_id_of_a_removed_item() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    let tracker = tracker.with_issue(child(9, "Card form", "Task", Some(2), Some(5)));
    let out = sync(&tracker, d);
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").expect("pulled");
    assert_ne!(pulled.id, "t1");
    assert_eq!(out.result.conflicts[0].kind, ConflictKind::RemovedInFlow);
    assert_valid(&out.doc);
}

/// FTS-FR-BMWI / FTS-FR-EGJL: an issue whose link `unlink` removed is not
/// pulled back by the same call.
#[test]
fn an_unlinked_issue_is_not_pulled_back_in_the_same_call() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Unlink)]);
    assert_eq!(out.result.added, 0);
    assert!(out.doc.items.iter().all(|i| i.name != "Tokenize"));
}

/// FTS-FR-AONJ / FTS-FR-VYOV: a blocker removed in the Flow, whose removal
/// waits for the author, keeps its relation on the tracker and its entry in
/// the base, so a restore brings the item back with its relation.
#[test]
fn a_blocker_removed_in_the_flow_keeps_its_relation_while_it_waits() {
    let (tracker, mut d) = plan_with_two_tasks();
    blocked_t1(&mut d);
    tracker.edit(3, |i| i.blocked_by.push(key(4)));
    d.items.retain(|i| i.id != "t2");
    d.edges.clear();
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().len(), 1);
    assert_eq!(tracker.issue(3).blocked_by.len(), 1);
}

/// FTS-FR-AONJ / FTS-FR-VYOV: a blocker gone from the tracker, whose removal
/// waits for the author, keeps its edge in the Flow.
#[test]
fn a_blocker_gone_from_the_tracker_keeps_its_edge_while_it_waits() {
    let (tracker, mut d) = plan_with_two_tasks();
    blocked_t1(&mut d);
    tracker.state.borrow_mut().issues.remove(&4);
    let out = sync(&tracker, d);
    assert_eq!(out.doc.edges.len(), 1);
    assert_eq!(link_of(&out.doc, "t1").base.blocked_by.as_ref().unwrap().len(), 1);
    assert_eq!(out.result.conflicts.len(), 1);
}

/// FTS-FR-AONJ / FTS-FR-VXMN: the members of an issue gone from the tracker
/// stay where they are while its removal waits, though the tracker now names
/// no parent for them.
#[test]
fn members_of_a_parent_gone_from_the_tracker_do_not_move() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().issues.remove(&2);
    tracker.edit(3, |i| i.parent = None);
    let out = sync(&tracker, d);
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f1"));
    assert_eq!(out.result.conflicts.len(), 1);
    assert_eq!(out.result.conflicts[0].item_id, "f1");
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
}

/// FTS-FR-HDRR / FTS-FR-HLVJ: an item the Flow moved into a new container
/// gets the new container's issue as its parent in the same sync.
#[test]
fn an_item_moved_into_a_new_container_names_its_new_issue() {
    let (tracker, mut d) = plan();
    d.items.push(serde_json::from_value(item("f9", "feature", "Checkout", Some("m1"))).unwrap());
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f9".into());
    let out = sync(&tracker, d);
    // The new feature sits directly in the milestone, so it has no parent.
    assert_eq!(
        tracker.writes(),
        vec!["create_issue 100 type=Feature milestone=5", "set_parent 3 2->100"]
    );
    assert_eq!(tracker.issue(3).parent.map(|k| k.number), Some(100));
    assert_eq!(link_of(&out.doc, "t1").base.parent.clone().flatten().map(|p| p.number), Some(100));
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!again.doc_changed);
}

/// FTS-FR-BMWI: a restore of an item and of its container comes out the same
/// whatever the order of their links.
#[test]
fn restores_of_an_item_and_its_container_do_not_depend_on_link_order() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1" && i.id != "f1");
    let links = &mut d.sync.as_mut().unwrap().links;
    let t1: SyncLink = links.remove(3);
    links.insert(2, t1);
    let first = sync(&tracker, d.clone());
    let picks: Vec<ConflictResolution> =
        first.result.conflicts.iter().map(|c| choose(c, Choice::Restore)).collect();
    assert_eq!(picks.len(), 2);
    let out = resolve(&tracker, d, &picks);
    assert_eq!(find(&out.doc, "f1").parent_id.as_deref(), Some("ep1"));
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f1"));
    assert!(out.result.conflicts.is_empty());
    assert_eq!(out.result.added, 2);
    assert_valid(&out.doc);
}

/// FTS-FR-SZGF: `unlink` of a link takes every conflict of that link with
/// it, so no conflict names a link that is gone.
#[test]
fn unlink_takes_every_conflict_of_the_link_with_it() {
    let (tracker, mut d) = plan();
    let tracker = tracker.with_issue(child(6, "Wallets", "Feature", Some(1), Some(5)));
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    tracker.edit(3, |i| {
        i.title = "Tokenize PANs".into();
        i.parent = Some(key(1));
    });
    let first = sync(&tracker, d.clone());
    assert_eq!(first.result.conflicts.len(), 2);
    let invalid =
        first.result.conflicts.iter().find(|c| c.kind == ConflictKind::InvalidOnTracker).unwrap();
    let out = resolve(&tracker, d, &[choose(invalid, Choice::Unlink)]);
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert!(!has_link(&out.doc, "t1"));
}

/// FTS-FR-CJVS: a sub-issue of a linked task has no kind that fits, and is
/// reported as skipped.
#[test]
fn a_sub_issue_of_a_linked_leaf_is_skipped() {
    let (tracker, d) = plan();
    let tracker = tracker.with_issue(child(9, "Sub-task", "Task", Some(3), Some(5)));
    let out = sync(&tracker, d);
    assert_eq!(out.result.skipped.len(), 1);
    assert_eq!(
        (out.result.skipped[0].number, out.result.skipped[0].reason),
        (Some(9), SKIP_NO_KIND_FITS)
    );
    assert_eq!(out.result.skipped[0].url.as_deref(), Some(url_of(9).as_str()));
}

/// FTS-FR-HDRR / FTS-FR-VXMN: an issue created under a container whose issue
/// is gone from the tracker gets no parent until the author decides.
#[test]
fn a_creation_under_a_missing_parent_sets_no_parent() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().issues.remove(&2);
    d.items.push(serde_json::from_value(item("t9", "task", "Card form", Some("f1"))).unwrap());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["create_issue 100 type=Task milestone=5"]);
    assert_eq!(link_of(&out.doc, "t9").base.parent, Some(None));
}
