//! FTS-FR-EGJL, FTS-FR-CJVS, FTS-FR-LKZO, FTS-FR-WANV, FTS-FR-YHVD:
//! sub-issues that come into the Flow, the ones it ignores, and links of
//! another repository.

use super::*;
use crate::flow_sync::values::{default_size, CARD_H, CONTAINER_PADDING};
use crate::flow_validation::model::{Kind, Size};

/// A sync with these flow tracker settings.
fn sync_with(
    tracker: &FakeTracker,
    doc: FlowDocument,
    settings: &FlowTrackerSettings,
) -> SyncOutcome {
    run(tracker, SyncRequest { flow_id: FLOW_ID, doc, settings, resolutions: &[] })
        .unwrap_or_else(|e| panic!("sync failed: {e:?}"))
}

/// Give the linked issue `number` of item `item_id` the Type `name` on both
/// sides and in its base, so a Type setting of `name` changes nothing there.
fn retype(tracker: &FakeTracker, d: &mut FlowDocument, item_id: &str, number: u64, name: &str) {
    tracker.edit(number, |i| i.issue_type = Some(name.into()));
    let links = &mut d.sync.as_mut().unwrap().links;
    links.iter_mut().find(|l| l.item_id == item_id).unwrap().base.r#type = Some(Some(name.into()));
}

/// The next sync of `doc` writes nothing, finds no conflict, and does not
/// change the Flow.
fn assert_next_sync_is_quiet(
    tracker: &FakeTracker,
    doc: FlowDocument,
    settings: &FlowTrackerSettings,
) {
    tracker.clear_calls();
    let again = sync_with(tracker, doc, settings);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty(), "{:?}", again.result.conflicts);
    assert_eq!(again.result.added, 0);
    assert!(!again.doc_changed);
}

/// FTS-FR-EGJL / FTS-FR-AIJM: an open sub-issue of a linked issue that no
/// link names comes into the Flow in its parent's item, with a link whose
/// base is the tracker values. Nothing is written to the tracker, and the
/// next sync finds both sides in sync.
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
    assert_next_sync_is_quiet(&tracker, out.doc, &settings());
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

/// FTS-FR-EGJL / FTS-FR-CJVS / FTS-FR-AIJM: a sub-issue of the epic goes
/// into the epic's item, since a milestone is never under a parent item,
/// and takes the kind of its Type. The next sync finds both sides in sync.
#[test]
fn a_sub_issue_of_an_epic_goes_into_the_epic() {
    let (tracker, d) = plan();
    let tracker = tracker
        .with_issue(child(9, "Wallets", "Feature", Some(1), Some(5)))
        .with_issue(child(10, "Wrong total", "Bug", Some(1), Some(5)));
    let out = sync(&tracker, d);
    let feature = out.doc.items.iter().find(|i| i.name == "Wallets").expect("pulled");
    assert_eq!((feature.kind, feature.parent_id.as_deref()), (Kind::Feature, Some("ep1")));
    let defect = out.doc.items.iter().find(|i| i.name == "Wrong total").expect("pulled");
    assert_eq!((defect.kind, defect.parent_id.as_deref()), (Kind::Defect, Some("ep1")));
    assert_eq!(
        link_of(&out.doc, &feature.id).base.parent.clone().flatten().map(|p| p.number),
        Some(1)
    );
    assert!(out.result.skipped.is_empty(), "{:?}", out.result.skipped);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings());
}

/// FTS-FR-CJVS: a pulled item takes the first kind, in the order epic,
/// feature, task, defect, whose Type matches and that its container
/// permits: with one Type for an epic and a feature, a sub-issue of an epic
/// is a feature, since an epic does not sit in an epic. The next sync finds
/// both sides in sync (FTS-FR-AIJM).
#[test]
fn a_pulled_item_takes_the_first_kind_its_container_permits() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().types.push("Work".into());
    retype(&tracker, &mut d, "ep1", 1, "Work");
    retype(&tracker, &mut d, "f1", 2, "Work");
    let tracker = tracker.with_issue(child(9, "Wallets", "Work", Some(1), Some(5)));
    let settings =
        FlowTrackerSettings { epic_type: "Work".into(), feature_type: "Work".into(), ..settings() };
    let out = sync_with(&tracker, d, &settings);
    let pulled = out.doc.items.iter().find(|i| i.name == "Wallets").expect("pulled");
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Feature, Some("ep1")));
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings);
}

/// FTS-FR-CJVS: where a feature and a defect share one Type, a sub-issue of
/// an epic is a feature, the earlier kind of the order that the epic
/// permits.
#[test]
fn a_feature_comes_before_a_defect_of_the_same_type() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().types.push("Work".into());
    retype(&tracker, &mut d, "f1", 2, "Work");
    let tracker = tracker.with_issue(child(9, "Wallets", "Work", Some(1), Some(5)));
    let settings = FlowTrackerSettings {
        feature_type: "Work".into(),
        defect_type: "Work".into(),
        ..settings()
    };
    let out = sync_with(&tracker, d, &settings);
    let pulled = out.doc.items.iter().find(|i| i.name == "Wallets").expect("pulled");
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Feature, Some("ep1")));
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings);
}

/// FTS-FR-CJVS: where a task and a defect share one Type, a sub-issue of a
/// feature is a task, the earlier kind of the order that the feature
/// permits.
#[test]
fn a_task_comes_before_a_defect_of_the_same_type() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().types.push("Work".into());
    retype(&tracker, &mut d, "t1", 3, "Work");
    let tracker = tracker.with_issue(child(9, "Card form", "Work", Some(2), Some(5)));
    let settings =
        FlowTrackerSettings { task_type: "Work".into(), defect_type: "Work".into(), ..settings() };
    let out = sync_with(&tracker, d, &settings);
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").expect("pulled");
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Task, Some("f1")));
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings);
}

/// FTS-FR-XITX / FTS-FR-CJVS: in a repository that offers no issue Type, a
/// sub-issue has no Type, so no kind fits it: it is not pulled and is
/// skipped with `no_kind_fits`. A changed Type in the settings writes no
/// `type` there, and finds no conflict.
#[test]
fn a_repository_without_types_pulls_no_sub_issue_and_writes_no_type() {
    let (tracker, d) = plan();
    tracker.state.borrow_mut().types.clear();
    for n in 1..=3 {
        tracker.edit(n, |i| i.issue_type = None);
    }
    let mut plain = child(9, "Card form", "Task", Some(2), Some(5));
    plain.issue_type = None;
    let tracker = tracker.with_issue(plain);
    let settings = FlowTrackerSettings { task_type: "Chore".into(), ..settings() };
    let out = sync_with(&tracker, d.clone(), &settings);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert_eq!(out.doc.items.len(), d.items.len());
    assert_eq!(
        out.result.skipped,
        vec![SkippedIssue {
            number: Some(9),
            title: "Card form".into(),
            url: Some(url_of(9)),
            reason: SKIP_NO_KIND_FITS,
            item_id: None
        }]
    );
    // The bases keep the Type they had: no `type` is compared.
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Task".into())));
    assert!(!out.doc_changed);
    tracker.clear_calls();
    let again = sync_with(&tracker, out.doc, &settings);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(again.result.skipped, out.result.skipped);
    assert!(!again.doc_changed);
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
    d.items.push(serde_json::from_value(item("m2", "milestone", "GA", None)).unwrap());
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
                url: Some(url_of(9)),
                reason: SKIP_NO_KIND_FITS,
                item_id: None
            },
            SkippedIssue {
                number: Some(10),
                title: "Payouts".into(),
                url: Some(url_of(10)),
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

/// FTS-FR-LKZO / FTS-FR-AIJM: a pulled container takes its default size,
/// and its own sub-issues are pulled in the same sync. The next sync finds
/// both sides in sync.
#[test]
fn a_pulled_container_takes_a_default_size_and_brings_its_sub_issues() {
    let (tracker, d) = plan();
    let tracker = tracker
        .with_issue(child(9, "Wallets", "Feature", Some(1), Some(5)))
        .with_issue(child(10, "Apple Pay", "Task", Some(9), Some(5)))
        .with_issue(child(11, "Token vault", "Bug", Some(9), Some(5)));
    let out = sync(&tracker, d);
    let feature = out.doc.items.iter().find(|i| i.name == "Wallets").unwrap();
    assert_eq!((feature.kind, feature.parent_id.as_deref()), (Kind::Feature, Some("ep1")));
    // The default size, grown to hold its two members (FLO-FR-44).
    let (size, default) = (feature.size.unwrap(), default_size(Kind::Feature).unwrap());
    assert_eq!(size.width, default.width);
    assert!(size.height > default.height, "{size:?}");
    let task = out.doc.items.iter().find(|i| i.name == "Apple Pay").unwrap();
    assert_eq!((task.kind, task.parent_id.as_deref()), (Kind::Task, Some(feature.id.as_str())));
    let defect = out.doc.items.iter().find(|i| i.name == "Token vault").unwrap();
    assert_eq!(
        (defect.kind, defect.parent_id.as_deref()),
        (Kind::Defect, Some(feature.id.as_str()))
    );
    assert_eq!(out.result.added, 3);
    assert!(tracker.writes().is_empty());
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings());
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
    t1.reference.url = format!("https://{HOST}/{OWNER}/other/issues/77");
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
            url: Some(format!("https://{HOST}/{OWNER}/other/issues/77")),
            reason: SKIP_FOREIGN_REPOSITORY,
            item_id: Some("t1".into()),
        }]
    );
    let read = tracker.calls().into_iter().find(|c| c.starts_with("read_issues")).unwrap();
    assert_eq!(read, "read_issues 2", "the foreign link is not read");
}

/// FTS-FR-WANV: a link to the same owner and repository on another host
/// names another repository, so it is not synced and is skipped.
#[test]
fn the_same_repository_on_another_host_is_foreign() {
    let (tracker, mut d) = plan();
    let t1 = d.sync.as_mut().unwrap().links.iter_mut().find(|l| l.item_id == "t1").unwrap();
    t1.reference.host = "github.example.com".into();
    t1.reference.url = format!("https://github.example.com/{OWNER}/{REPO}/issues/3");
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Changed".into();
    // Issue 3 of this host is not the linked issue.
    tracker.state.borrow_mut().issues.remove(&3);
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert_eq!(out.result.skipped.len(), 1);
    assert_eq!(
        (out.result.skipped[0].reason, out.result.skipped[0].item_id.as_deref()),
        (SKIP_FOREIGN_REPOSITORY, Some("t1"))
    );
    let read = tracker.calls().into_iter().find(|c| c.starts_with("read_issues")).unwrap();
    assert_eq!(read, "read_issues 2", "the link of the other host is not read");
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
    assert_eq!(out.result.skipped[0].url.as_deref(), Some(url_of(12).as_str()));
}

/// FTS-FR-EGJL / FTS-FR-YHVD: a sub-issue that `sync.ignored` holds is not
/// pulled, nor are its own sub-issues, and it is not reported.
#[test]
fn an_ignored_sub_issue_is_not_pulled() {
    let (tracker, d) = plan();
    let tracker = tracker
        .with_issue(child(9, "Card form", "Task", Some(2), Some(5)))
        .with_issue(child(12, "Wallets", "Feature", Some(1), Some(5)))
        .with_issue(child(13, "Apple Pay", "Task", Some(12), Some(5)));
    let mut d = d;
    let ignored = |n: u64| serde_json::from_value(ref_json("issue", n)).unwrap();
    d.sync.as_mut().unwrap().ignored = vec![ignored(9), ignored(12)];
    let out = sync(&tracker, d.clone());
    assert_eq!(out.doc.items.len(), d.items.len());
    assert_eq!(out.result.added, 0);
    assert!(out.result.skipped.is_empty(), "{:?}", out.result.skipped);
    assert!(!out.doc_changed);
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-YHVD / FTS-FR-EGJL: an ignored reference with the
/// number 9 in another repository, or on another host, does not name own
/// issue 9, so issue 9 is still pulled.
#[test]
fn an_ignored_reference_of_another_repository_does_not_stop_a_pull() {
    let (tracker, mut d) = plan();
    let tracker = tracker.with_issue(child(9, "Card form", "Task", Some(2), Some(5)));
    let mut other_repo = ref_json("issue", 9);
    other_repo["repo"] = json!("upstream");
    other_repo["url"] = json!(format!("https://{HOST}/{OWNER}/upstream/issues/9"));
    let mut other_host = ref_json("issue", 9);
    other_host["host"] = json!("github.example.com");
    other_host["url"] = json!(format!("https://github.example.com/{OWNER}/{REPO}/issues/9"));
    d.sync.as_mut().unwrap().ignored = vec![
        serde_json::from_value(other_repo).unwrap(),
        serde_json::from_value(other_host).unwrap(),
    ];
    let out = sync(&tracker, d);
    let pulled = out.doc.items.iter().find(|i| i.name == "Card form").expect("pulled");
    assert_eq!((pulled.kind, pulled.parent_id.as_deref()), (Kind::Task, Some("f1")));
    assert_eq!(link_of(&out.doc, &pulled.id).reference.number, 9);
    assert_eq!(out.result.added, 1);
    assert!(out.result.skipped.is_empty(), "{:?}", out.result.skipped);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert_eq!(out.doc.sync.as_ref().unwrap().ignored.len(), 2);
    assert_valid(&out.doc);
    assert_next_sync_is_quiet(&tracker, out.doc, &settings());
}
