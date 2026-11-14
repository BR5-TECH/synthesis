//! The tests of the edits of a GitHub delivery plan
//! (`GDP-github-delivery-plan.md` GDP-FR-VNNU to GDP-FR-IRDD), against a fake
//! tracker adapter in a mock application.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use super::super::edits::{apply_edit_impl, apply_for_target, GithubDeliveryEdit as E, RemoveMode};
use super::super::*;
use super::fake::*;
use crate::delivery::settings::{DeliverySettings, MappedKind};
use crate::flow_validation::model::{EdgeType, Kind};

fn edit(h: &Harness, e: E) -> Result<GithubDeliveryPlan, DeliveryError> {
    apply_edit_impl(&h.handle(), e)
}

/// The edit, with the id of a new item.
fn create(h: &Harness, e: E) -> Result<(GithubDeliveryPlan, Option<String>), DeliveryError> {
    let app = h.handle();
    let target = delivery_target(&app)?;
    apply_for_target(&app, &target, &e)
}

/// A loaded plan, so the read cache exists.
fn loaded() -> (Harness, GithubDeliveryPlan) {
    let h = Harness::github_plan();
    let plan = load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    (h, plan)
}

fn stamp(plan: &GithubDeliveryPlan, id: &str) -> String {
    plan.tracker.iter().find(|t| t.item_id == id).unwrap().updated_at.clone()
}

fn item<'a>(plan: &'a GithubDeliveryPlan, id: &str) -> &'a crate::flow_validation::model::Item {
    plan.document.items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no item {id}"))
}

/// GDP-FR-VNNU, GDP-FR-DTPB, GDP-FR-GBDF: `rename` reads the item, writes the
/// issue title, reads the item again, updates the read cache, and returns the
/// plan with the new name.
#[test]
fn rename_writes_the_title_and_updates_the_cache() {
    let (h, plan) = loaded();
    let events = listen(&h.handle(), GITHUB_DELIVERY_PLAN_CHANGED);
    let after = edit(
        &h,
        E::Rename {
            item_id: "issue-3".into(),
            name: "New".into(),
            updated_at: stamp(&plan, "issue-3"),
        },
    )
    .unwrap();
    assert_eq!(h.fake.calls(), ["read_issue 3", "update_issue 3", "read_issue 3"]);
    assert_eq!(item(&after, "issue-3").name, "New");
    assert_eq!(h.cache().unwrap().issue(3).unwrap().title, "New");
    assert_ne!(stamp(&after, "issue-3"), stamp(&plan, "issue-3"));
    assert_eq!(events.lock().unwrap().len(), 1, "the cache change is announced");
}

/// GDP-FR-GBDF: an item whose `updated_at` differs from the edit's is
/// `changed_on_tracker`; nothing is written, and the cache takes the values
/// read.
#[test]
fn a_changed_item_is_refused_and_the_cache_takes_it() {
    let (h, plan) = loaded();
    {
        let mut s = h.fake.lock();
        let i = s.issues.iter_mut().find(|i| i.number == 3).unwrap();
        i.title = "Changed on GitHub".into();
        i.updated_at = "2026-09-09T09:09:09Z".into();
    }
    let refused = edit(
        &h,
        E::SetDescription {
            item_id: "issue-3".into(),
            description: "x".into(),
            updated_at: stamp(&plan, "issue-3"),
        },
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_CHANGED_ON_TRACKER);
    assert!(h.fake.writes().is_empty(), "nothing is written");
    let cache = h.cache().unwrap();
    assert_eq!(cache.issue(3).unwrap().title, "Changed on GitHub");
    assert_eq!(cache.issue(3).unwrap().updated_at, "2026-09-09T09:09:09Z");
}

/// GDP-FR-UDOL: `create_item` of a milestone creates a repository milestone
/// and returns its id.
#[test]
fn create_item_makes_a_milestone() {
    let (h, _) = loaded();
    let (plan, id) = create(
        &h,
        E::CreateItem {
            item_kind: Kind::Milestone,
            name: "Beta".into(),
            container_id: None,
            description: Some("Second".into()),
            due: Some("2027-01-01".into()),
        },
    )
    .unwrap();
    let id = id.unwrap();
    assert_eq!(id, "milestone-2");
    let m = item(&plan, &id);
    assert_eq!((m.name.as_str(), m.due.as_deref()), ("Beta", Some("2027-01-01")));
    assert_eq!(h.fake.writes(), ["create_milestone"]);
    // A milestone sits at the top level alone.
    let refused = create(
        &h,
        E::CreateItem {
            item_kind: Kind::Milestone,
            name: "x".into(),
            container_id: Some("issue-1".into()),
            description: None,
            due: None,
        },
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_KIND_CHANGE_UNSUPPORTED);
}

/// GDP-FR-UDOL, GDP-FR-NJJZ: `create_item` of another kind creates an issue
/// with the create Type of its kind, its milestone from the nearest milestone
/// container, adds it to the Project, and sets its parent to the nearest issue
/// container.
#[test]
fn create_item_makes_an_issue_in_its_containers() {
    let (h, _) = loaded();
    let (plan, id) = create(
        &h,
        E::CreateItem {
            item_kind: Kind::Defect,
            name: "Crash".into(),
            container_id: Some("issue-2".into()),
            description: None,
            due: None,
        },
    )
    .unwrap();
    let id = id.unwrap();
    assert_eq!(id, "issue-101");
    assert_eq!(
        h.fake.writes(),
        ["create_issue", &format!("add_project_item {PROJECT} I_101"), "set_parent 101 Some(2)"]
    );
    let created = h.fake.lock().issues.iter().find(|i| i.number == 101).cloned().unwrap();
    assert_eq!(created.issue_type.as_deref(), Some("Bug"));
    assert_eq!(created.milestone, None, "issue-2 sits in no milestone");
    let new = item(&plan, &id);
    assert_eq!((new.kind, new.parent_id.as_deref()), (Kind::Defect, Some("issue-2")));

    // Under a milestone: the milestone is set at creation.
    let (_, id) = create(
        &h,
        E::CreateItem {
            item_kind: Kind::Task,
            name: "T".into(),
            container_id: Some("milestone-1".into()),
            description: None,
            due: None,
        },
    )
    .unwrap();
    let number: u64 = id.unwrap().trim_start_matches("issue-").parse().unwrap();
    assert_eq!(
        h.fake.lock().issues.iter().find(|i| i.number == number).unwrap().milestone,
        Some(1)
    );
}

/// GDP-FR-CNVO: a repository that offers Types but not the create Type is
/// `issue_type_unavailable`. A repository with no Type takes an untyped issue
/// for the kind of the untyped choice alone.
#[test]
fn create_item_needs_an_available_type() {
    let (h, _) = loaded();
    h.fake.lock().types = vec!["Epic".into(), "Task".into()];
    let feature = || E::CreateItem {
        item_kind: Kind::Feature,
        name: "F".into(),
        container_id: None,
        description: None,
        due: None,
    };
    assert_eq!(create(&h, feature()).unwrap_err().code, ERR_ISSUE_TYPE_UNAVAILABLE);
    h.fake.lock().types = Vec::new();
    assert_eq!(
        create(&h, feature()).unwrap_err().code,
        ERR_ISSUE_TYPE_UNAVAILABLE,
        "untyped choice unset"
    );
    assert!(h.fake.writes().is_empty());
    let mut settings = h.delivery_settings();
    settings.untyped = Some(MappedKind::Feature);
    h.save_delivery_settings(&settings);
    let (_, id) = create(&h, feature()).unwrap();
    let number: u64 = id.unwrap().trim_start_matches("issue-").parse().unwrap();
    assert_eq!(h.fake.lock().issues.iter().find(|i| i.number == number).unwrap().issue_type, None);
    let task = E::CreateItem {
        item_kind: Kind::Task,
        name: "T".into(),
        container_id: None,
        description: None,
        due: None,
    };
    assert_eq!(create(&h, task).unwrap_err().code, ERR_ISSUE_TYPE_UNAVAILABLE);
}

/// GDP-FR-DTPB: `rename`, `set_description`, and `set_due` of a milestone
/// write its title, description, and due date; `set_due` of an issue is
/// refused.
#[test]
fn milestone_fields_are_written() {
    // A task that sits in the milestone itself; issue-3 sits in issue-2.
    let (h, _) = loaded();
    let mut in_milestone = issue(5, Some("Task"));
    in_milestone.milestone = Some(1);
    h.fake.board_issue(in_milestone, None);
    let plan = load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    let at = stamp(&plan, "milestone-1");
    let p = edit(&h, E::Rename { item_id: "milestone-1".into(), name: "M".into(), updated_at: at })
        .unwrap();
    let at = stamp(&p, "milestone-1");
    let p = edit(
        &h,
        E::SetDescription {
            item_id: "milestone-1".into(),
            description: "D".into(),
            updated_at: at,
        },
    )
    .unwrap();
    let at = stamp(&p, "milestone-1");
    assert_eq!(h.fake.writes(), ["update_milestone 1", "update_milestone 1"]);
    let written = h.fake.lock().milestones[0].clone();
    assert_eq!((written.title.as_str(), written.description.as_str()), ("M", "D"));
    h.fake.lock().calls.clear();
    let p = edit(
        &h,
        E::SetDue { item_id: "milestone-1".into(), due: Some("2027-03-04".into()), updated_at: at },
    )
    .unwrap();
    assert_eq!(h.fake.writes(), ["update_milestone 1"]);
    assert_eq!(h.fake.lock().milestones[0].due.as_deref(), Some("2027-03-04"));
    assert_eq!(item(&p, "milestone-1").due.as_deref(), Some("2027-03-04"));
    let at = stamp(&p, "milestone-1");
    let p =
        edit(&h, E::SetDue { item_id: "milestone-1".into(), due: None, updated_at: at }).unwrap();
    assert_eq!(h.fake.lock().milestones[0].due, None, "the due date is removed");
    let m = item(&p, "milestone-1");
    assert_eq!(
        (m.name.as_str(), m.description.as_deref(), m.due.as_deref()),
        ("M", Some("D"), None)
    );
    let at = stamp(&p, "issue-1");
    let refused =
        edit(&h, E::SetDue { item_id: "issue-1".into(), due: None, updated_at: at }).unwrap_err();
    assert_eq!(refused.code, ERR_KIND_CHANGE_UNSUPPORTED);
    let at = stamp(&p, "issue-1");
    let p = edit(
        &h,
        E::SetDescription { item_id: "issue-1".into(), description: "Body".into(), updated_at: at },
    )
    .unwrap();
    assert_eq!(item(&p, "issue-1").description.as_deref(), Some("Body"));
}

/// GDP-FR-EJTS: `set_kind` writes the create Type of the new kind; a change
/// to or from `milestone` is `kind_change_unsupported`.
#[test]
fn set_kind_writes_the_create_type() {
    let (h, plan) = loaded();
    let at = stamp(&plan, "issue-3");
    let p =
        edit(&h, E::SetKind { item_id: "issue-3".into(), item_kind: Kind::Defect, updated_at: at })
            .unwrap();
    assert_eq!(
        h.fake.lock().issues.iter().find(|i| i.number == 3).unwrap().issue_type.as_deref(),
        Some("Bug")
    );
    assert_eq!(item(&p, "issue-3").kind, Kind::Defect);
    let at = stamp(&p, "issue-3");
    let to_milestone =
        E::SetKind { item_id: "issue-3".into(), item_kind: Kind::Milestone, updated_at: at };
    assert_eq!(edit(&h, to_milestone).unwrap_err().code, ERR_KIND_CHANGE_UNSUPPORTED);
    let at = stamp(&p, "milestone-1");
    let from_milestone =
        E::SetKind { item_id: "milestone-1".into(), item_kind: Kind::Task, updated_at: at };
    assert_eq!(edit(&h, from_milestone).unwrap_err().code, ERR_KIND_CHANGE_UNSUPPORTED);
}

/// GDP-FR-NJJZ: `move` sets the parent to the nearest issue container and the
/// milestone to the nearest milestone container, and removes each where there
/// is none. A milestone cannot move.
#[test]
fn move_sets_parent_and_milestone() {
    let (h, plan) = loaded();
    let at = stamp(&plan, "issue-3");
    let p = edit(
        &h,
        E::Move { item_id: "issue-3".into(), container_id: Some("issue-1".into()), updated_at: at },
    )
    .unwrap();
    assert_eq!(h.fake.writes(), ["set_parent 3 Some(1)", "set_milestone 3 None"]);
    assert_eq!(item(&p, "issue-3").parent_id.as_deref(), Some("issue-1"));
    h.fake.lock().calls.clear();
    let at = stamp(&p, "issue-3");
    let p = edit(&h, E::Move { item_id: "issue-3".into(), container_id: None, updated_at: at })
        .unwrap();
    assert_eq!(h.fake.writes(), ["set_parent 3 None"]);
    assert_eq!(item(&p, "issue-3").parent_id, None);
    // Each write names the current parent, which a removal needs.
    assert_eq!(h.fake.lock().current_parents, [(3, Some(2)), (3, Some(1))]);
    let at = stamp(&p, "milestone-1");
    let refused =
        edit(&h, E::Move { item_id: "milestone-1".into(), container_id: None, updated_at: at })
            .unwrap_err();
    assert_eq!(refused.code, ERR_KIND_CHANGE_UNSUPPORTED);
    let at = stamp(&p, "issue-3");
    let unknown = edit(
        &h,
        E::Move {
            item_id: "issue-3".into(),
            container_id: Some("issue-77".into()),
            updated_at: at,
        },
    );
    assert_eq!(unknown.unwrap_err().code, ERR_TRACKER_ITEM_NOT_FOUND);
}

/// GDP-FR-YGXA, GDP-FR-GBDF: `add_edge` and `remove_edge` of type `blocks`
/// add and remove a blocked-by relation on the blocked issue, checked against
/// the blocked item's `updated_at`. A milestone at either end is refused.
#[test]
fn blocks_edges_are_blocked_by_relations() {
    let (h, plan) = loaded();
    let add = |at: String| E::AddEdge {
        from: "issue-1".into(),
        to: "issue-3".into(),
        edge_type: EdgeType::Blocks,
        updated_at: at,
    };
    // The blocked item's stamp is checked, not the blocker's.
    assert_eq!(edit(&h, add(stamp(&plan, "issue-1"))).unwrap_err().code, ERR_CHANGED_ON_TRACKER);
    let p = edit(&h, add(stamp(&plan, "issue-3"))).unwrap();
    assert!(h.fake.writes().contains(&"add_blocked_by 3 1".to_string()));
    assert!(
        p.notices.omitted_relations.iter().any(|o| o.blocked_item_id == "issue-3"),
        "a lineage relation is omitted"
    );
    h.fake.board_issue(issue(4, Some("Task")), None);
    let p = load_plan_impl(&h.handle(), true).unwrap();
    let at = stamp(&p, "issue-4");
    let p = edit(
        &h,
        E::AddEdge {
            from: "issue-1".into(),
            to: "issue-4".into(),
            edge_type: EdgeType::Blocks,
            updated_at: at,
        },
    )
    .unwrap();
    assert!(p.document.edges.iter().any(|e| e.from == "issue-1" && e.to == "issue-4"));
    let at = stamp(&p, "issue-4");
    let p = edit(
        &h,
        E::RemoveEdge {
            from: "issue-1".into(),
            to: "issue-4".into(),
            edge_type: EdgeType::Blocks,
            updated_at: at,
        },
    )
    .unwrap();
    assert!(!p.document.edges.iter().any(|e| e.to == "issue-4"));
    let at = stamp(&p, "milestone-1");
    let refused = edit(
        &h,
        E::AddEdge {
            from: "issue-4".into(),
            to: "milestone-1".into(),
            edge_type: EdgeType::Blocks,
            updated_at: at,
        },
    );
    assert_eq!(refused.unwrap_err().code, ERR_MILESTONE_EDGE_UNSUPPORTED);
    // A milestone at the `from` end is refused too, for an add and a remove,
    // and nothing is written.
    h.fake.lock().calls.clear();
    let at = stamp(&p, "issue-4");
    for e in [
        E::AddEdge {
            from: "milestone-1".into(),
            to: "issue-4".into(),
            edge_type: EdgeType::Blocks,
            updated_at: at.clone(),
        },
        E::RemoveEdge {
            from: "milestone-1".into(),
            to: "issue-4".into(),
            edge_type: EdgeType::Blocks,
            updated_at: at.clone(),
        },
    ] {
        assert_eq!(edit(&h, e).unwrap_err().code, ERR_MILESTONE_EDGE_UNSUPPORTED);
    }
    assert!(h.fake.writes().is_empty(), "a refused edge writes nothing");
}

/// GDP-FR-WAKG: `remove_from_board` removes the item and each plan issue
/// under it from the Project; `close_not_planned` closes each as not planned;
/// for a milestone, either mode also closes the milestone.
#[test]
fn remove_item_takes_the_members_too() {
    let (h, plan) = loaded();
    let at = stamp(&plan, "issue-1");
    let p = edit(
        &h,
        E::RemoveItem {
            item_id: "issue-1".into(),
            mode: RemoveMode::RemoveFromBoard,
            updated_at: at,
        },
    )
    .unwrap();
    let removed: Vec<String> = h.fake.writes();
    assert_eq!(
        removed,
        [
            format!("remove_project_item {PROJECT} ITEM_3"),
            format!("remove_project_item {PROJECT} ITEM_2"),
            format!("remove_project_item {PROJECT} ITEM_1"),
        ]
    );
    let ids: Vec<&str> = p.document.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["milestone-1"]);

    // A task that sits in the milestone itself; issue-3 sits in issue-2.
    let (h, _) = loaded();
    let mut in_milestone = issue(5, Some("Task"));
    in_milestone.milestone = Some(1);
    h.fake.board_issue(in_milestone, None);
    let plan = load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().calls.clear();
    let at = stamp(&plan, "milestone-1");
    let p = edit(
        &h,
        E::RemoveItem {
            item_id: "milestone-1".into(),
            mode: RemoveMode::CloseNotPlanned,
            updated_at: at,
        },
    )
    .unwrap();
    assert_eq!(h.fake.writes(), ["close_issue 5 NotPlanned", "close_milestone 1"]);
    let facts = |id: &str| p.tracker.iter().find(|t| t.item_id == id).unwrap().clone();
    assert_eq!(
        (facts("issue-5").state, facts("issue-5").close_reason),
        ("closed", Some("not_planned"))
    );
    assert_eq!(facts("issue-3").state, "open", "an issue outside the milestone item stays");
    assert_eq!(facts("milestone-1").state, "closed");
}

/// GDP-FR-HULQ: `add_existing_issue` adds an issue of the repository to the
/// Project and moves it to the container; no such issue, an issue in the
/// plan, and an unmapped Type are refused.
#[test]
fn an_existing_issue_joins_the_plan() {
    let (h, _) = loaded();
    h.fake.lock().issues.push(issue(40, Some("Task")));
    h.fake.lock().issues.push(issue(41, Some("Chore")));
    let (p, id) =
        create(&h, E::AddExistingIssue { number: 40, container_id: Some("milestone-1".into()) })
            .unwrap();
    assert_eq!(id.as_deref(), Some("issue-40"));
    assert_eq!(
        h.fake.writes(),
        [format!("add_project_item {PROJECT} I_40"), "set_milestone 40 Some(1)".into()]
    );
    assert_eq!(item(&p, "issue-40").parent_id.as_deref(), Some("milestone-1"));
    let code =
        |n| create(&h, E::AddExistingIssue { number: n, container_id: None }).unwrap_err().code;
    assert_eq!(code(99), ERR_TRACKER_ITEM_NOT_FOUND);
    assert_eq!(code(40), ERR_ALREADY_IN_PLAN);
    assert_eq!(code(41), ERR_TYPE_UNMAPPED);
}

/// GDP-FR-YBHD: the overlay edits write the per-machine overlay and make no
/// GitHub request; the plan shows them.
#[test]
fn overlay_edits_make_no_request() {
    let (h, _) = loaded();
    edit(
        &h,
        E::SetReferences { item_id: "issue-3".into(), artifact_ids: vec!["specs/a.md".into()] },
    )
    .unwrap();
    edit(&h, E::SetFootprint { item_id: "issue-3".into(), footprint: vec!["src/".into()] })
        .unwrap();
    edit(&h, E::LinkDraft { item_id: "issue-3".into(), draft_id: "d-1".into() }).unwrap();
    edit(
        &h,
        E::AddEdge {
            from: "issue-1".into(),
            to: "milestone-1".into(),
            edge_type: EdgeType::Relates,
            updated_at: String::new(),
        },
    )
    .unwrap();
    let p = edit(&h, E::SetPlanReferences { references: vec!["specs/plan.md".into()] }).unwrap();
    assert!(h.fake.calls().is_empty(), "no request: {:?}", h.fake.calls());
    let i = item(&p, "issue-3");
    assert_eq!(i.draft_id.as_deref(), Some("d-1"));
    assert_eq!(i.footprint.as_deref(), Some(&["src/".to_string()][..]));
    assert_eq!(i.artifact_ids.as_deref(), Some(&["specs/a.md".to_string()][..]));
    assert!(p.document.edges.iter().any(|e| e.edge_type == EdgeType::Relates));
    assert_eq!(p.document.references.as_deref(), Some(&["specs/plan.md".to_string()][..]));
    let p = edit(&h, E::UnlinkDraft { item_id: "issue-3".into() }).unwrap();
    let p2 = edit(
        &h,
        E::RemoveEdge {
            from: "issue-1".into(),
            to: "milestone-1".into(),
            edge_type: EdgeType::Relates,
            updated_at: String::new(),
        },
    )
    .unwrap();
    assert_eq!(item(&p, "issue-3").draft_id, None);
    assert!(!p2.document.edges.iter().any(|e| e.edge_type == EdgeType::Relates));
    // DMS-FR-QFEA: an overlay that breaks a field rule is refused.
    let bad =
        edit(&h, E::SetFootprint { item_id: "issue-3".into(), footprint: vec!["../x".into()] });
    assert_eq!(bad.unwrap_err().code, crate::delivery::ERR_INVALID_OVERLAY);
}

/// GDP-FR-MSVS: an edit that needs several writes stops at the first failure
/// with `partial_edit`; the writes made stay, and the cache shows them.
#[test]
fn a_failure_after_a_write_is_a_partial_edit() {
    let (h, plan) = loaded();
    h.fake.lock().writes_before_failure = Some(1);
    let at = stamp(&plan, "issue-3");
    let refused = edit(
        &h,
        E::Move { item_id: "issue-3".into(), container_id: Some("issue-1".into()), updated_at: at },
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_PARTIAL_EDIT);
    assert_eq!(
        h.fake.writes(),
        ["set_parent 3 Some(1)", "set_milestone 3 None"],
        "it stops at the failure"
    );
    let cache = h.cache().unwrap();
    assert_eq!(cache.issue(3).unwrap().parent.as_ref().map(|p| p.number), Some(1));
    let p = load_plan_impl(&h.handle(), false).unwrap();
    assert_eq!(item(&p, "issue-3").parent_id.as_deref(), Some("issue-1"));
    // A failure of the first write is that write's own error.
    let at = stamp(&p, "issue-3");
    let first = edit(&h, E::Rename { item_id: "issue-3".into(), name: "x".into(), updated_at: at })
        .unwrap_err();
    assert_eq!(first.code, "github_request_failed");
}

/// GDP-FR-IRDD: edits run one at a time, in the order they arrive.
#[test]
fn edits_run_one_at_a_time_in_order() {
    let queue = Arc::new(EditQueue::default());
    let order = Arc::new(Mutex::new(Vec::new()));
    let first = queue.enter();
    let mut threads = Vec::new();
    for name in ["b", "c", "d"] {
        let (q, o) = (queue.clone(), order.clone());
        let arrived = queue.next.load(Ordering::SeqCst) + 1;
        threads.push(std::thread::spawn(move || {
            let _turn = q.enter();
            o.lock().unwrap().push(name);
        }));
        // Wait until this edit took its ticket: a state, not a time.
        super::wait_until("the edit takes its ticket", || {
            queue.next.load(Ordering::SeqCst) >= arrived
        });
    }
    assert!(order.lock().unwrap().is_empty(), "no edit runs while one holds its turn");
    drop(first);
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), ["b", "c", "d"]);
}

/// GDP-FR-IRDD: `apply_github_delivery_edit` takes its turn in the edit
/// queue of the project. While another edit holds its turn, the edit waits
/// and makes no request; it runs when the turn ends.
#[test]
fn an_applied_edit_waits_for_its_turn() {
    let (h, plan) = loaded();
    let app = h.handle();
    let state = app.state::<GithubDeliveryState>();
    let held = state.edits.enter();
    let waiting = {
        let app = h.handle();
        let updated_at = stamp(&plan, "issue-1");
        std::thread::spawn(move || {
            apply_edit_impl(
                &app,
                E::Rename { item_id: "issue-1".into(), name: "A".into(), updated_at },
            )
        })
    };
    super::wait_until("the edit takes its ticket", || state.edits.next.load(Ordering::SeqCst) >= 2);
    assert!(h.fake.calls().is_empty(), "no request while another edit holds its turn");
    drop(held);
    waiting.join().unwrap().unwrap();
    assert_eq!(h.fake.calls(), ["read_issue 1", "update_issue 1", "read_issue 1"]);
}

/// GDP-FR-UPXR: the edit layer reaches the tracker through the adapter trait
/// alone. The same edit, given another adapter, goes to that adapter, and the
/// adapter of the application sees no request.
#[test]
fn all_tracker_access_goes_through_the_adapter() {
    use super::super::edits::TrackerEdit;
    let (h, _) = loaded();
    let other = FakeTracker::new();
    other.lock().issues.push(issue(1, Some("Epic")));
    let mut read = h.cache().unwrap();
    let plan = super::super::plan::build_plan(
        &read,
        &DeliverySettings::default(),
        &Default::default(),
        &crate::github_polling::eligibility::StatusNames::of(&Default::default()),
    );
    let stamp = read.issue(1).unwrap().updated_at.clone();
    TrackerEdit {
        adapter: &other,
        project_id: PROJECT,
        settings: &DeliverySettings::default(),
        read: &mut read,
        plan: &plan,
    }
    .apply(&E::Rename { item_id: "issue-1".into(), name: "A".into(), updated_at: stamp })
    .unwrap();
    assert_eq!(other.calls(), ["read_issue 1", "update_issue 1", "read_issue 1"]);
    assert!(h.fake.calls().is_empty(), "the other adapter took every request");
    assert_eq!(read.issue(1).unwrap().title, "A", "the read takes the other adapter's answer");
}
