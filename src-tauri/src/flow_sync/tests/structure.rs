//! FTS-FR-AIJM, FTS-FR-SZGF, FTS-FR-PBQC: `parent`, `milestone`, and `type`,
//! which move an item or change its kind.

use super::*;
use crate::flow_validation::model::{Kind, SyncLink};

/// [`plan`] with feature `f2` (issue 6) directly in `m1`, so with no parent
/// issue, and milestone `m2` (milestone 7) at the top level, all in sync.
fn wide_plan() -> (FakeTracker, FlowDocument) {
    let (tracker, mut d) = plan();
    let tracker = tracker
        .with_issue(child(6, "Wallets", "Feature", None, Some(5)))
        .with_milestone(milestone(7, "GA"));
    let f2: Item = serde_json::from_value(item("f2", "feature", "Wallets", Some("m1"))).unwrap();
    let m2: Item = serde_json::from_value(item("m2", "milestone", "GA", None)).unwrap();
    d.items.push(f2);
    d.items.push(m2);
    let links = &mut d.sync.as_mut().unwrap().links;
    let f2_link: SyncLink = serde_json::from_value(link(
        "f2",
        "issue",
        6,
        issue_base("Wallets", Some("Feature"), None, Some(5), &[]),
    ))
    .unwrap();
    let m2_link: SyncLink =
        serde_json::from_value(link("m2", "milestone", 7, milestone_base("GA", None))).unwrap();
    links.push(f2_link);
    links.push(m2_link);
    (tracker, d)
}

/// FTS-FR-AIJM: a parent only the tracker changed moves the item in the Flow.
#[test]
fn a_parent_the_tracker_changed_moves_the_item() {
    let (tracker, d) = wide_plan();
    tracker.edit(3, |i| i.parent = Some(key(6)));
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f2"));
    let base = &link_of(&out.doc, "t1").base;
    assert_eq!(base.parent.clone().flatten().map(|p| p.number), Some(6));
    assert_eq!(out.result.changed, 1);
    assert!(out.result.conflicts.is_empty());
    assert_valid(&out.doc);
}

/// FTS-FR-AIJM / FTS-FR-HLVJ: an item the Flow moved gets its new parent on
/// the tracker.
#[test]
fn a_parent_the_flow_changed_is_set_on_the_tracker() {
    let (tracker, mut d) = wide_plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f2".into());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["set_parent 3 2->6"]);
    assert_eq!(tracker.issue(3).parent.map(|k| k.number), Some(6));
    assert_eq!(link_of(&out.doc, "t1").base.parent.clone().flatten().map(|p| p.number), Some(6));
}

/// FTS-FR-AIJM / FTS-FR-HLVJ: a feature the Flow moved to another milestone
/// gets that milestone on the tracker.
#[test]
fn a_milestone_the_flow_changed_is_set_on_the_tracker() {
    let (tracker, mut d) = wide_plan();
    d.items.iter_mut().find(|i| i.id == "f2").unwrap().parent_id = Some("m2".into());
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["set_milestone 6 5->7"]);
    assert_eq!(tracker.issue(6).milestone, Some(7));
    assert_eq!(link_of(&out.doc, "f2").base.milestone, Some(Some(7)));
}

/// FTS-FR-AIJM / FTS-FR-EGJL: a milestone the tracker changed moves an item
/// with no parent issue into the milestone item, and the next sync finds
/// both sides in sync.
#[test]
fn a_milestone_the_tracker_changed_moves_the_item() {
    let (tracker, d) = wide_plan();
    tracker.edit(6, |i| i.milestone = Some(7));
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(find(&out.doc, "f2").parent_id.as_deref(), Some("m2"));
    assert_eq!(link_of(&out.doc, "f2").base.milestone, Some(Some(7)));
    assert_eq!(out.result.changed, 1);
    assert_valid(&out.doc);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-EGJL / FTS-FR-SZGF / FTS-FR-AONJ: a milestone is never under a
/// parent item, so a tracker milestone that differs from the one the
/// parent's item gives is `invalid_on_tracker` with the choices `keep_flow`
/// and `unlink`, and the item stays in its parent. The next sync reports the
/// same conflict with the same id.
#[test]
fn a_milestone_the_parent_does_not_give_is_invalid_on_tracker() {
    let (tracker, d) = wide_plan();
    tracker.edit(2, |i| i.milestone = Some(7));
    let out = sync(&tracker, d);
    assert_eq!(out.result.conflicts.len(), 1, "{:?}", out.result.conflicts);
    let c = &out.result.conflicts[0];
    assert_eq!(
        (c.kind, c.field, c.item_id.as_str()),
        (ConflictKind::InvalidOnTracker, Some("milestone"), "f1")
    );
    assert_eq!(c.choices, vec![Choice::KeepFlow, Choice::Unlink]);
    assert_eq!((c.flow.clone(), c.tracker.clone()), (json!(5), json!(7)));
    assert_eq!(find(&out.doc, "f1").parent_id.as_deref(), Some("ep1"));
    assert_eq!(link_of(&out.doc, "f1").base.milestone, Some(Some(5)));
    assert!(tracker.writes().is_empty());
    tracker.clear_calls();
    let again = sync(&tracker, out.doc.clone());
    assert_eq!(again.result.conflicts, out.result.conflicts);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(find(&again.doc, "f1").parent_id.as_deref(), Some("ep1"));
}

/// FTS-FR-AIJM / FTS-FR-EGJL: a parent and a milestone that the tracker
/// changes together move the item once, into the place both give.
#[test]
fn a_parent_and_a_milestone_the_tracker_changed_together_move_the_item() {
    let (tracker, mut d) = wide_plan();
    let tracker = tracker.with_issue(child(8, "Payouts", "Feature", None, Some(7)));
    d.items.push(serde_json::from_value(item("f3", "feature", "Payouts", Some("m2"))).unwrap());
    let f3_link: SyncLink = serde_json::from_value(link(
        "f3",
        "issue",
        8,
        issue_base("Payouts", Some("Feature"), None, Some(7), &[]),
    ))
    .unwrap();
    d.sync.as_mut().unwrap().links.push(f3_link);
    tracker.edit(3, |i| {
        i.parent = Some(key(8));
        i.milestone = Some(7);
    });
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f3"));
    let base = &link_of(&out.doc, "t1").base;
    assert_eq!(base.parent.clone().flatten().map(|p| p.number), Some(8));
    assert_eq!(base.milestone, Some(Some(7)));
    assert_eq!(out.result.changed, 1);
    assert_valid(&out.doc);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-SZGF: a tracker parent the hierarchy refuses (a task in a
/// milestone) is `invalid_on_tracker`, and neither side changes.
#[test]
fn a_parent_the_hierarchy_refuses_is_invalid_on_tracker() {
    let (tracker, d) = wide_plan();
    tracker.edit(3, |i| i.parent = Some(key(1)));
    let out = sync(&tracker, d.clone());
    assert!(tracker.writes().is_empty());
    assert_eq!(out.result.conflicts.len(), 1);
    let c = &out.result.conflicts[0];
    assert_eq!(
        (c.kind, c.field, c.item_id.as_str()),
        (ConflictKind::InvalidOnTracker, Some("parent"), "t1")
    );
    assert_eq!(c.choices, vec![Choice::KeepFlow, Choice::Unlink]);
    assert_eq!(c.url.as_deref(), Some(url_of(3).as_str()));
    assert_eq!(c.flow["number"], json!(2));
    assert_eq!(c.tracker["number"], json!(1));
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f1"));
    assert_eq!(link_of(&out.doc, "t1").base, link_of(&d, "t1").base);
}

/// FTS-FR-SZGF: `keep_flow` writes the Flow's value to the tracker and sets
/// the base to it.
#[test]
fn keep_flow_writes_the_flow_parent_back() {
    let (tracker, d) = wide_plan();
    tracker.edit(3, |i| i.parent = Some(key(1)));
    let first = sync(&tracker, d.clone());
    let pick = choose(&first.result.conflicts[0], Choice::KeepFlow);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    assert_eq!(tracker.writes(), vec!["set_parent 3 1->2"]);
    assert_eq!(tracker.issue(3).parent.map(|k| k.number), Some(2));
    assert!(out.result.conflicts.is_empty());
    assert!(out.result.stale.is_empty());
}

/// FTS-FR-SZGF / FTS-FR-YHVD: `unlink` removes the link, keeps the item as a
/// local-only item, ignores its issue, and the call creates no issue for it.
#[test]
fn unlink_of_an_invalid_change_keeps_the_item_without_a_link() {
    let (tracker, d) = wide_plan();
    tracker.edit(3, |i| i.parent = Some(key(1)));
    let first = sync(&tracker, d.clone());
    let pick = choose(&first.result.conflicts[0], Choice::Unlink);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    assert!(!has_link(&out.doc, "t1"));
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f1"));
    assert_eq!(find(&out.doc, "t1").local_only, Some(true));
    let ignored = &out.doc.sync.as_ref().unwrap().ignored;
    assert_eq!(ignored.iter().map(|r| r.number).collect::<Vec<_>>(), vec![3]);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty());
    assert_eq!(out.result.created, 0);
}

/// FTS-FR-AIJM / FTS-FR-CJVS: a Type the tracker changed changes the item's
/// kind where its container permits the new kind.
#[test]
fn a_type_the_tracker_changed_changes_the_kind() {
    let (tracker, d) = plan();
    tracker.edit(3, |i| i.issue_type = Some("Bug".into()));
    let out = sync(&tracker, d);
    assert_eq!(find(&out.doc, "t1").kind, Kind::Defect);
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Bug".into())));
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
}

/// FTS-FR-SZGF: a Type that gives a container with members a leaf kind is
/// `invalid_on_tracker`.
#[test]
fn a_type_the_hierarchy_refuses_is_invalid_on_tracker() {
    let (tracker, d) = plan();
    tracker.edit(2, |i| i.issue_type = Some("Task".into()));
    let out = sync(&tracker, d);
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field), (ConflictKind::InvalidOnTracker, Some("type")));
    assert_eq!((c.flow.clone(), c.tracker.clone()), (json!("Feature"), json!("Task")));
    assert_eq!(find(&out.doc, "f1").kind, Kind::Feature);
}

/// FTS-FR-SZGF: a tracker milestone that no milestone item of the Flow links
/// is `invalid_on_tracker`.
#[test]
fn a_milestone_the_flow_does_not_hold_is_invalid_on_tracker() {
    let (tracker, d) = plan();
    tracker.edit(2, |i| i.milestone = Some(99));
    let out = sync(&tracker, d);
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field), (ConflictKind::InvalidOnTracker, Some("milestone")));
    assert_eq!((c.flow.clone(), c.tracker.clone()), (json!(5), json!(99)));
}

/// FTS-FR-VEEG / FTS-FR-PBQC: a parent both sides changed differently, where
/// the tracker's value is valid, is a field conflict that offers both sides.
#[test]
fn a_parent_both_changed_is_a_field_conflict() {
    let (tracker, mut d) = wide_plan();
    let tracker = tracker.with_issue(child(8, "Payouts", "Feature", None, Some(5)));
    let f3: Item = serde_json::from_value(item("f3", "feature", "Payouts", Some("m1"))).unwrap();
    d.items.push(f3);
    let f3_link: SyncLink = serde_json::from_value(link(
        "f3",
        "issue",
        8,
        issue_base("Payouts", Some("Feature"), None, Some(5), &[]),
    ))
    .unwrap();
    d.sync.as_mut().unwrap().links.push(f3_link);
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f3".into());
    tracker.edit(3, |i| i.parent = Some(key(6)));
    let out = sync(&tracker, d.clone());
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field), (ConflictKind::Field, Some("parent")));
    assert_eq!(c.choices, vec![Choice::KeepFlow, Choice::TakeTracker]);
    assert!(tracker.writes().is_empty());

    // FTS-FR-PBQC: `take_tracker` moves the item and sets the base.
    let pick = choose(c, Choice::TakeTracker);
    let out = resolve(&tracker, d, &[pick]);
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f2"));
    assert_eq!(link_of(&out.doc, "t1").base.parent.clone().flatten().map(|p| p.number), Some(6));
    assert!(out.result.conflicts.is_empty());
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-SZGF: a parent both sides changed, where the tracker's value is not
/// valid, is `invalid_on_tracker`, so `take_tracker` is never offered.
#[test]
fn a_parent_both_changed_to_an_invalid_place_is_invalid_on_tracker() {
    let (tracker, mut d) = wide_plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f2".into());
    tracker.edit(3, |i| i.parent = Some(key(1)));
    let out = sync(&tracker, d);
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field), (ConflictKind::InvalidOnTracker, Some("parent")));
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-HLVJ / FTS-FR-AIJM: an item the Flow moved into a container of
/// another milestone gets both its new parent and its new milestone.
#[test]
fn a_move_to_another_milestone_writes_parent_and_milestone() {
    let (tracker, mut d) = wide_plan();
    let tracker = tracker.with_issue(child(8, "Payouts", "Feature", None, Some(7)));
    d.items.push(serde_json::from_value(item("f3", "feature", "Payouts", Some("m2"))).unwrap());
    let f3_link: SyncLink = serde_json::from_value(link(
        "f3",
        "issue",
        8,
        issue_base("Payouts", Some("Feature"), None, Some(7), &[]),
    ))
    .unwrap();
    d.sync.as_mut().unwrap().links.push(f3_link);
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f3".into());
    let out = sync(&tracker, d);
    let mut writes = tracker.writes();
    writes.sort();
    assert_eq!(writes, vec!["set_milestone 3 5->7", "set_parent 3 2->8"]);
    let t1 = tracker.issue(3);
    assert_eq!((t1.parent.map(|k| k.number), t1.milestone), (Some(8), Some(7)));
    let base = &link_of(&out.doc, "t1").base;
    assert_eq!(base.milestone, Some(Some(7)));
    assert_eq!(base.parent.clone().flatten().map(|p| p.number), Some(8));
}

/// FTS-FR-AIJM: a kind the Flow and the tracker both changed to the same
/// Type only updates the base.
#[test]
fn a_type_both_sides_changed_alike_only_updates_the_base() {
    let (tracker, mut d) = plan();
    let t1 = d.items.iter_mut().find(|i| i.id == "t1").unwrap();
    t1.kind = Kind::Defect;
    tracker.edit(3, |i| i.issue_type = Some("Bug".into()));
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Bug".into())));
    assert!(out.result.conflicts.is_empty());
}

fn type_conflict() -> (FakeTracker, FlowDocument, SyncConflict) {
    let (tracker, d) = plan();
    tracker.edit(2, |i| i.issue_type = Some("Task".into()));
    let c = sync(&tracker, d.clone()).result.conflicts.remove(0);
    assert_eq!((c.kind, c.field), (ConflictKind::InvalidOnTracker, Some("type")));
    tracker.clear_calls();
    (tracker, d, c)
}

/// FTS-FR-SZGF: `keep_flow` on an `invalid_on_tracker` Type writes the
/// Flow's Type back, sets the base, and the next sync finds nothing to do.
#[test]
fn keep_flow_of_an_invalid_type_writes_the_flow_type_back() {
    let (tracker, d, c) = type_conflict();
    let out = resolve(&tracker, d, &[choose(&c, Choice::KeepFlow)]);
    assert_eq!(tracker.writes(), vec!["update_issue 2 type"]);
    assert_eq!(tracker.issue(2).issue_type.as_deref(), Some("Feature"));
    assert_eq!(link_of(&out.doc, "f1").base.r#type, Some(Some("Feature".into())));
    assert_eq!(find(&out.doc, "f1").kind, Kind::Feature);
    assert!(out.result.conflicts.is_empty() && out.result.stale.is_empty());
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
}

/// FTS-FR-SZGF / FTS-FR-YHVD / FTS-FR-HLVJ / FTS-FR-EGJL / FTS-FR-AIJM: `unlink` on an
/// `invalid_on_tracker` Type removes the link, ignores the issue, and makes
/// the item local only. The item keeps its kind, and the resolve writes
/// nothing to the tracker. A later sync does not pull issue 2 back.
///
/// Today's behaviour, pinned for a later decision of the user: the unlinked
/// container `f1` has no link now, so the nearest linked ancestor of its
/// member `t1` is the epic `ep1` (FTS-FR-HLVJ). The resolve does not see
/// this, but the next plain sync does: it moves issue 3 on the tracker from
/// parent 2 to parent 1 (`set_parent 3 2->1`) and sets the base of `t1` to
/// parent 1. The sync after that writes nothing.
#[test]
fn unlink_of_an_invalid_type_ignores_the_issue() {
    let (tracker, d, c) = type_conflict();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Unlink)]);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(tracker.issue(2).issue_type.as_deref(), Some("Task"));
    assert!(!has_link(&out.doc, "f1"));
    let f1 = find(&out.doc, "f1");
    assert_eq!((f1.kind, f1.local_only), (Kind::Feature, Some(true)));
    let ignored = &out.doc.sync.as_ref().unwrap().ignored;
    assert_eq!(ignored.iter().map(|r| r.number).collect::<Vec<_>>(), vec![2]);
    assert_eq!(link_of(&out.doc, "t1").base.parent.clone().flatten().map(|p| p.number), Some(2));
    assert!(out.result.conflicts.is_empty());
    assert_valid(&out.doc);

    // The next sync: issue 2 stays out of the Flow, and the member of the
    // unlinked container moves to the epic's issue on the tracker.
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert_eq!(tracker.writes(), vec!["set_parent 3 2->1"]);
    assert_eq!(tracker.issue(3).parent.map(|k| k.number), Some(1));
    assert_eq!((again.result.added, again.result.updated), (0, 1));
    assert!(again.result.conflicts.is_empty(), "{:?}", again.result.conflicts);
    assert!(again.result.skipped.is_empty(), "{:?}", again.result.skipped);
    assert_eq!(again.doc.items.iter().filter(|i| i.name == "Card capture").count(), 1);
    assert!(!has_link(&again.doc, "f1"));
    assert_eq!(find(&again.doc, "t1").parent_id.as_deref(), Some("f1"));
    let base = &link_of(&again.doc, "t1").base;
    assert_eq!(base.parent.clone().flatten().map(|p| p.number), Some(1));
    assert_valid(&again.doc);

    // The sync after it finds both sides in sync.
    tracker.clear_calls();
    let last = sync(&tracker, again.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(last.result.conflicts.is_empty());
    assert!(!last.doc_changed);
}

/// FTS-FR-ARHM: `take_tracker` is not a choice of an `invalid_on_tracker`
/// conflict: it applies nothing, is stale, and the conflict stands.
#[test]
fn a_choice_an_invalid_conflict_does_not_offer_is_stale() {
    let (tracker, d, c) = type_conflict();
    let out = resolve(&tracker, d, &[choose(&c, Choice::TakeTracker)]);
    assert_eq!(out.result.stale, vec![c.id.clone()]);
    assert_eq!(out.result.conflicts, vec![c]);
    assert_eq!(find(&out.doc, "f1").kind, Kind::Feature);
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-SZGF / FTS-FR-AIJM: a parent in another repository that the Flow
/// clears is removed on the tracker for real, where the parent is, and the
/// base takes the cleared parent.
#[test]
fn a_parent_in_another_repository_is_cleared_for_real() {
    let (tracker, mut d) = plan();
    let mut upstream = key(9);
    upstream.repo = "upstream".into();
    tracker.edit(2, |i| i.parent = Some(upstream));
    d.items.iter_mut().find(|i| i.id == "f1").unwrap().parent_id = Some("m1".into());
    let first = sync(&tracker, d.clone());
    let c = first.result.conflicts[0].clone();
    assert_eq!((c.kind, c.field), (ConflictKind::InvalidOnTracker, Some("parent")));
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(&c, Choice::KeepFlow)]);
    assert_eq!(tracker.writes(), vec!["set_parent 2 acme/upstream#9->none"]);
    assert_eq!(tracker.issue(2).parent, None);
    assert_eq!(link_of(&out.doc, "f1").base.parent, Some(None));
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
}
