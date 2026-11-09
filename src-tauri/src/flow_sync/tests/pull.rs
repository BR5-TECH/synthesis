//! FTS-FR-EGJL, FTS-FR-CJVS, FTS-FR-LKZO, FTS-FR-WANV: sub-issues that come
//! into the Flow, and links of another repository.

use super::*;
use crate::flow_sync::values::{default_size, CARD_H, CONTAINER_PADDING};
use crate::flow_validation::model::{Kind, Size};

/// FTS-FR-EGJL: an open sub-issue of a linked issue that no link names comes
/// into the Flow in its parent's item, with a link whose base is the tracker
/// values. Nothing is written to the tracker.
#[test]
fn a_sub_issue_no_link_names_is_pulled() {
    let (tracker, d) = plan();
    let tracker = tracker.with_issue(child(9, "Card form", "Task", Some(2), Some(5)));
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").expect("pulled");
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Task, Some("f1")));
    let l = link_of(&out.doc, &pulled.id);
    assert_eq!(l.reference.number, 9);
    assert_eq!(l.base.name, "Card form");
    assert_eq!(l.base.milestone, Some(Some(5)));
    assert_eq!(l.base.parent.clone().flatten().map(|p| p.number), Some(2));
    assert_eq!((out.result.added, out.result.changed), (1, 0));
    assert_valid(&out.doc);
    // The next sync finds it in sync.
    let again = sync(&tracker, out.doc);
    assert!(!again.doc_changed);
}

/// FTS-FR-EGJL: a closed sub-issue is pulled too.
#[test]
fn a_closed_sub_issue_is_pulled() {
    let (tracker, d) = plan();
    let mut done = child(9, "Old spike", "Task", Some(2), Some(5));
    done.state = "closed".into();
    let tracker = tracker.with_issue(done);
    let out = sync(&tracker, d);
    let pulled = out.doc.items.iter().find(|i| i.name == "Old spike").expect("pulled");
    assert_eq!(link_of(&out.doc, &pulled.id).base.state, "closed");
}

/// FTS-FR-EGJL / FTS-FR-CJVS: a sub-issue of the deliverable goes into the
/// linked milestone item of its milestone, which is under the deliverable,
/// and takes the first kind of its Type that the milestone permits.
#[test]
fn a_sub_issue_goes_into_its_milestone_item_with_the_first_kind_that_fits() {
    let (tracker, d) = plan();
    let tracker = tracker.with_issue(child(9, "Wallets", "Feature", Some(1), Some(5)));
    let out = sync(&tracker, d);
    let pulled = out.doc.items.iter().find(|i| i.name == "Wallets").expect("pulled");
    // `deliverable` does not sit in a milestone; `epic` does, and its Type is
    // `Feature` by default.
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Epic, Some("m1")));
    assert_eq!(pulled.id, "ep1");
    assert_valid(&out.doc);
}

/// FTS-FR-EGJL / FTS-FR-SZGF: a milestone item that is not under the parent's
/// item does not hold the pulled item, which goes into the parent's item; the
/// milestone the plan cannot give it is then `invalid_on_tracker`.
#[test]
fn a_milestone_outside_the_parent_leaves_the_item_in_the_parent() {
    let (tracker, mut d) = plan();
    let tracker = tracker
        .with_issue(child(9, "Card form", "Task", Some(2), Some(7)))
        .with_milestone(milestone(7, "GA"));
    d.items.push(serde_json::from_value(item("m2", "milestone", "GA", Some("d1"))).unwrap());
    d.sync.as_mut().unwrap().links.push(
        serde_json::from_value(link("m2", "milestone", 7, milestone_base("GA", None))).unwrap(),
    );
    let out = sync(&tracker, d);
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").expect("pulled");
    assert_eq!(pulled.parent_id.as_deref(), Some("f1"));
    let c = &out.result.conflicts[0];
    assert_eq!(
        (c.kind, c.field, c.item_id.as_str()),
        (ConflictKind::InvalidOnTracker, Some("milestone"), pulled.id.as_str())
    );
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-CJVS: an issue for which no kind fits is not pulled and is
/// reported as skipped with `no_kind_fits`.
#[test]
fn a_sub_issue_no_kind_fits_is_skipped() {
    let (tracker, d) = plan();
    let mut untyped = child(9, "Load test", "Task", Some(2), Some(5));
    untyped.issue_type = None;
    let tracker =
        tracker.with_issue(untyped).with_issue(child(10, "Payouts", "Task", Some(1), None));
    let out = sync(&tracker, d.clone());
    assert_eq!(out.doc.items.len(), d.items.len());
    let mut skipped = out.result.skipped.clone();
    skipped.sort_by_key(|s| s.number);
    assert_eq!(
        skipped,
        vec![
            SkippedIssue {
                number: Some(9),
                title: "Load test".into(),
                reason: SKIP_NO_KIND_FITS,
                item_id: None
            },
            SkippedIssue {
                number: Some(10),
                title: "Payouts".into(),
                reason: SKIP_NO_KIND_FITS,
                item_id: None
            },
        ]
    );
}

/// FTS-FR-LKZO: a pulled item is placed below the last member of its
/// container, and the container grows to hold it.
#[test]
fn a_pulled_item_goes_below_the_last_member() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "f1").unwrap().size =
        Some(Size { width: 400.0, height: 200.0 });
    let tracker = tracker.with_issue(child(9, "Card form", "Task", Some(2), Some(5)));
    let out = sync(&tracker, d);
    let t1 = find(&out.doc, "t1");
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").unwrap();
    assert_eq!(pulled.position.x, CONTAINER_PADDING);
    assert_eq!(pulled.position.y, t1.position.y + CARD_H + CONTAINER_PADDING);
    let f1 = find(&out.doc, "f1").size.unwrap();
    assert!(f1.height >= pulled.position.y + CARD_H + CONTAINER_PADDING, "{f1:?}");
}

/// FTS-FR-LKZO: a pulled container takes its default size, and its own
/// sub-issues are pulled in the same sync.
#[test]
fn a_pulled_container_takes_a_default_size_and_brings_its_sub_issues() {
    let (tracker, d) = plan();
    let tracker = tracker
        .with_issue(child(9, "Wallets", "Feature", Some(1), Some(5)))
        .with_issue(child(10, "Apple Pay", "Feature", Some(9), Some(5)))
        .with_issue(child(11, "Token vault", "Task", Some(10), Some(5)));
    let out = sync(&tracker, d);
    let epic = out.doc.items.iter().find(|i| i.name == "Wallets").unwrap();
    assert_eq!(epic.kind, Kind::Epic);
    assert_eq!(epic.size, default_size(Kind::Epic));
    let feature = out.doc.items.iter().find(|i| i.name == "Apple Pay").unwrap();
    assert_eq!(
        (feature.kind, feature.parent_id.as_deref()),
        (Kind::Feature, Some(epic.id.as_str()))
    );
    let task = out.doc.items.iter().find(|i| i.name == "Token vault").unwrap();
    assert_eq!((task.kind, task.parent_id.as_deref()), (Kind::Task, Some(feature.id.as_str())));
    assert_eq!(out.result.added, 3);
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
}

/// FTS-FR-EGJL: a sub-issue that a link already names elsewhere is not
/// pulled a second time.
#[test]
fn a_sub_issue_a_link_names_is_not_pulled() {
    let (tracker, d) = plan();
    let out = sync(&tracker, d.clone());
    assert_eq!(out.doc.items.len(), d.items.len());
    assert_eq!(out.result.added, 0);
}

/// FTS-FR-WANV: a link whose reference names another repository is not
/// synced and is reported as skipped with `foreign_repository`.
#[test]
fn a_link_of_another_repository_is_skipped() {
    let (tracker, mut d) = plan();
    let t1 = d.sync.as_mut().unwrap().links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.reference.repo = "other".into();
    t1.reference.number = 77;
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Changed".into();
    // Issue 3 of this repository is not the linked issue any more.
    tracker.state.borrow_mut().issues.remove(&3);
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert!(out.result.conflicts.is_empty());
    assert_eq!(
        out.result.skipped,
        vec![SkippedIssue {
            number: Some(77),
            title: "Tokenize".into(),
            reason: SKIP_FOREIGN_REPOSITORY,
            item_id: None,
        }]
    );
    let read = tracker.calls().into_iter().find(|c| c.starts_with("read_issues")).unwrap();
    assert_eq!(read, "read_issues 2", "the foreign link is not read");
}

/// FTS-FR-WANV / FTS-FR-EGJL: a sub-issue in another repository is not
/// pulled and is reported as skipped with `foreign_repository`.
#[test]
fn a_sub_issue_of_another_repository_is_skipped() {
    let (tracker, d) = plan();
    let mut foreign = child(12, "Upstream fix", "Task", Some(2), None);
    foreign.repo = "upstream".into();
    tracker.state.borrow_mut().foreign_sub_issues.push((2, foreign));
    let out = sync(&tracker, d.clone());
    assert_eq!(out.doc.items.len(), d.items.len());
    assert_eq!(out.result.skipped[0].reason, SKIP_FOREIGN_REPOSITORY);
    assert_eq!(out.result.skipped[0].number, Some(12));
}
