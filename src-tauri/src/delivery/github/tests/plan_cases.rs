//! The tests of the plan a GitHub read builds (`GDP-github-delivery-plan.md`
//! GDP-FR-BGIT to GDP-FR-XWES). Everything here is pure.

use serde_json::json;

use super::super::adapter::*;
use super::super::cache::TrackerRead;
use super::super::plan::*;
use super::fake::{issue, link, milestone, repository, status_field, PROJECT};
use crate::delivery::machine_state::{DeliveryOverlay, OverlayEntry, RelatesEdge};
use crate::delivery::settings::{DeliverySettings, MappedKind, ReadEntry};
use crate::flow_delivery::{CloseReason, TrackerState};
use crate::flow_validation::model::{EdgeType, Kind};
use crate::github_polling::eligibility::StatusNames;

fn on_board(issue: TrackerIssue, status: Option<&str>) -> ProjectItemRead {
    ProjectItemRead {
        item_id: format!("ITEM_{}", issue.number),
        status: status.map(str::to_string),
        content: ItemContent::Issue(issue),
    }
}

fn read(items: Vec<ProjectItemRead>, milestones: Vec<TrackerMilestone>) -> TrackerRead {
    TrackerRead {
        read_at: "2026-11-14T10:00:00Z".into(),
        repository: repository(),
        project: ProjectRead {
            node_id: PROJECT.into(),
            title: "Roadmap".into(),
            short_description: "The plan".into(),
            owner_login: "acme".into(),
            number: 7,
            url: "https://github.com/orgs/acme/projects/7".into(),
            status_field: Some(status_field()),
            items,
        },
        milestones,
    }
}

fn build(read: &TrackerRead) -> BuiltPlan {
    build_plan(
        read,
        &DeliverySettings::default(),
        &DeliveryOverlay::default(),
        &StatusNames::default(),
    )
}

fn item<'a>(plan: &'a BuiltPlan, id: &str) -> &'a crate::flow_validation::model::Item {
    plan.document.items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no item {id}"))
}

fn ids(plan: &BuiltPlan) -> Vec<&str> {
    plan.document.items.iter().map(|i| i.id.as_str()).collect()
}

fn child(number: u64, kind: &str, parent: u64) -> TrackerIssue {
    let mut i = issue(number, Some(kind));
    i.parent = Some(link(parent, "open"));
    i
}

/// GDP-FR-BGIT, DSR-FR-MKIG, DSR-FR-TZDZ, GDP-FR-XWES: a plan issue is an
/// issue of the polling repository whose kind is mapped. A pull request, a
/// draft, an issue of another repository, an excluded issue, and an unmapped
/// issue are skipped, and each unmapped Type is counted.
#[test]
fn only_mapped_issues_of_the_repository_are_plan_issues() {
    let mut foreign = issue(9, Some("Task"));
    foreign.owner = "other".into();
    let settings = DeliverySettings {
        read_entries: vec![
            ReadEntry { type_name: "Story".into(), kind: MappedKind::Feature },
            ReadEntry { type_name: "Spike".into(), kind: MappedKind::Exclude },
        ],
        ..DeliverySettings::default()
    };
    let r = read(
        vec![
            on_board(issue(1, Some("Task")), None),
            on_board(issue(2, Some("Story")), None),
            on_board(issue(3, Some("Spike")), None),
            on_board(issue(4, Some("Chore")), None),
            on_board(issue(5, Some("Chore")), None),
            on_board(issue(6, None), None),
            on_board(foreign, None),
            ProjectItemRead {
                item_id: "PR".into(),
                status: None,
                content: ItemContent::PullRequest,
            },
            ProjectItemRead { item_id: "D".into(), status: None, content: ItemContent::Draft },
        ],
        Vec::new(),
    );
    let plan = build_plan(&r, &settings, &DeliveryOverlay::default(), &StatusNames::default());
    assert_eq!(ids(&plan), ["issue-1", "issue-2"]);
    assert_eq!(item(&plan, "issue-1").kind, Kind::Task);
    assert_eq!(item(&plan, "issue-2").kind, Kind::Feature, "a read entry maps Story");
    assert_eq!(plan.notices.skipped, 7);
    let unmapped = serde_json::to_value(&plan.notices.unmapped_types).unwrap();
    assert_eq!(unmapped, json!([{ "type": "Chore", "count": 2 }, { "type": null, "count": 1 }]));

    // DSR-FR-TZDZ: with the untyped choice `exclude`, an untyped issue is
    // skipped and is not unmapped.
    let excluded = DeliverySettings { untyped: Some(MappedKind::Exclude), ..settings.clone() };
    let plan = build_plan(&r, &excluded, &DeliveryOverlay::default(), &StatusNames::default());
    assert!(plan.notices.unmapped_types.iter().all(|u| u.type_name.is_some()));
    // With the untyped choice `task`, the untyped issue is a task.
    let typed = DeliverySettings { untyped: Some(MappedKind::Task), ..settings };
    let plan = build_plan(&r, &typed, &DeliveryOverlay::default(), &StatusNames::default());
    assert_eq!(item(&plan, "issue-6").kind, Kind::Task);
}

/// DSR-FR-MKIG: a create Type wins over a read entry of the same name, and
/// names compare exactly.
#[test]
fn the_type_mapping_compares_names_exactly() {
    use crate::delivery::settings::{IssueKind, TypeMapping};
    let settings = DeliverySettings {
        read_entries: vec![ReadEntry { type_name: "Task".into(), kind: MappedKind::Epic }],
        ..DeliverySettings::default()
    };
    assert_eq!(settings.map_type(Some("Task")), TypeMapping::Kind(IssueKind::Task));
    assert_eq!(settings.map_type(Some("Bug")), TypeMapping::Kind(IssueKind::Defect));
    assert_eq!(settings.map_type(Some("task")), TypeMapping::Unmapped, "case matters");
    assert_eq!(settings.map_type(None), TypeMapping::Unmapped, "untyped choice unset");
}

/// GDP-FR-MYLM, GDP-FR-NFWA, GDP-FR-HNEW: every open milestone, and a closed
/// one only where a plan issue names it, as `milestone-<n>` items with the
/// milestone's title, description, and due date, by due date with undated ones
/// last and then by number. Issues follow in Project order as `issue-<n>`.
#[test]
fn milestones_come_first_in_due_order() {
    let mut named = issue(5, Some("Task"));
    named.milestone = Some(4);
    let mut m2 = milestone(2, Some("2026-12-01"), "open");
    m2.description = "Second".into();
    let r = read(
        vec![on_board(issue(8, Some("Task")), None), on_board(named, None)],
        vec![
            milestone(1, None, "open"),
            m2,
            milestone(3, Some("2026-10-01"), "open"),
            milestone(4, None, "closed"),
            milestone(6, Some("2026-01-01"), "closed"),
        ],
    );
    let plan = build(&r);
    assert_eq!(
        ids(&plan),
        ["milestone-3", "milestone-2", "milestone-1", "milestone-4", "issue-8", "issue-5"]
    );
    let m = item(&plan, "milestone-2");
    assert_eq!((m.kind, m.name.as_str()), (Kind::Milestone, "Milestone 2"));
    assert_eq!(m.description.as_deref(), Some("Second"));
    assert_eq!(m.due.as_deref(), Some("2026-12-01"));
    let i = item(&plan, "issue-8");
    assert_eq!((i.name.as_str(), i.description.as_deref()), ("Issue 8", Some("Body 8")));
    assert_eq!(plan.document.version, 3);
    assert_eq!(plan.document.name.as_deref(), Some("Roadmap"));
    assert_eq!(plan.document.description.as_deref(), Some("The plan"));
}

/// GDP-FR-KBWK, GDP-FR-JCAR: an item sits in its parent issue where that
/// parent is a plan issue that can hold it, else in its milestone, else at the
/// top level. A parent that cannot hold it makes the item misplaced.
#[test]
fn containment_follows_the_relaxed_hierarchy() {
    let mut task_in_milestone = child(3, "Task", 2);
    task_in_milestone.milestone = Some(1);
    let mut epic_under_task = child(4, "Epic", 3);
    epic_under_task.milestone = Some(1);
    let foreign_parent = {
        let mut i = issue(5, Some("Task"));
        i.parent = Some(IssueLink { owner: "other".into(), ..link(1, "open") });
        i
    };
    let r = read(
        vec![
            on_board(issue(1, Some("Epic")), None),
            on_board(child(2, "Task", 1), None),
            on_board(task_in_milestone, None),
            on_board(epic_under_task, None),
            on_board(foreign_parent, None),
            on_board(child(6, "Feature", 99), None),
        ],
        vec![milestone(1, None, "open")],
    );
    let plan = build(&r);
    assert_eq!(item(&plan, "issue-2").parent_id.as_deref(), Some("issue-1"), "a task in an epic");
    assert_eq!(
        item(&plan, "issue-3").parent_id.as_deref(),
        Some("milestone-1"),
        "a task cannot hold a task, so the milestone holds it"
    );
    assert_eq!(item(&plan, "issue-4").parent_id.as_deref(), Some("milestone-1"));
    assert_eq!(item(&plan, "issue-5").parent_id, None, "a parent of another repository");
    assert_eq!(item(&plan, "issue-6").parent_id, None, "a parent that is no plan issue");
    let misplaced = serde_json::to_value(&plan.notices.misplaced).unwrap();
    assert_eq!(
        misplaced,
        json!([
            { "itemId": "issue-3", "parentItemId": "issue-2" },
            { "itemId": "issue-4", "parentItemId": "issue-3" }
        ])
    );
    assert!(
        crate::flow_validation::validate(&serde_json::to_string(&plan.document).unwrap()).valid
    );
}

/// GDP-FR-SYSZ, GDP-FR-URZL: a blocked-by relation between plan issues is a
/// `blocks` edge from the blocker. One within a lineage, and one that closes a
/// cycle in Project order, is omitted with its reason. An open blocker that is
/// no plan issue is an external blocker; a closed one is nothing.
#[test]
fn relations_become_edges_or_notices() {
    let mut a = issue(1, Some("Task"));
    a.blocked_by = vec![link(2, "open")];
    let mut b = issue(2, Some("Task"));
    b.blocked_by = vec![link(1, "open"), link(50, "open"), link(51, "closed")];
    let mut epic = issue(3, Some("Epic"));
    epic.blocked_by = vec![link(4, "open")];
    let child_task = child(4, "Task", 3);
    let mut foreign = issue(5, Some("Task"));
    foreign.blocked_by = vec![IssueLink { owner: "other".into(), ..link(6, "open") }];
    let r = read(
        vec![
            on_board(a, None),
            on_board(b, None),
            on_board(epic, None),
            on_board(child_task, None),
            on_board(foreign, None),
        ],
        Vec::new(),
    );
    let plan = build(&r);
    let edges: Vec<(&str, &str, EdgeType)> =
        plan.document.edges.iter().map(|e| (e.from.as_str(), e.to.as_str(), e.edge_type)).collect();
    assert_eq!(edges, [("issue-2", "issue-1", EdgeType::Blocks)]);
    let omitted = serde_json::to_value(&plan.notices.omitted_relations).unwrap();
    assert_eq!(
        omitted,
        json!([
            { "blockedItemId": "issue-2", "blockingItemId": "issue-1", "reason": "cycle" },
            { "blockedItemId": "issue-3", "blockingItemId": "issue-4", "reason": "lineage" }
        ])
    );
    let facts = |id: &str| plan.tracker.iter().find(|t| t.item_id == id).unwrap();
    let blockers: Vec<u64> = facts("issue-2").external_blockers.iter().map(|b| b.number).collect();
    assert_eq!(blockers, [50], "the closed blocker is not listed");
    assert_eq!(facts("issue-2").external_blockers[0].title, "Issue 50");
    assert_eq!(facts("issue-5").external_blockers.len(), 1, "another repository is external");
    assert_eq!(plan.facts["issue-2"].open_external_blockers, 1);
    assert!(
        crate::flow_validation::validate(&serde_json::to_string(&plan.document).unwrap()).valid
    );
}

/// GDP-FR-QGFO, GPP-FR-BOQX, GPP-FR-VXSB, FDL-FR-WPSZ: the tracker facts of
/// each item: its reference, state, close reason, `updated_at`, Status, and
/// claim eligibility. The In Progress status resolves through the Status
/// field, and the state rules get it as `status_is_in_progress`.
#[test]
fn each_item_carries_its_tracker_facts() {
    let mut done = issue(2, Some("Task"));
    done.state = "closed".into();
    done.state_reason = Some(StateReason::Completed);
    let mut dropped = issue(3, Some("Task"));
    dropped.state = "closed".into();
    dropped.state_reason = Some(StateReason::NotPlanned);
    let r = read(
        vec![
            on_board(issue(1, Some("Task")), Some("Ready")),
            on_board(done, None),
            on_board(dropped, None),
            on_board(issue(4, Some("Epic")), Some("Ready")),
            on_board(issue(5, Some("Task")), Some("In Progress")),
        ],
        vec![milestone(1, None, "closed"), milestone(2, None, "open")],
    );
    let plan = build(&r);
    let value = serde_json::to_value(&plan.tracker).unwrap();
    assert_eq!(
        value[0],
        json!({
            "itemId": "milestone-2",
            "ref": { "kind": "milestone", "number": 2,
                     "url": "https://github.com/acme/widgets/milestone/2" },
            "state": "open", "closeReason": null, "updatedAt": "2026-01-02T00:00:02Z",
            "status": null, "claimEligible": false, "externalBlockers": [], "assignees": []
        })
    );
    assert_eq!(
        value[1],
        json!({
            "itemId": "issue-1",
            "ref": { "kind": "issue", "number": 1,
                     "url": "https://github.com/acme/widgets/issues/1" },
            "state": "open", "closeReason": null, "updatedAt": "2026-01-01T00:00:01Z",
            "status": "Ready", "claimEligible": true, "externalBlockers": [], "assignees": []
        })
    );
    assert_eq!(value[2]["closeReason"], "completed");
    assert_eq!(value[3]["closeReason"], "not_planned");
    assert_eq!(value[4]["claimEligible"], false, "an Epic is never eligible");
    assert_eq!(plan.facts["issue-3"].close_reason, Some(CloseReason::NotPlanned));
    assert_eq!(plan.facts["issue-2"].state, TrackerState::Closed);
    assert!(plan.facts["issue-5"].status_is_in_progress);
    assert!(!plan.facts["issue-1"].status_is_in_progress);
    let reference = plan.facts["issue-1"].issue.as_ref().unwrap();
    assert_eq!((reference.owner.as_str(), reference.number), ("acme", 1));

    // A stored In Progress name resolves exactly.
    let statuses = StatusNames { ready: None, in_progress: Some("Todo".into()), blocked: None };
    let r2 = read(vec![on_board(issue(5, Some("Task")), Some("Todo"))], Vec::new());
    let plan =
        build_plan(&r2, &DeliverySettings::default(), &DeliveryOverlay::default(), &statuses);
    assert!(plan.facts["issue-5"].status_is_in_progress);
}

/// GDP-FR-QGFO: the tracker facts of an issue carry the logins of its
/// assignees in GitHub order, and a milestone carries none.
#[test]
fn the_tracker_facts_carry_the_assignees() {
    let mut taken = issue(1, Some("Task"));
    taken.assignees = vec!["mia".into(), "jo".into()];
    let r = read(
        vec![on_board(taken, None), on_board(issue(2, Some("Task")), None)],
        vec![milestone(1, None, "open")],
    );
    let value = serde_json::to_value(&build(&r).tracker).unwrap();
    let of = |id: &str| {
        value.as_array().unwrap().iter().find(|f| f["itemId"] == id).unwrap()["assignees"].clone()
    };
    assert_eq!(of("issue-1"), json!(["mia", "jo"]));
    assert_eq!(of("issue-2"), json!([]));
    assert_eq!(of("milestone-1"), json!([]));
}

/// GDP-FR-QGFO: an issue of a read cache that holds no assignees reads as an
/// issue with no assignee.
#[test]
fn a_cached_issue_without_assignees_has_none() {
    let mut cached = serde_json::to_value(issue(1, Some("Task"))).unwrap();
    cached.as_object_mut().unwrap().remove("assignees").expect("an issue holds its assignees");
    let back: TrackerIssue = serde_json::from_value(cached).unwrap();
    assert!(back.assignees.is_empty());
}

/// GDP-FR-CRRO: the overlay adds each item's references, footprint, and draft
/// link, the `relates` edges, and the plan's references. An entry of no plan
/// item is not part of the plan.
#[test]
fn the_overlay_completes_the_plan() {
    let mut overlay = DeliveryOverlay::default();
    overlay.items.insert(
        "issue-1".into(),
        OverlayEntry {
            artifact_ids: Some(vec!["specs/a.md".into()]),
            footprint: Some(vec!["src/".into()]),
            draft_id: Some("draft-1".into()),
        },
    );
    overlay.items.insert(
        "issue-77".into(),
        OverlayEntry { draft_id: Some("d".into()), ..Default::default() },
    );
    overlay.relates = vec![
        RelatesEdge { from: "issue-1".into(), to: "issue-2".into() },
        RelatesEdge { from: "issue-1".into(), to: "issue-77".into() },
        RelatesEdge { from: "issue-3".into(), to: "issue-4".into() },
    ];
    overlay.references = vec!["specs/plan.md".into()];
    let r = read(
        vec![
            on_board(issue(1, Some("Task")), None),
            on_board(issue(2, Some("Task")), None),
            on_board(issue(3, Some("Epic")), None),
            on_board(child(4, "Task", 3), None),
        ],
        Vec::new(),
    );
    let plan = build_plan(&r, &DeliverySettings::default(), &overlay, &StatusNames::default());
    let i = item(&plan, "issue-1");
    assert_eq!(i.artifact_ids.as_deref(), Some(&["specs/a.md".to_string()][..]));
    assert_eq!(i.footprint.as_deref(), Some(&["src/".to_string()][..]));
    assert_eq!(i.draft_id.as_deref(), Some("draft-1"));
    assert!(!ids(&plan).contains(&"issue-77"));
    let relates: Vec<(&str, &str)> = plan
        .document
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Relates)
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    assert_eq!(relates, [("issue-1", "issue-2")], "unknown ends and lineage are left out");
    assert_eq!(plan.document.references.as_deref(), Some(&["specs/plan.md".to_string()][..]));
    assert!(
        crate::flow_validation::validate(&serde_json::to_string(&plan.document).unwrap()).valid
    );
}

/// GDP-FR-NFWA: an item id names its tracker item; other text names none.
#[test]
fn item_ids_name_tracker_items() {
    assert_eq!(parse_item_id("issue-12"), Some(PlanRef::Issue(12)));
    assert_eq!(parse_item_id("milestone-3"), Some(PlanRef::Milestone(3)));
    for bad in ["issue-", "issue-x", "milestone--1", "task-1", "issue-1a", ""] {
        assert_eq!(parse_item_id(bad), None, "{bad:?}");
    }
    assert_eq!(issue_id(4), "issue-4");
    assert_eq!(milestone_id(4), "milestone-4");
}

/// GDP-FR-XJDE, GDP-FR-HNEW: the milestones of an answer follow the milestone
/// order of the overlay. The milestones it does not name come after them, by
/// due date with undated ones last, then by number. An id of the order that
/// names no milestone of the plan, or no milestone at all, is passed over.
#[test]
fn milestones_follow_the_overlay_order_then_the_due_date() {
    let milestones = vec![
        milestone(1, Some("2026-12-01"), "open"),
        milestone(2, None, "open"),
        milestone(3, Some("2026-11-01"), "open"),
        milestone(4, None, "open"),
        milestone(5, Some("2026-10-01"), "open"),
    ];
    let r = read(Vec::new(), milestones);
    let ids = |overlay: &DeliveryOverlay| -> Vec<String> {
        let built = build_plan(&r, &DeliverySettings::default(), overlay, &StatusNames::default());
        built.document.items.iter().map(|i| i.id.clone()).collect()
    };
    assert_eq!(
        ids(&DeliveryOverlay::default()),
        ["milestone-5", "milestone-3", "milestone-1", "milestone-2", "milestone-4"],
        "no order: by due date, undated last, then by number"
    );
    let ordered = DeliveryOverlay {
        milestone_order: vec![
            "milestone-4".into(),
            "milestone-9".into(),
            "issue-1".into(),
            "milestone-1".into(),
        ],
        ..Default::default()
    };
    assert_eq!(
        ids(&ordered),
        ["milestone-4", "milestone-1", "milestone-5", "milestone-3", "milestone-2"]
    );
}

/// GPP-FR-BLVX, GPP-FR-VXSB, GPP-FR-DBPI, FDL-FR-SBLQ: the Blocked status
/// resolves through the Status field: an unset name takes the option named
/// `Blocked` without case, also where the Ready status resolves to none, and a
/// stored name takes its option exactly. Without such an option no item is
/// blocked by Status, and the Blocked status never shares the option of
/// another status.
#[test]
fn the_blocked_status_resolves_through_the_status_field() {
    let with_options = |names: &[&str], items: Vec<ProjectItemRead>| {
        let mut r = read(items, Vec::new());
        let field = r.project.status_field.as_mut().unwrap();
        field.options = names
            .iter()
            .map(|n| StatusChoice { id: format!("OPT_{n}"), name: (*n).into() })
            .collect();
        r
    };
    let blocked_of = |r: &TrackerRead, statuses: &StatusNames, id: &str| {
        let plan =
            build_plan(r, &DeliverySettings::default(), &DeliveryOverlay::default(), statuses);
        plan.facts[id].status_is_blocked
    };
    let items = || {
        vec![
            on_board(issue(1, Some("Task")), Some("blocked")),
            on_board(issue(2, Some("Task")), Some("On hold")),
            on_board(issue(3, Some("Task")), Some("Todo")),
        ]
    };
    let unset = StatusNames::default();
    // No Ready option: the configuration is not valid, the Blocked status still resolves.
    let r = with_options(&["Todo", "blocked", "On hold"], items());
    assert!(blocked_of(&r, &unset, "issue-1"), "the default name, without case");
    assert!(!blocked_of(&r, &unset, "issue-2"));
    assert!(!blocked_of(&r, &unset, "issue-3"));

    let stored = StatusNames { blocked: Some("On hold".into()), ..StatusNames::default() };
    assert!(blocked_of(&r, &stored, "issue-2"), "a stored name, exactly");
    assert!(!blocked_of(&r, &stored, "issue-1"), "the default does not apply");
    let cased = StatusNames { blocked: Some("Blocked".into()), ..StatusNames::default() };
    assert!(!blocked_of(&r, &cased, "issue-1"), "a stored name never matches another case");

    let none = with_options(&["Todo", "Ready", "In Progress"], items());
    assert!(!blocked_of(&none, &unset, "issue-1"), "no Blocked option");

    let taken = StatusNames { ready: Some("blocked".into()), ..StatusNames::default() };
    assert!(!blocked_of(&r, &taken, "issue-1"), "the Ready status holds that option");
}
