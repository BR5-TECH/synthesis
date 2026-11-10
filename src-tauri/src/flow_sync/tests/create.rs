//! FTS-FR-HDRR, FTS-FR-DNBY, FTS-FR-XITX, FTS-FR-VFNM, FTS-FR-AIDI,
//! FTS-FR-LTMW: creations, the order of the writes, and failures.

use super::*;
use crate::flow_validation::model::RefKind;

fn add(d: &mut FlowDocument, v: Value) {
    d.items.push(serde_json::from_value(v).unwrap());
}

/// FTS-FR-HDRR: an item with no link gets an issue with its name,
/// description, Type, milestone, and parent, and its link stores the created
/// values as the base.
#[test]
fn an_unlinked_item_gets_an_issue() {
    let (tracker, mut d) = plan();
    let mut t9 = item("t9", "task", "Card form", Some("f1"));
    t9["description"] = json!("Use the SDK.");
    add(&mut d, t9);
    let out = sync(&tracker, d);
    assert_eq!(
        tracker.writes(),
        vec!["create_issue 100 type=Task milestone=5", "set_parent 100 none->2"]
    );
    let created = tracker.issue(100);
    assert_eq!(created.title, "Card form");
    assert_eq!(created.body, "Use the SDK.");
    assert_eq!(created.issue_type.as_deref(), Some("Task"));
    assert_eq!(created.milestone, Some(5));
    assert_eq!(created.parent.map(|k| k.number), Some(2));
    let l = link_of(&out.doc, "t9");
    assert_eq!((l.reference.kind, l.reference.number), (RefKind::Issue, 100));
    assert_eq!(l.reference.url, url_of(100));
    assert_eq!(l.base.name, "Card form");
    assert_eq!(l.base.description, "Use the SDK.");
    assert_eq!(l.base.r#type, Some(Some("Task".into())));
    assert_eq!(l.base.milestone, Some(Some(5)));
    assert_eq!(l.base.parent.clone().flatten().map(|p| p.number), Some(2));
    assert_eq!(l.base.state, "open");
    assert_eq!(out.result.created, 1);
    assert_eq!(out.created_urls, vec![url_of(100)]);
    assert_valid(&out.doc);
    // The next sync finds both sides equal.
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!again.doc_changed);
}

/// FTS-FR-HDRR: items are created parents first, so a new task names the
/// issue of its new feature as its parent.
#[test]
fn items_are_created_parents_first() {
    let (tracker, mut d) = plan();
    // The task comes first in the document; its feature is still created
    // first.
    add(&mut d, item("t9", "task", "Card form", Some("f9")));
    add(&mut d, item("f9", "feature", "Checkout", Some("ep1")));
    let out = sync(&tracker, d);
    assert_eq!(
        tracker.writes(),
        vec![
            "create_issue 100 type=Feature milestone=5",
            "set_parent 100 none->1",
            "create_issue 101 type=Task milestone=5",
            "set_parent 101 none->100",
        ]
    );
    assert_eq!(link_of(&out.doc, "f9").reference.number, 100);
    assert_eq!(tracker.issue(101).parent.map(|k| k.number), Some(100));
    assert_eq!(tracker.issue(100).parent.map(|k| k.number), Some(1));
    assert_eq!(out.result.created, 2);
}

/// FTS-FR-HDRR: a milestone item gets a milestone with its name, description,
/// and due date.
#[test]
fn a_milestone_item_gets_a_milestone() {
    let (tracker, mut d) = plan();
    let mut m9 = item("m9", "milestone", "GA", None);
    m9["due"] = json!("2027-09-30");
    add(&mut d, m9);
    let out = sync(&tracker, d);
    assert_eq!(tracker.writes(), vec!["create_milestone 100"]);
    let m = tracker.state.borrow().milestones[&100].clone();
    assert_eq!((m.title.as_str(), m.due.as_deref()), ("GA", Some("2027-09-30")));
    let l = link_of(&out.doc, "m9");
    assert_eq!(l.reference.kind, RefKind::Milestone);
    assert_eq!(l.base.due, Some(Some("2027-09-30".into())));
    assert_eq!(l.base.r#type, None);
    assert_valid(&out.doc);
}

/// FTS-FR-HDRR: a `localOnly` item is never created on the tracker.
#[test]
fn a_local_only_item_is_not_created() {
    let (tracker, mut d) = plan();
    let mut t9 = item("t9", "task", "Spike", Some("f1"));
    t9["localOnly"] = json!(true);
    add(&mut d, t9);
    let out = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert!(!has_link(&out.doc, "t9"));
    assert_eq!(out.result.created, 0);
}

/// FTS-FR-DNBY: after the creations, the blocked-by relations of the created
/// items are added, in both directions.
#[test]
fn the_relations_of_created_items_are_added_after_the_creations() {
    let (tracker, mut d) = plan();
    add(&mut d, item("t8", "task", "Card form", Some("f1")));
    add(&mut d, item("t9", "task", "Receipt", Some("f1")));
    d.edges.push(serde_json::from_value(blocks("e1", "t1", "t8")).unwrap());
    d.edges.push(serde_json::from_value(blocks("e2", "t9", "t1")).unwrap());
    let out = sync(&tracker, d);
    let writes = tracker.writes();
    let first_relation = writes.iter().position(|w| w.starts_with("add_blocked_by")).unwrap();
    let last_create = writes.iter().rposition(|w| w.starts_with("create")).unwrap();
    assert!(first_relation > last_create, "{writes:?}");
    assert_eq!(tracker.issue(100).blocked_by.iter().map(|k| k.number).collect::<Vec<_>>(), vec![3]);
    assert_eq!(tracker.issue(3).blocked_by.iter().map(|k| k.number).collect::<Vec<_>>(), vec![101]);
    let t8 = link_of(&out.doc, "t8").base.blocked_by.clone().unwrap();
    assert_eq!(t8.iter().map(|r| r.number).collect::<Vec<_>>(), vec![3]);
    let t1 = link_of(&out.doc, "t1").base.blocked_by.clone().unwrap();
    assert_eq!(t1.iter().map(|r| r.number).collect::<Vec<_>>(), vec![101]);
    assert_eq!(out.result.updated, 1, "t1 got a relation");
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!again.doc_changed);
}

/// FTS-FR-XITX: an issue whose Type the repository does not offer is not
/// created, is reported as skipped, and the sync goes on with the others.
#[test]
fn an_item_whose_type_is_not_offered_is_skipped() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().types.retain(|t| t != "Bug");
    add(&mut d, item("x9", "defect", "Wrong decline text", Some("f1")));
    add(&mut d, item("t9", "task", "Card form", Some("f1")));
    let out = sync(&tracker, d);
    assert_eq!(out.result.created, 1);
    assert!(!has_link(&out.doc, "x9"));
    assert!(has_link(&out.doc, "t9"));
    assert_eq!(
        out.result.skipped,
        vec![SkippedIssue {
            number: None,
            title: "Wrong decline text".into(),
            url: None,
            reason: SKIP_TYPE_UNAVAILABLE,
            item_id: Some("x9".into()),
        }]
    );
}

/// FTS-FR-XITX: a Type change to a Type the repository does not offer is not
/// written, the issue is reported, and the base keeps the old Type.
#[test]
fn a_type_change_the_repository_does_not_offer_is_skipped() {
    let (tracker, d) = plan();
    let settings = FlowTrackerSettings { task_type: "Chore".into(), ..settings() };
    let out = run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap();
    assert!(tracker.writes().is_empty());
    assert_eq!(out.result.skipped[0].number, Some(3));
    assert_eq!(out.result.skipped[0].url.as_deref(), Some(url_of(3).as_str()));
    assert_eq!(out.result.skipped[0].item_id.as_deref(), Some("t1"));
    assert_eq!(out.result.skipped[0].reason, SKIP_TYPE_UNAVAILABLE);
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Task".into())));
}

/// FTS-FR-VFNM / FTS-FR-AIDI: a write that fails stops the writes after it.
/// The outcome keeps the link of each created item and the URLs, and the
/// base of the failed write stays as it was, so the next sync completes the
/// work without a conflict.
#[test]
fn a_failed_write_stops_the_writes_and_the_next_sync_completes_them() {
    let (tracker, mut d) = plan();
    let tracker = tracker.failing("set_parent", 0);
    add(&mut d, item("t9", "task", "Card form", Some("f1")));
    add(&mut d, item("t10", "task", "Receipt", Some("f1")));
    let out = sync(&tracker, d);
    assert_eq!(out.write_error.as_deref(), Some(ERR_GITHUB_UNREACHABLE));
    assert_eq!(out.created_urls, vec![url_of(100)]);
    assert!(has_link(&out.doc, "t9"));
    assert!(!has_link(&out.doc, "t10"), "no write after the failure");
    assert_eq!(link_of(&out.doc, "t9").base.parent, Some(None));

    tracker.state.borrow_mut().fail = None;
    tracker.clear_calls();
    let next = sync(&tracker, out.doc);
    assert!(next.write_error.is_none());
    assert!(next.result.conflicts.is_empty());
    assert_eq!(tracker.issue(100).parent.map(|k| k.number), Some(2));
    assert_eq!(next.result.created, 1, "t10 is created now");
    assert_eq!(tracker.state.borrow().issues.len(), 5, "no issue is created twice");
}

/// FTS-FR-AIDI: when the Flow write fails after the tracker writes, the bases
/// stay as they were, and the next sync finds the written fields equal on
/// both sides and only updates their bases.
#[test]
fn after_a_lost_flow_write_the_next_sync_only_updates_bases() {
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    let lost = sync(&tracker, d.clone());
    assert_eq!(tracker.issue(3).title, "Tokenize cards");
    drop(lost);
    tracker.clear_calls();
    let next = sync(&tracker, d);
    assert!(tracker.writes().is_empty());
    assert!(next.result.conflicts.is_empty());
    assert_eq!(link_of(&next.doc, "t1").base.name, "Tokenize cards");
}

/// FTS-FR-AIDI: the `write_failed` error carries the created URLs.
#[test]
fn write_failed_carries_the_created_urls() {
    let error = FlowSyncError::of(ERR_WRITE_FAILED).with_created(vec![url_of(100)]);
    let v = serde_json::to_value(&error).unwrap();
    assert_eq!(v["code"], json!("write_failed"));
    assert_eq!(v["createdUrls"], json!([url_of(100)]));
    assert!(!v["message"].as_str().unwrap().is_empty());
}

/// FTS-FR-LTMW / FTS-FR-ZLFX: a read that fails returns its code, and no
/// write is made.
#[test]
fn a_failed_read_returns_its_code_and_writes_nothing() {
    let (tracker, mut d) = plan();
    let tracker = tracker.failing("read_issues", 0);
    add(&mut d, item("t9", "task", "Card form", Some("f1")));
    let settings = settings();
    let error = run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap_err();
    assert_eq!(error.code, ERR_GITHUB_UNREACHABLE);
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-LTMW: a Flow with no sync section is refused with
/// `sync_not_enabled`, and no tracker request is made.
#[test]
fn a_flow_without_a_sync_section_is_refused() {
    let tracker = FakeTracker::new();
    let mut d = plan().1;
    d.sync = None;
    let settings = settings();
    let error = run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap_err();
    assert_eq!(error.code, ERR_SYNC_NOT_ENABLED);
    assert!(tracker.calls().is_empty());
}

/// A plan of `count` epics in sync, each with its own issue, in milestone
/// `m0` (milestone 1000).
fn many_epics(count: u64) -> (FakeTracker, FlowDocument) {
    let tracker = FakeTracker::new().with_milestone(milestone(1000, "Beta"));
    let mut items = vec![item("m0", "milestone", "Beta", None)];
    let mut links = vec![link("m0", "milestone", 1000, milestone_base("Beta", None))];
    for n in 1..=count {
        let id = format!("ep{n}");
        let name = format!("Plan {n}");
        items.push(item(&id, "epic", &name, Some("m0")));
        links.push(link(&id, "issue", n, issue_base(&name, Some("Epic"), None, Some(1000), &[])));
        tracker.state.borrow_mut().issues.insert(n, child(n, &name, "Epic", None, Some(1000)));
    }
    (tracker, doc(items, vec![], links))
}

/// FTS NFR: the reads are made in batches, so many linked items need few
/// requests: 121 issues in batches of 50.
#[test]
fn linked_items_are_read_in_batches() {
    let (tracker, d) = many_epics(121);
    let out = sync(&tracker, d);
    let reads: Vec<String> =
        tracker.calls().into_iter().filter(|c| c.starts_with("read_issues")).collect();
    assert_eq!(reads, vec!["read_issues 50", "read_issues 50", "read_issues 21"]);
    let milestones: Vec<String> =
        tracker.calls().into_iter().filter(|c| c.starts_with("read_milestones")).collect();
    assert_eq!(milestones, vec!["read_milestones 1"]);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(!out.doc_changed);
}

/// FTS-FR-LTMW / PST-FR-28: a tracker value the plan cannot hold as a Flow
/// value refuses the call with `flow_invalid` before any tracker write.
#[test]
fn a_merge_that_would_make_an_invalid_plan_is_refused_before_any_write() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().milestones.get_mut(&5).unwrap().due = Some("2027-13-45".into());
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    let settings = settings();
    let error = run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap_err();
    assert_eq!(error.code, ERR_FLOW_INVALID);
    assert!(error.report.unwrap().violations.iter().any(|v| v.code == "invalid_due_date"));
    assert!(tracker.writes().is_empty());
}

/// FTS-FR-XITX / FTS-FR-HDRR: a new issue takes the Type in the spelling the
/// repository offers.
#[test]
fn a_new_issue_takes_the_repository_spelling_of_its_type() {
    let (tracker, mut d) = plan();
    add(&mut d, item("t9", "task", "Card form", Some("f1")));
    let settings = FlowTrackerSettings { task_type: "task".into(), ..settings() };
    let out = run(
        &tracker,
        SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
    )
    .unwrap();
    assert_eq!(tracker.issue(100).issue_type.as_deref(), Some("Task"));
    assert_eq!(link_of(&out.doc, "t9").base.r#type, Some(Some("Task".into())));
}

/// FTS-FR-XITX: the Types are read once per sync where the sync merges or
/// creates an issue, and not at all for a plan of milestones alone.
#[test]
fn the_types_are_read_once_where_an_issue_is_merged_or_created() {
    let types = |tracker: &FakeTracker| {
        tracker.calls().iter().filter(|c| c.starts_with("list_issue_types")).count()
    };
    let (tracker, mut d) = plan();
    d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
    add(&mut d, item("t8", "task", "One", Some("f1")));
    add(&mut d, item("t9", "task", "Two", Some("f1")));
    sync(&tracker, d);
    assert_eq!(types(&tracker), 1);

    let tracker = FakeTracker::new().with_milestone(milestone(5, "Beta"));
    let mut d = doc(
        vec![item("m1", "milestone", "Beta", None)],
        vec![],
        vec![link("m1", "milestone", 5, milestone_base("Beta", None))],
    );
    add(&mut d, item("m9", "milestone", "GA", None));
    sync(&tracker, d);
    assert_eq!(types(&tracker), 0);
    assert_eq!(tracker.writes(), vec!["create_milestone 100"]);
}

/// FTS NFR: the sub-issues are read in batches too: 25 parents in batches
/// of 10.
#[test]
fn sub_issues_are_read_in_batches() {
    let (tracker, d) = many_epics(25);
    sync(&tracker, d);
    let reads: Vec<String> =
        tracker.calls().into_iter().filter(|c| c.starts_with("read_sub_issues")).collect();
    assert_eq!(reads, vec!["read_sub_issues 10", "read_sub_issues 10", "read_sub_issues 5"]);
}

/// FTS-FR-HDRR: the creations of a plan of several levels come parents
/// first, milestones included, and the URLs are in creation order.
#[test]
fn a_new_milestone_is_created_before_its_members() {
    let (tracker, mut d) = plan();
    add(&mut d, item("x9", "defect", "Decline text", Some("m9")));
    add(&mut d, item("m9", "milestone", "GA", None));
    let out = sync(&tracker, d);
    // A defect directly in a milestone has no parent issue (FTS-FR-HLVJ).
    assert_eq!(
        tracker.writes(),
        vec!["create_milestone 100", "create_issue 101 type=Bug milestone=100"]
    );
    assert_eq!(tracker.issue(101).milestone, Some(100));
    assert_eq!(tracker.issue(101).parent, None);
    assert_eq!(link_of(&out.doc, "x9").base.parent, Some(None));
    assert_eq!(out.created_urls, vec![milestone_url(100), url_of(101)]);
}

/// FTS-FR-XITX: a repository that offers no issue Type at all gets plain
/// issues. A new issue has no Type and a base `type` of `null`, a linked
/// issue's `type` is neither written nor compared, and the next sync finds
/// nothing to do.
#[test]
fn a_repository_without_types_gets_plain_issues() {
    let (tracker, mut d) = plan();
    tracker.state.borrow_mut().types.clear();
    for n in 1..=3 {
        tracker.edit(n, |i| i.issue_type = None);
    }
    add(&mut d, item("t9", "task", "Card form", Some("f1")));
    let out = sync(&tracker, d);
    assert_eq!(
        tracker.writes(),
        vec!["create_issue 100 type=none milestone=5", "set_parent 100 none->2"]
    );
    assert_eq!(tracker.issue(100).issue_type, None);
    assert_eq!(link_of(&out.doc, "t9").base.r#type, Some(None));
    // The bases of the linked issues keep the Type they had.
    assert_eq!(link_of(&out.doc, "t1").base.r#type, Some(Some("Task".into())));
    assert!(out.result.conflicts.is_empty(), "{:?}", out.result.conflicts);
    assert!(out.result.skipped.is_empty(), "{:?}", out.result.skipped);
    assert_eq!(find(&out.doc, "t1").kind, crate::flow_validation::model::Kind::Task);
    assert_valid(&out.doc);
    tracker.clear_calls();
    let again = sync(&tracker, out.doc);
    assert!(tracker.writes().is_empty(), "{:?}", tracker.writes());
    assert!(again.result.conflicts.is_empty());
    assert!(!again.doc_changed);
}

/// FTS-FR-LTMW: a read of the milestones, of the sub-issues, or of the
/// Types that fails returns its code, and no write is made.
#[test]
fn a_failed_read_of_any_kind_returns_its_code_and_writes_nothing() {
    for operation in ["read_milestones", "read_sub_issues", "list_issue_types"] {
        let (tracker, mut d) = plan();
        let tracker = tracker.failing(operation, 0);
        d.items.iter_mut().find(|i| i.id == "t1").unwrap().name = "Tokenize cards".into();
        add(&mut d, item("t9", "task", "Card form", Some("f1")));
        let settings = settings();
        let error = run(
            &tracker,
            SyncRequest { flow_id: FLOW_ID, doc: d, settings: &settings, resolutions: &[] },
        )
        .unwrap_err();
        assert_eq!(error.code, ERR_GITHUB_UNREACHABLE, "{operation}");
        assert!(error.created_urls.is_empty(), "{operation}");
        let writes: Vec<String> =
            tracker.writes().into_iter().filter(|w| !w.contains("FAILED")).collect();
        assert!(writes.is_empty(), "{operation}: {writes:?}");
    }
}
