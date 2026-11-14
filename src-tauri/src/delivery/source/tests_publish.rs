//! The tests of a resumed publish (`DSR-delivery-source.md` DSR-FR-ULON,
//! DSR-FR-GXSE): an issue that a partial create made is finished before it
//! counts as done. Against the fake tracker adapter; no test reaches a
//! network.

use serde_json::json;

use super::publish::publish_impl;
use super::*;
use crate::delivery::github::tests::fake::*;
use crate::delivery::machine_state::read_publish_journal;
use crate::github_polling::records::ERR_REQUEST_FAILED;

const FLOW: &str = "plans/resume.flow";

fn file_source(h: &Harness, body: &serde_json::Value) {
    h.write(FLOW, &body.to_string());
    h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: FLOW.into() },
        ..DeliverySettings::default()
    });
}

fn journal(h: &Harness) -> crate::delivery::machine_state::PublishJournal {
    read_publish_journal(&h.handle(), &crate::logging::BUFFER, &h.machine_store(), FLOW).unwrap()
}

fn tracked(h: &Harness, number: u64) -> crate::delivery::github::adapter::TrackerIssue {
    h.fake.lock().issues.iter().find(|i| i.number == number).cloned().unwrap()
}

fn board_items(h: &Harness, number: u64) -> usize {
    h.fake
        .lock()
        .board
        .iter()
        .filter(|(_, _, b)| matches!(b, Board::Issue(n) if *n == number))
        .count()
}

fn on_board(h: &Harness, number: u64) -> bool {
    board_items(h, number) > 0
}

fn url(number: u64) -> String {
    format!("https://github.com/acme/widgets/issues/{number}")
}

/// An epic e1 with a task t1 under it.
fn epic_and_task() -> serde_json::Value {
    json!({ "version": 3, "name": "Plan", "items": [
        { "id": "e1", "kind": "epic", "name": "Epic" },
        { "id": "t1", "kind": "task", "name": "Task", "parentId": "e1" }
    ], "edges": [] })
}

/// DSR-FR-ULON, DSR-FR-GXSE: an issue whose create succeeded but whose add to
/// the Project failed is journalled as not complete. A resumed publish adds
/// it to the Project before it counts as done, so its member is created under
/// it, and no second copy of it is made.
#[test]
fn a_resumed_publish_adds_a_partly_created_issue_to_the_project() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(
        &h,
        &json!({ "version": 3, "name": "Plan", "items": [
            { "id": "t1", "kind": "task", "name": "Task", "parentId": "e1" },
            { "id": "e1", "kind": "epic", "name": "Epic" }
        ], "edges": [] }),
    );
    // The issue of e1 is created; its add to the Project fails.
    h.fake.lock().writes_before_failure = Some(1);
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, crate::delivery::github::ERR_PARTIAL_EDIT);
    assert_eq!(
        failed.created_urls.as_deref(),
        Some(&["https://github.com/acme/widgets/issues/101".to_string()][..])
    );
    let entry = journal(&h).items["e1"].clone();
    assert_eq!((entry.number, entry.complete), (101, false), "journalled, not complete");
    assert!(!on_board(&h, 101));

    h.fake.lock().writes_before_failure = None;
    h.fake.lock().calls.clear();
    let result = publish_impl(&h.handle()).expect("the resumed publish completes");
    assert_eq!(
        h.fake.writes(),
        [
            format!("add_project_item {PROJECT} I_101"),
            "create_issue".to_string(),
            format!("add_project_item {PROJECT} I_102"),
            "set_parent 102 Some(101)".to_string(),
        ],
        "e1 is finished, not made again"
    );
    assert!(on_board(&h, 101));
    assert_eq!(tracked(&h, 102).parent.map(|p| p.number), Some(101));
    assert_eq!((result.milestones, result.issues, result.relations), (0, 1, 0));
    assert_eq!(result.created_urls, ["https://github.com/acme/widgets/issues/102"]);
    assert!(journal(&h).items.is_empty(), "the journal goes when the publish completes");
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::Github);
}

/// DSR-FR-ULON, DSR-FR-GXSE: an issue whose create and add to the Project
/// succeeded but whose parent write failed is journalled as not complete. A
/// resumed publish sets its parent before it counts as done, and its member
/// is created under it.
#[test]
fn a_resumed_publish_sets_the_parent_of_a_partly_created_issue() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(
        &h,
        &json!({ "version": 3, "name": "Plan", "items": [
            { "id": "e1", "kind": "epic", "name": "Epic" },
            { "id": "f1", "kind": "feature", "name": "Feature", "parentId": "e1" },
            { "id": "t1", "kind": "task", "name": "Task", "parentId": "f1" }
        ], "edges": [] }),
    );
    // e1 is created and added; f1 is created and added; its parent fails.
    h.fake.lock().writes_before_failure = Some(4);
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, crate::delivery::github::ERR_PARTIAL_EDIT);
    let urls = failed.created_urls.unwrap();
    assert_eq!(
        urls,
        [
            "https://github.com/acme/widgets/issues/101",
            "https://github.com/acme/widgets/issues/102"
        ]
    );
    let entries = journal(&h).items;
    assert!(entries["e1"].complete);
    assert_eq!((entries["f1"].number, entries["f1"].complete), (102, false));
    assert!(on_board(&h, 102));
    assert_eq!(tracked(&h, 102).parent, None);

    h.fake.lock().writes_before_failure = None;
    h.fake.lock().calls.clear();
    let result = publish_impl(&h.handle()).expect("the resumed publish completes");
    assert_eq!(
        h.fake.writes(),
        [
            "set_parent 102 Some(101)".to_string(),
            "create_issue".to_string(),
            format!("add_project_item {PROJECT} I_103"),
            "set_parent 103 Some(102)".to_string(),
        ]
    );
    assert_eq!(tracked(&h, 102).parent.map(|p| p.number), Some(101));
    assert_eq!(tracked(&h, 103).parent.map(|p| p.number), Some(102));
    assert_eq!((result.issues, result.created_urls.len()), (1, 1));
    assert_eq!(h.fake.lock().issues.len(), 3, "no second copy");
    assert!(journal(&h).items.is_empty());
}

/// DSR-FR-ULON: a resume that fails again while it finishes a partial create
/// keeps the entry not complete, so a later resume finishes it. Its error
/// names the issue that the earlier run created.
#[test]
fn a_failed_completion_stays_not_complete() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(
        &h,
        &json!({ "version": 3, "name": "Plan", "items": [
            { "id": "e1", "kind": "epic", "name": "Epic" }
        ], "edges": [] }),
    );
    h.fake.lock().writes_before_failure = Some(1);
    publish_impl(&h.handle()).unwrap_err();
    h.fake.lock().writes = 0;
    h.fake.lock().writes_before_failure = Some(0);
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, ERR_REQUEST_FAILED, "the first write of the edit failed");
    assert_eq!(failed.created_urls.as_deref(), Some(&[url(101)][..]));
    assert!(!journal(&h).items["e1"].complete);
    h.fake.lock().writes_before_failure = None;
    publish_impl(&h.handle()).unwrap();
    assert!(on_board(&h, 101));
    assert_eq!(h.fake.lock().issues.len(), 1);
}

/// DSR-FR-ULON, DSR-FR-GXSE: the add of a new issue to the Project takes
/// effect on GitHub, but its answer is lost. The resume finds the issue on
/// the Project: it does not refuse with `already_in_plan`, it adds no second
/// Project item, and it creates the member under the issue.
#[test]
fn a_lost_answer_of_the_project_add_is_resumed_without_a_second_add() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &epic_and_task());
    h.fake.lock().lost_answer = Some(format!("add_project_item {PROJECT} I_101"));
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, crate::delivery::github::ERR_PARTIAL_EDIT);
    assert_eq!(failed.created_urls.as_deref(), Some(&[url(101)][..]));
    assert!(!journal(&h).items["e1"].complete);
    assert_eq!(board_items(&h, 101), 1, "the add took effect");

    h.fake.lock().calls.clear();
    let result = publish_impl(&h.handle()).expect("the resume completes");
    assert_eq!(
        h.fake.writes(),
        [
            "create_issue".to_string(),
            format!("add_project_item {PROJECT} I_102"),
            "set_parent 102 Some(101)".to_string(),
        ],
        "e1 needs no write: it is on the Project and has no container"
    );
    assert_eq!(board_items(&h, 101), 1, "no second Project item");
    assert_eq!(tracked(&h, 102).parent.map(|p| p.number), Some(101));
    assert_eq!(result.created_urls, [url(102)]);
    assert!(journal(&h).items.is_empty());
}

/// DSR-FR-ULON, GDP-FR-GBDF: a partly created issue that is edited on GitHub
/// between two publishes. The resume reads the Project before it finishes the
/// issue, so the `move` edit names the `updated_at` of that read and is not
/// refused with `changed_on_tracker`: the first resume completes. The edit
/// made on GitHub stays.
#[test]
fn a_partly_created_issue_edited_on_github_is_finished_by_the_resume() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(
        &h,
        &json!({ "version": 3, "name": "Plan", "items": [
            { "id": "e1", "kind": "epic", "name": "Epic" },
            { "id": "f1", "kind": "feature", "name": "Feature", "parentId": "e1" }
        ], "edges": [] }),
    );
    // e1 is created and added; f1 is created and added; its parent fails.
    h.fake.lock().writes_before_failure = Some(4);
    publish_impl(&h.handle()).unwrap_err();
    let before = tracked(&h, 102).updated_at;
    {
        let mut s = h.fake.lock();
        let edited = s.issues.iter_mut().find(|i| i.number == 102).unwrap();
        edited.title = "Renamed on GitHub".into();
        edited.updated_at = "2026-07-01T00:00:00Z".into();
    }
    assert_ne!(before, tracked(&h, 102).updated_at);

    h.fake.lock().writes_before_failure = None;
    h.fake.lock().calls.clear();
    publish_impl(&h.handle()).expect("the first resume completes");
    assert_eq!(h.fake.writes(), ["set_parent 102 Some(101)"]);
    assert_eq!(tracked(&h, 102).parent.map(|p| p.number), Some(101));
    assert_eq!(tracked(&h, 102).title, "Renamed on GitHub");
    assert!(journal(&h).items.is_empty());
}

/// DSR-FR-ULON: a partly created issue that is deleted on GitHub between two
/// publishes cannot be finished. The resume drops its journal entry and
/// creates the item again, and its member under the new issue. This holds
/// for an issue that was not on the Project and for one that was.
#[test]
fn a_partly_created_issue_deleted_on_github_is_created_again() {
    // 1: the add of e1 to the Project failed, so e1 is not on the Project.
    // 4: t1 is on the Project, and its parent failed.
    for (failed_after, gone, item) in [(1, 101, "e1"), (4, 102, "t1")] {
        let h = Harness::new();
        h.select_project(Some(PROJECT));
        file_source(&h, &epic_and_task());
        h.fake.lock().writes_before_failure = Some(failed_after);
        publish_impl(&h.handle()).unwrap_err();
        assert_eq!(
            (journal(&h).items[item].number, journal(&h).items[item].complete),
            (gone, false)
        );
        // Deleted on GitHub: the issue is gone, and its Project item reads
        // as an item with no issue.
        assert_eq!(on_board(&h, gone), gone == 102);
        h.fake.lock().issues.retain(|i| i.number != gone);

        h.fake.lock().writes_before_failure = None;
        h.fake.lock().calls.clear();
        let result = publish_impl(&h.handle()).expect("the resume completes");
        let numbers: Vec<u64> = h.fake.lock().issues.iter().map(|i| i.number).collect();
        match gone {
            101 => {
                assert_eq!(numbers, [102, 103], "e1 and t1 are created again");
                assert_eq!(tracked(&h, 103).parent.map(|p| p.number), Some(102));
                assert_eq!(result.created_urls, [url(102), url(103)]);
            }
            _ => {
                assert_eq!(numbers, [101, 103], "t1 is created again");
                assert_eq!(tracked(&h, 103).parent.map(|p| p.number), Some(101));
                assert_eq!(result.created_urls, [url(103)]);
            }
        }
        assert!(on_board(&h, 103));
        assert!(journal(&h).items.is_empty(), "the journal goes");
        assert_eq!(h.delivery_settings().source, DeliverySourceChoice::Github);
    }
}

/// DSR-FR-ULON, DSR-FR-GXSE: a partly created issue inside a milestone keeps
/// its milestone after the resume, where its add to the Project failed and
/// where its parent failed. The resume writes no milestone.
#[test]
fn a_partly_created_issue_in_a_milestone_keeps_its_milestone() {
    // 2: the add of e1 failed. 5: the parent of f1 failed.
    for failed_after in [2, 5] {
        let h = Harness::new();
        h.select_project(Some(PROJECT));
        file_source(
            &h,
            &json!({ "version": 3, "name": "Plan", "items": [
                { "id": "m1", "kind": "milestone", "name": "Beta" },
                { "id": "e1", "kind": "epic", "name": "Epic", "parentId": "m1" },
                { "id": "f1", "kind": "feature", "name": "Feature", "parentId": "e1" }
            ], "edges": [] }),
        );
        h.fake.lock().writes_before_failure = Some(failed_after);
        let failed = publish_impl(&h.handle()).unwrap_err();
        assert_eq!(failed.code, crate::delivery::github::ERR_PARTIAL_EDIT, "{failed_after}");

        h.fake.lock().writes_before_failure = None;
        h.fake.lock().calls.clear();
        publish_impl(&h.handle()).expect("the resume completes");
        assert!(
            h.fake.writes().iter().all(|w| !w.starts_with("set_milestone")),
            "{failed_after}: {:?}",
            h.fake.writes()
        );
        assert_eq!(tracked(&h, 101).milestone, Some(1), "{failed_after}");
        assert_eq!(tracked(&h, 102).milestone, Some(1), "{failed_after}");
        assert_eq!(tracked(&h, 102).parent.map(|p| p.number), Some(101), "{failed_after}");
        assert!(on_board(&h, 101) && on_board(&h, 102));
        assert_eq!(h.fake.lock().issues.len(), 2, "no second copy");
    }
}

/// Three tasks: t1 blocks t2, and t2 blocks t3.
fn chain() -> serde_json::Value {
    json!({ "version": 3, "name": "Plan", "items": [
        { "id": "t1", "kind": "task", "name": "One" },
        { "id": "t2", "kind": "task", "name": "Two" },
        { "id": "t3", "kind": "task", "name": "Three" }
    ], "edges": [
        { "id": "b1", "from": "t1", "to": "t2", "type": "blocks" },
        { "id": "b2", "from": "t2", "to": "t3", "type": "blocks" }
    ] })
}

/// DSR-FR-ULON: the journal is written after each step, before the next
/// request, and not only on an error. At each write the fake reads the
/// journal from the store: each create finds every earlier item, and each
/// relation finds every item and every earlier relation.
#[test]
fn the_journal_is_written_before_each_next_request() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &chain());
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let (seen, app, store) = (seen.clone(), h.handle(), h.machine_store());
        h.fake.lock().on_write = Some(Box::new(move |call: &str| {
            let j = read_publish_journal(&app, &crate::logging::BUFFER, &store, FLOW).unwrap();
            let complete = j.items.values().filter(|e| e.complete).count();
            seen.lock().unwrap().push((call.to_string(), complete, j.edges.len()));
        }));
    }
    publish_impl(&h.handle()).unwrap();
    let seen = seen.lock().unwrap().clone();
    let creates: Vec<_> =
        seen.iter().filter(|(c, _, _)| c == "create_issue").map(|(_, n, _)| *n).collect();
    assert_eq!(creates, [0, 1, 2], "each create finds every earlier item journalled");
    let relations: Vec<_> = seen
        .iter()
        .filter(|(c, _, _)| c.starts_with("add_blocked_by"))
        .map(|(_, n, e)| (*n, *e))
        .collect();
    assert_eq!(relations, [(3, 0), (3, 1)], "each relation finds the earlier ones journalled");
}

/// DSR-FR-ULON, DSR-FR-GXSE: a publish that fails between two `blocks`
/// relations keeps the first in the journal. The resume adds only the
/// second; no item and no relation is made twice.
#[test]
fn a_failure_between_two_relations_resumes_with_the_second() {
    // The writes of a whole publish, to find the second relation.
    let probe = Harness::new();
    probe.select_project(Some(PROJECT));
    file_source(&probe, &chain());
    publish_impl(&probe.handle()).unwrap();
    let writes = probe.fake.writes();
    let second = writes.iter().enumerate().filter(|(_, c)| c.starts_with("add_blocked_by")).nth(1);
    let (at, _) = second.expect("two relations");

    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &chain());
    h.fake.lock().writes_before_failure = Some(at);
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, ERR_REQUEST_FAILED);
    assert_eq!(failed.created_urls.as_deref(), Some(&[url(101), url(102), url(103)][..]));
    let kept = journal(&h);
    assert_eq!(kept.items.len(), 3);
    assert!(kept.items.values().all(|e| e.complete));
    assert_eq!(kept.edges.keys().collect::<Vec<_>>(), ["b1"], "the first relation is journalled");
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::File { flow_id: FLOW.into() });

    h.fake.lock().writes_before_failure = None;
    h.fake.lock().calls.clear();
    let result = publish_impl(&h.handle()).expect("the resumed publish completes");
    assert_eq!(h.fake.writes(), ["add_blocked_by 103 102"], "only the second relation");
    assert_eq!((result.issues, result.relations), (0, 1));
    let blockers = |n: u64| tracked(&h, n).blocked_by.iter().map(|b| b.number).collect::<Vec<_>>();
    assert_eq!((blockers(102), blockers(103)), (vec![101], vec![102]));
    assert_eq!(h.fake.lock().issues.len(), 3, "no item is made twice");
    assert!(journal(&h).items.is_empty(), "the journal goes when the publish completes");
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::Github);
}

/// Three milestones of a Flow in the document order late, undated, early.
fn three_milestones() -> serde_json::Value {
    json!({ "version": 3, "name": "Plan", "items": [
        { "id": "late", "kind": "milestone", "name": "Late", "due": "2027-03-01" },
        { "id": "open", "kind": "milestone", "name": "Open" },
        { "id": "early", "kind": "milestone", "name": "Early", "due": "2027-01-01" }
    ], "edges": [] })
}

/// DSR-FR-PTIT, DMS-FR-WAUS: a publish writes the file's milestone order, the
/// document order of its milestone items, into the overlay under the ids of
/// the created milestones.
#[test]
fn a_publish_writes_the_milestone_order_of_the_file() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &three_milestones());
    publish_impl(&h.handle()).unwrap();
    let overlay = crate::delivery::machine_state::read_delivery_overlay(
        &h.handle(),
        &crate::logging::BUFFER,
        &h.machine_store(),
        PROJECT,
    )
    .unwrap();
    let numbers: Vec<u64> = ["Late", "Open", "Early"]
        .iter()
        .map(|t| h.fake.lock().milestones.iter().find(|m| m.title == *t).unwrap().number)
        .collect();
    let expected: Vec<String> = numbers.iter().map(|n| format!("milestone-{n}")).collect();
    assert_eq!(overlay.milestone_order, expected);
}

/// DSR-FR-FQZD, DSR-FR-BIYW, DSR-FR-RZUH, FLO-FR-MCFR: the milestone order of
/// a GitHub plan starts from the order of GDP-FR-HNEW; the dated milestones
/// take the dated places by due date. The milestones answer and an export
/// both follow it, and `current` is the first in that order.
#[test]
fn a_github_plan_answers_and_exports_its_milestones_in_milestone_order() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &three_milestones());
    publish_impl(&h.handle()).unwrap();
    let id = |t: &str| {
        let n = h.fake.lock().milestones.iter().find(|m| m.title == t).unwrap().number;
        format!("milestone-{n}")
    };
    let expected = [id("Early"), id("Open"), id("Late")];
    let answer = super::milestones::milestones_impl(&h.handle(), true).unwrap();
    let ids: Vec<String> = answer.milestones.iter().map(|m| m.item_id.clone()).collect();
    assert_eq!(ids, expected, "the dated places by due date; the undated keeps its place");
    assert_eq!(
        answer
            .milestones
            .iter()
            .filter(|m| m.current)
            .map(|m| m.item_id.clone())
            .collect::<Vec<_>>(),
        [id("Early")]
    );

    super::export::export_impl(&h.handle(), "plans/back.flow").unwrap();
    let body = h.read("plans/back.flow");
    let doc = crate::flow_validation::model::parse_valid(&body).unwrap();
    let exported: Vec<&str> = doc
        .items
        .iter()
        .filter(|i| i.kind == crate::flow_validation::model::Kind::Milestone)
        .map(|i| i.id.as_str())
        .collect();
    assert_eq!(exported, expected, "the file holds the milestone items in milestone order");
}
