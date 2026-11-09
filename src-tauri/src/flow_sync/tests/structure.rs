//! FTS-FR-AIJM, FTS-FR-SZGF, FTS-FR-PBQC: `parent`, `milestone`, and `type`,
//! which move an item or change its kind.

use super::*;
use crate::flow_validation::model::{Kind, SyncLink};

/// [`plan`] with feature `f2` (issue 6) in `m1` and milestone `m2`
/// (milestone 7) in `d1`, all in sync.
fn wide_plan() -> (FakeTracker, FlowDocument) {
    let (tracker, mut d) = plan();
    let tracker = tracker
        .with_issue(child(6, "Wallets", "Feature", Some(1), Some(5)))
        .with_milestone(milestone(7, "GA"));
    let f2: Item = serde_json::from_value(item("f2", "feature", "Wallets", Some("m1"))).unwrap();
    let m2: Item = serde_json::from_value(item("m2", "milestone", "GA", Some("d1"))).unwrap();
    d.items.push(f2);
    d.items.push(m2);
    let links = &mut d.sync.as_mut().unwrap().links;
    let f2_link: SyncLink = serde_json::from_value(link(
        "f2",
        "issue",
        6,
        issue_base("Wallets", Some("Feature"), Some(1), Some(5), &[]),
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
    assert_eq!(tracker.writes(), vec!["set_parent 3"]);
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
    assert_eq!(tracker.writes(), vec!["set_milestone 6"]);
    assert_eq!(tracker.issue(6).milestone, Some(7));
    assert_eq!(link_of(&out.doc, "f2").base.milestone, Some(Some(7)));
}

/// FTS-FR-AIJM / FTS-FR-EGJL: a milestone the tracker changed moves the
/// feature into the milestone item, which is under its parent's item.
#[test]
fn a_milestone_the_tracker_changed_moves_the_item() {
    let (tracker, d) = wide_plan();
    tracker.edit(2, |i| i.milestone = Some(7));
    let out = sync(&tracker, d);
    assert_eq!(find(&out.doc, "f1").parent_id.as_deref(), Some("m2"));
    assert_eq!(find(&out.doc, "t1").parent_id.as_deref(), Some("f1"));
    assert_eq!(link_of(&out.doc, "f1").base.milestone, Some(Some(7)));
    assert_valid(&out.doc);
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
    assert_eq!(tracker.writes(), vec!["set_parent 3"]);
    assert_eq!(tracker.issue(3).parent.map(|k| k.number), Some(2));
    assert!(out.result.conflicts.is_empty());
    assert!(out.result.stale.is_empty());
}

/// FTS-FR-SZGF: `unlink` removes the link and keeps the item, and the call
/// creates no issue for it.
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
    let tracker = tracker.with_issue(child(8, "Payouts", "Feature", Some(1), Some(5)));
    let f3: Item = serde_json::from_value(item("f3", "feature", "Payouts", Some("m1"))).unwrap();
    d.items.push(f3);
    let f3_link: SyncLink = serde_json::from_value(link(
        "f3",
        "issue",
        8,
        issue_base("Payouts", Some("Feature"), Some(1), Some(5), &[]),
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
    let tracker = tracker.with_issue(child(8, "Payouts", "Feature", Some(1), Some(7)));
    d.items.push(serde_json::from_value(item("f3", "feature", "Payouts", Some("m2"))).unwrap());
    let f3_link: SyncLink = serde_json::from_value(link(
        "f3",
        "issue",
        8,
        issue_base("Payouts", Some("Feature"), Some(1), Some(7), &[]),
    ))
    .unwrap();
    d.sync.as_mut().unwrap().links.push(f3_link);
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().parent_id = Some("f3".into());
    let out = sync(&tracker, d);
    let mut writes = tracker.writes();
    writes.sort();
    assert_eq!(writes, vec!["set_milestone 3", "set_parent 3"]);
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
