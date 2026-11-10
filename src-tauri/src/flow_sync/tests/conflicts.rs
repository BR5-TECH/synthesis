//! FTS-FR-VEEG, FTS-FR-AONJ, FTS-FR-PBQC, FTS-FR-ARHM, FTS-FR-BMWI,
//! FTS-FR-VXMN: conflicts, their ids, and the author's choices.

use super::*;
use crate::flow_validation::model::Kind;

fn name_conflict() -> (FakeTracker, FlowDocument) {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    tracker.edit(3, |i| i.title = "Tokenize PANs".into());
    (tracker, d)
}

/// FTS-FR-VEEG / FTS-FR-AIJM: a field both sides changed differently is a
/// conflict that carries the item, the field, and the three values.
#[test]
fn a_field_both_changed_differently_is_a_conflict() {
    let (tracker, d) = name_conflict();
    let out = sync(&tracker, d);
    assert_eq!(out.result.conflicts.len(), 1);
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field, c.item_id.as_str()), (ConflictKind::Field, Some("name"), "t1"));
    assert_eq!(c.item_name, "Tokenize cards");
    assert_eq!(
        (c.base.clone(), c.flow.clone(), c.tracker.clone()),
        (json!("Tokenize"), json!("Tokenize cards"), json!("Tokenize PANs"))
    );
    assert_eq!(c.choices, vec![Choice::KeepFlow, Choice::TakeTracker]);
    assert_eq!(c.url.as_deref(), Some(url_of(3).as_str()));
    assert!(c.id.starts_with("fsc_"));
}

/// FTS-FR-AONJ: a sync changes neither side of a conflicting field and keeps
/// its base, so the next sync reports the same conflict with the same id.
#[test]
fn a_conflict_is_reported_again_with_the_same_id() {
    let (tracker, d) = name_conflict();
    let first = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert_eq!(find(&first.doc, "t1").name, "Tokenize cards");
    assert_eq!(link_of(&first.doc, "t1").base.name, "Tokenize");
    assert_eq!(tracker.issue(3).title, "Tokenize PANs");
    let second = sync(&tracker, first.doc.clone());
    assert_eq!(second.result.conflicts, first.result.conflicts);
}

/// FTS-FR-VEEG: the id is derived from the Flow, the item, the field, and the
/// two values, so another value or another Flow gives another id.
#[test]
fn the_conflict_id_follows_the_values_and_the_flow() {
    let (tracker, d) = name_conflict();
    let first = sync(&tracker, d.clone()).result.conflicts[0].id.clone();
    tracker.edit(3, |i| i.title = "Tokenize all".into());
    let other_value = sync(&tracker, d.clone()).result.conflicts[0].id.clone();
    assert_ne!(first, other_value);
    let settings = settings();
    let other_flow = run(
        &tracker,
        SyncRequest { flow_id: "plans/other.flow", doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap()
    .result
    .conflicts[0]
        .id
        .clone();
    assert_ne!(other_value, other_flow);
}

/// FTS-FR-PBQC: `keep_flow` writes the Flow's value to the tracker and sets
/// the base to it.
#[test]
fn keep_flow_writes_the_flow_value() {
    let (tracker, d) = name_conflict();
    let pick = choose(&sync(&tracker, d.clone()).result.conflicts[0], Choice::KeepFlow);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    assert_eq!(tracker.writes(), vec!["update_issue 3 title"]);
    assert_eq!(tracker.issue(3).title, "Tokenize cards");
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize cards");
    assert!(out.result.conflicts.is_empty() && out.result.stale.is_empty());
    assert_eq!(out.result.updated, 1);
}

/// FTS-FR-PBQC: `take_tracker` writes the tracker's value to the Flow and
/// sets the base to it.
#[test]
fn take_tracker_writes_the_tracker_value() {
    let (tracker, d) = name_conflict();
    let pick = choose(&sync(&tracker, d.clone()).result.conflicts[0], Choice::TakeTracker);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    assert!(tracker.writes().is_empty());
    assert_eq!(find(&out.doc, "t1").name, "Tokenize PANs");
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize PANs");
    assert!(out.result.conflicts.is_empty());
}

/// FTS-FR-ARHM: a resolution whose conflict changed since the sync is
/// returned as stale, applies nothing, and the new conflict stands.
#[test]
fn a_resolution_that_no_longer_matches_is_stale() {
    let (tracker, d) = name_conflict();
    let old = sync(&tracker, d.clone()).result.conflicts[0].clone();
    tracker.edit(3, |i| i.title = "Tokenize again".into());
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(&old, Choice::KeepFlow)]);
    assert_eq!(out.result.stale, vec![old.id.clone()]);
    assert!(tracker.writes().is_empty());
    assert_eq!(tracker.issue(3).title, "Tokenize again");
    assert_eq!(out.result.conflicts.len(), 1);
    assert_ne!(out.result.conflicts[0].id, old.id);
}

/// FTS-FR-ARHM: a choice that the conflict does not offer applies nothing,
/// the conflict stands, and the resolution is returned as stale.
#[test]
fn a_choice_the_conflict_does_not_offer_applies_nothing() {
    let (tracker, d) = name_conflict();
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Restore)]);
    assert_eq!(out.result.conflicts, vec![c.clone()]);
    assert_eq!(out.result.stale, vec![c.id]);
    assert_eq!(find(&out.doc, "t1").name, "Tokenize cards");
}

/// FTS-FR-BMWI: a link whose item is gone from the Flow is `removed_in_flow`,
/// named with the tracker title, and nothing is written.
#[test]
fn a_link_whose_item_is_gone_is_removed_in_flow() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.field, c.item_id.as_str()), (ConflictKind::RemovedInFlow, None, "t1"));
    assert_eq!(c.item_name, "Tokenize");
    assert_eq!(c.choices, vec![Choice::Unlink, Choice::Restore]);
    assert_eq!(c.url.as_deref(), Some(url_of(3).as_str()));
    assert!(has_link(&out.doc, "t1"));
}

/// FTS-FR-BMWI / FTS-FR-YHVD: `unlink` removes the link, adds its reference
/// to `sync.ignored`, and leaves the tracker item as it is.
#[test]
fn unlink_of_a_removed_item_drops_the_link() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    let pick = choose(&sync(&tracker, d.clone()).result.conflicts[0], Choice::Unlink);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    assert!(!has_link(&out.doc, "t1"));
    let ignored = &out.doc.sync.as_ref().unwrap().ignored;
    assert_eq!(ignored, &vec![link_of(&plan().1, "t1").reference.clone()]);
    assert!(tracker.writes().is_empty());
    assert_eq!(tracker.issue(3).state, "open");
    assert!(out.result.conflicts.is_empty());
    assert_eq!(out.result.changed, 0, "no item of the Flow changed");
    assert_valid(&out.doc);
}

/// FTS-FR-YHVD / FTS-FR-EGJL: after `unlink` of an item gone from the Flow, a
/// plain sync does not pull its issue back, though it is still a sub-issue
/// of a linked issue.
#[test]
fn after_unlink_a_plain_sync_does_not_pull_the_issue_back() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    let pick = choose(&sync(&tracker, d.clone()).result.conflicts[0], Choice::Unlink);
    let unlinked = resolve(&tracker, d, &[pick]).doc;
    tracker.clear_calls();
    let next = sync(&tracker, unlinked);
    assert_eq!(next.result.added, 0);
    assert!(next.doc.items.iter().all(|i| i.name != "Tokenize"));
    assert!(next.result.conflicts.is_empty());
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!next.doc_changed);
}

/// FTS-FR-BMWI: `restore` adds the item back from the tracker values, in the
/// container its parent and milestone give, with the kind its Type gives.
#[test]
fn restore_adds_the_item_back_from_the_tracker() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    tracker.edit(3, |i| i.title = "Tokenize PANs".into());
    let pick = choose(&sync(&tracker, d.clone()).result.conflicts[0], Choice::Restore);
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[pick]);
    let t1 = find(&out.doc, "t1");
    assert_eq!((t1.kind, t1.name.as_str()), (Kind::Task, "Tokenize PANs"));
    assert_eq!(t1.parent_id.as_deref(), Some("f1"));
    assert_eq!(link_of(&out.doc, "t1").base.name, "Tokenize PANs");
    assert_eq!(out.result.added, 1);
    assert!(out.result.conflicts.is_empty());
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-BMWI / FGV-FR-YHFX: a restored milestone goes to the top level,
/// below the lowest top-level item.
#[test]
fn restore_of_a_milestone_goes_to_the_top_level() {
    let (tracker, mut d) = plan();
    // The milestone's members go too, or the plan would not be valid.
    d.items.clear();
    let mut m2 = item("m2", "milestone", "Later", None);
    m2["localOnly"] = json!(true);
    d.items.push(serde_json::from_value(m2).unwrap());
    d.sync.as_mut().unwrap().links.retain(|l| l.item_id == "m1");
    let first = sync(&tracker, d.clone());
    let c = first.result.conflicts.iter().find(|c| c.item_id == "m1").unwrap();
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(c, Choice::Restore)]);
    let m1 = find(&out.doc, "m1");
    assert_eq!(
        (m1.kind, m1.parent_id.as_deref(), m1.name.as_str()),
        (Kind::Milestone, None, "Beta")
    );
    let m2 = find(&out.doc, "m2");
    assert!(m1.position.y >= m2.position.y + m2.size.unwrap().height, "{m1:?}");
    assert_eq!(out.result.added, 1);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-BMWI: a restore that the plan cannot hold applies nothing, and the
/// conflict stands.
#[test]
fn a_restore_the_plan_cannot_hold_leaves_the_conflict() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    tracker.edit(3, |i| i.issue_type = None);
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Restore)]);
    assert_eq!(out.result.conflicts, vec![c]);
    assert!(out.result.stale.is_empty());
    assert!(out.doc.items.iter().all(|i| i.id != "t1"));
}

/// FTS-FR-VXMN: a link whose tracker item cannot be found is
/// `removed_on_tracker`, named with the Flow's name, with no URL.
#[test]
fn a_missing_tracker_item_is_removed_on_tracker() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().issues.remove(&3);
    let out = sync(&tracker, d);
    let c = &out.result.conflicts[0];
    assert_eq!((c.kind, c.item_id.as_str(), c.field), (ConflictKind::RemovedOnTracker, "t1", None));
    assert_eq!(c.item_name, "Tokenize");
    assert_eq!(c.choices, vec![Choice::RemoveItem, Choice::Unlink]);
    assert_eq!(c.url, None);
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-VXMN: a pull request at the linked number, and an issue moved to
/// another repository, are each `removed_on_tracker`.
#[test]
fn a_pull_request_or_a_transferred_issue_is_removed_on_tracker() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().pull_requests.push(3);
    tracker.edit(2, |i| i.repo = "elsewhere".into());
    let out = sync(&tracker, d);
    let mut gone: Vec<&str> = out
        .result
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::RemovedOnTracker)
        .map(|c| c.item_id.as_str())
        .collect();
    gone.sort();
    assert_eq!(gone, vec!["f1", "t1"]);
}

/// FTS-FR-VXMN: `remove_item` removes the item and its edges.
#[test]
fn remove_item_removes_the_item_and_its_edges() {
    let (tracker, mut d) = plan_with_two_tasks();
    d.edges.push(serde_json::from_value(blocks("e1", "t2", "t1")).unwrap());
    let links = &mut d.sync.as_mut().unwrap().links;
    let t1 = links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.base.blocked_by = Some(vec![serde_json::from_value(ref_json("issue", 4)).unwrap()]);
    tracker.state.borrow_mut().issues.remove(&3);
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(&c, Choice::RemoveItem)]);
    assert!(out.doc.items.iter().all(|i| i.id != "t1"));
    assert!(out.doc.edges.is_empty());
    assert!(!has_link(&out.doc, "t1"));
    assert!(out.doc.sync.as_ref().unwrap().ignored.is_empty(), "remove_item ignores nothing");
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
}

/// FTS-FR-VXMN: `remove_item` on a container removes every item under it
/// too. The links of the items under it stay, so the next sync reports them
/// as removed in the Flow.
#[test]
fn remove_item_on_a_container_removes_its_members() {
    let (tracker, d) = plan_with_two_tasks();
    tracker.state.borrow_mut().issues.remove(&2);
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    assert_eq!((c.kind, c.item_id.as_str()), (ConflictKind::RemovedOnTracker, "f1"));
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(&c, Choice::RemoveItem)]);
    let ids: Vec<&str> = out.doc.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["m1", "ep1"]);
    assert!(!has_link(&out.doc, "f1"));
    assert!(has_link(&out.doc, "t1") && has_link(&out.doc, "t2"));
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    let next = sync(&tracker, out.doc);
    let mut gone: Vec<(&str, ConflictKind)> =
        next.result.conflicts.iter().map(|c| (c.item_id.as_str(), c.kind)).collect();
    gone.sort_by_key(|(id, _)| *id);
    assert_eq!(
        gone,
        vec![("t1", ConflictKind::RemovedInFlow), ("t2", ConflictKind::RemovedInFlow)]
    );
}

/// FTS-FR-VXMN / FTS-FR-YHVD: `unlink` keeps the item as a local-only item,
/// removes the link, and ignores its issue. Neither the call nor a plain
/// sync after it creates an issue for the item.
#[test]
fn unlink_of_a_missing_item_keeps_it_and_no_sync_creates_it() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().issues.remove(&3);
    let c = sync(&tracker, d.clone()).result.conflicts[0].clone();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Unlink)]);
    assert!(!has_link(&out.doc, "t1"));
    let t1 = find(&out.doc, "t1");
    assert_eq!((t1.name.as_str(), t1.local_only), ("Tokenize", Some(true)));
    assert_eq!(out.doc.sync.as_ref().unwrap().ignored[0].number, 3);
    assert_eq!((out.result.created, out.result.changed), (0, 1));
    assert_valid(&out.doc);
    tracker.clear_calls();
    let next = sync(&tracker, out.doc);
    assert_eq!(next.result.created, 0);
    assert!(!has_link(&next.doc, "t1"));
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!next.doc_changed);
}

/// FTS-FR-BMWI / FTS-FR-VXMN: a link whose item is gone from both sides holds
/// nothing to decide and is removed.
#[test]
fn a_link_gone_on_both_sides_is_removed() {
    let (tracker, mut d) = plan();
    d.items.retain(|i| i.id != "t1");
    tracker.state.borrow_mut().issues.remove(&3);
    let out = sync(&tracker, d);
    assert!(!has_link(&out.doc, "t1"));
    assert!(out.result.conflicts.is_empty());
}

/// FTS-FR-VEEG: the wire shape of a conflict uses camelCase names and the
/// snake_case kind and choices the review panel reads.
#[test]
fn a_conflict_serializes_in_the_contract_shape() {
    let (tracker, d) = name_conflict();
    let c = sync(&tracker, d).result.conflicts.remove(0);
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(v["kind"], json!("field"));
    assert_eq!(v["itemId"], json!("t1"));
    assert_eq!(v["itemName"], json!("Tokenize cards"));
    assert_eq!(v["choices"], json!(["keep_flow", "take_tracker"]));
    let r: ConflictResolution =
        serde_json::from_value(json!({ "conflictId": c.id, "choice": "take_tracker" })).unwrap();
    assert_eq!(r.choice, Choice::TakeTracker);
}

/// FTS-FR-VXMN / FTS-FR-YHVD / FTS-FR-AIJM: `unlink` of a milestone link
/// whose milestone was deleted (GitHub then clears the milestone of its
/// issues) keeps the milestone item as a local-only item, removes the link,
/// and ignores the milestone's reference. The members' bases take the
/// cleared milestone, nothing is written, and no later sync creates a
/// milestone for the item.
#[test]
fn unlink_of_a_missing_milestone_keeps_it_and_no_sync_creates_it() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().milestones.remove(&5);
    for n in 1..=3 {
        tracker.edit(n, |i| i.milestone = None);
    }
    let first = sync(&tracker, d.clone());
    assert_eq!(first.result.conflicts.len(), 1, "{:?}", first.result.conflicts);
    let c = first.result.conflicts[0].clone();
    assert_eq!(
        (c.kind, c.item_id.as_str(), c.choices.clone()),
        (ConflictKind::RemovedOnTracker, "m1", vec![Choice::RemoveItem, Choice::Unlink])
    );
    tracker.clear_calls();
    let out = resolve(&tracker, d, &[choose(&c, Choice::Unlink)]);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert_eq!((out.result.created, out.result.changed), (0, 1));
    assert!(!has_link(&out.doc, "m1"));
    let m1 = find(&out.doc, "m1");
    assert_eq!((m1.kind, m1.local_only), (Kind::Milestone, Some(true)));
    assert_eq!(find(&out.doc, "ep1").parent_id.as_deref(), Some("m1"));
    let ignored = &out.doc.sync.as_ref().unwrap().ignored;
    assert_eq!(ignored, &vec![link_of(&plan().1, "m1").reference.clone()]);
    for id in ["ep1", "f1", "t1"] {
        assert_eq!(link_of(&out.doc, id).base.milestone, Some(None), "{id}");
    }
    assert_valid(&out.doc);

    tracker.clear_calls();
    let next = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(next.result.conflicts.is_empty(), "{:?}", next.result.conflicts);
    assert_eq!(next.result.created, 0);
    assert!(!has_link(&next.doc, "m1"));
    assert!(!next.doc_changed);
}
