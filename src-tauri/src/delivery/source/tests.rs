//! The tests of the delivery source (`DSR-delivery-source.md`) and of the
//! GitHub plan search of `FDL-flow-delivery.md` (FDL-FR-JLUJ), against the
//! fake tracker adapter of `crate::delivery::github::fake`. No test reaches a
//! network.

use serde_json::json;

use super::export::export_impl;
use super::milestones::{milestone_order, milestones_impl, milestones_of};
use super::publish::{containers_first, publish_impl};
use super::*;
use crate::delivery::github::tests::fake::*;
use crate::delivery::machine_state::{
    load_layout, read_delivery_overlay, read_publish_journal, save_layout, DeliveryLayout,
    LayoutEntry, Point,
};
use crate::delivery::settings::{CreateTypes, MappedKind};
use crate::flow_validation::model::parse_valid;
use crate::logging::{LogBuffer, LogFilter};

const FLOW: &str = "plans/delivery.flow";

/// A Flow of a milestone, an epic in it, a task in the epic, a second task,
/// a `blocks` edge, and a `relates` edge.
fn flow_body() -> String {
    json!({
        "version": 3,
        "name": "Delivery",
        "references": ["specs/plan.md"],
        "items": [
            { "id": "t2", "kind": "task", "name": "Second", "footprint": ["src/b/"] },
            { "id": "t1", "kind": "task", "name": "First", "parentId": "e1",
              "artifactIds": ["specs/a.md"], "draftId": "draft-1" },
            { "id": "e1", "kind": "epic", "name": "Epic", "parentId": "m1" },
            { "id": "m1", "kind": "milestone", "name": "Beta", "due": "2027-01-01" }
        ],
        "edges": [
            { "id": "b1", "from": "t1", "to": "t2", "type": "blocks" },
            { "id": "r1", "from": "t2", "to": "e1", "type": "relates" }
        ]
    })
    .to_string()
}

fn file_source(h: &Harness, body: &str) {
    h.write(FLOW, body);
    h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: FLOW.into() },
        ..DeliverySettings::default()
    });
}

fn records(buffer: &LogBuffer) -> String {
    let page = buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap();
    serde_json::to_string(&page.records).unwrap()
}

/// DSR-FR-AECK, DSR-FR-EDHP, DSR-FR-RHUM, DSR-FR-ZKGG: the resolution of each
/// choice.
#[test]
fn each_choice_resolves() {
    let flow = |id: &str| id == "a.flow";
    let github = |p: &str| Resolved::Github { project_node_id: p.into() };
    let none =
        |reason, id: Option<&str>| Resolved::None { reason, flow_id: id.map(str::to_string) };
    assert_eq!(resolve_choice(&DeliverySourceChoice::Automatic, Some("P"), flow), github("P"));
    assert_eq!(
        resolve_choice(&DeliverySourceChoice::Automatic, None, flow),
        none(NoneReason::NoSource, None)
    );
    assert_eq!(resolve_choice(&DeliverySourceChoice::Github, Some("P"), flow), github("P"));
    assert_eq!(
        resolve_choice(&DeliverySourceChoice::Github, Some("  "), flow),
        none(NoneReason::GithubProjectUnset, None)
    );
    let file = |id: &str| DeliverySourceChoice::File { flow_id: id.into() };
    assert_eq!(
        resolve_choice(&file("a.flow"), None, flow),
        Resolved::File { flow_id: "a.flow".into() }
    );
    assert_eq!(
        resolve_choice(&file("b.md"), Some("P"), flow),
        none(NoneReason::DeliveryFileMissing, Some("b.md"))
    );
}

/// GPP-FR-OVOS, DSR-FR-AECK: the selected GitHub Project is the delivery, so
/// a change of the selection changes which plan the source names.
#[test]
fn the_selected_project_is_the_delivery() {
    let flow = |_: &str| false;
    for choice in [DeliverySourceChoice::Automatic, DeliverySourceChoice::Github] {
        assert_eq!(
            resolve_choice(&choice, Some("P1"), flow),
            Resolved::Github { project_node_id: "P1".into() }
        );
        assert_eq!(
            resolve_choice(&choice, Some("P2"), flow),
            Resolved::Github { project_node_id: "P2".into() }
        );
    }
}

/// DSR-FR-AECK, DSR-FR-EDHP, DSR-FR-RHUM, DSR-FR-ZKGG: the resolved source of
/// the open project, in the frontend shape.
#[test]
fn the_source_resolves_in_the_frontend_shape() {
    let h = Harness::new();
    let app = h.handle();
    assert_eq!(
        serde_json::to_value(get_source_impl(&app).unwrap()).unwrap(),
        json!({ "kind": "none", "reason": "no_source" })
    );
    h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::Github,
        ..Default::default()
    });
    assert_eq!(
        serde_json::to_value(get_source_impl(&app).unwrap()).unwrap(),
        json!({ "kind": "none", "reason": "github_project_unset" })
    );
    file_source(&h, "");
    assert_eq!(
        serde_json::to_value(get_source_impl(&app).unwrap()).unwrap(),
        json!({ "kind": "file", "flowId": FLOW, "path": FLOW })
    );
    h.write("notes/x.md", "# x");
    h.save_delivery_settings(&DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: "notes/x.md".into() },
        ..Default::default()
    });
    assert_eq!(
        serde_json::to_value(get_source_impl(&app).unwrap()).unwrap(),
        json!({ "kind": "none", "reason": "delivery_file_missing", "flowId": "notes/x.md" })
    );

    // DSR-FR-EDHP: a GitHub source with its Project, repository, and
    // configuration state; the cache gives the Project's details.
    let h = Harness::github_plan();
    let source = serde_json::to_value(get_source_impl(&h.handle()).unwrap()).unwrap();
    assert_eq!(source["kind"], "github");
    assert_eq!(source["project"]["nodeId"], PROJECT);
    assert_eq!(
        source["repository"],
        json!({ "host": "github.com", "owner": "acme", "name": "widgets" })
    );
    assert_eq!(source["configuration"]["state"], "unchecked");
    assert_eq!(source["project"]["url"], "", "no read is cached: the URL is empty");
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    let source = serde_json::to_value(get_source_impl(&h.handle()).unwrap()).unwrap();
    assert_eq!(
        source["project"],
        json!({ "nodeId": PROJECT, "title": "Roadmap", "ownerLogin": "acme", "number": 7,
                "url": PROJECT_URL })
    );
    assert_eq!(source["configuration"]["state"], "valid");
}

/// DSR-FR-CPYM: empty names and shared Types are `invalid_delivery_settings`;
/// names are trimmed and compare exactly.
#[test]
fn settings_validation_refuses_shared_types() {
    let with = |types: CreateTypes, entries: Vec<ReadEntry>| DeliverySettings {
        create_types: types,
        read_entries: entries,
        ..Default::default()
    };
    let entry = |t: &str, k| ReadEntry { type_name: t.into(), kind: k };
    let types = |task: &str| CreateTypes { task: task.into(), ..CreateTypes::default() };
    let code = |s: DeliverySettings| validate_settings(&s).map(|_| ()).map_err(|e| e.code);
    assert_eq!(code(with(types("  "), vec![])), Err(ERR_INVALID_SETTINGS.into()));
    assert_eq!(code(with(types("Epic"), vec![])), Err(ERR_INVALID_SETTINGS.into()));
    assert_eq!(
        code(with(types("Task"), vec![entry(" ", MappedKind::Task)])),
        Err(ERR_INVALID_SETTINGS.into())
    );
    assert_eq!(
        code(with(
            types("Task"),
            vec![entry("Story", MappedKind::Feature), entry("Story", MappedKind::Task)]
        )),
        Err(ERR_INVALID_SETTINGS.into())
    );
    assert_eq!(
        code(with(types("Task"), vec![entry("Bug ", MappedKind::Exclude)])),
        Err(ERR_INVALID_SETTINGS.into())
    );
    let ok =
        validate_settings(&with(types(" Chore "), vec![entry("task", MappedKind::Task)])).unwrap();
    assert_eq!(ok.create_types.task, "Chore");
}

/// DSR-FR-HOEW, DSR-FR-ZJMC, DSR-FR-NFFP, DSR-FR-EGFF: the settings are
/// stored and returned, with defaults applied; a `file` choice that is no Flow
/// is `not_a_flow` and writes nothing; each stored change emits the resolved
/// source.
#[test]
fn settings_are_stored_and_announced() {
    let h = Harness::new();
    let app = h.handle();
    let events = listen(&app, DELIVERY_SOURCE_CHANGED);
    assert_eq!(get_settings_impl(&app).unwrap(), DeliverySettings::default());
    h.write("notes/x.md", "# x");
    let before = std::fs::read_to_string(h.dir.path().join(".synthesis/project.toml")).ok();
    let refused = set_settings_impl(
        &app,
        DeliverySettings {
            source: DeliverySourceChoice::File { flow_id: "notes/x.md".into() },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(refused.code, ERR_NOT_A_FLOW);
    let invalid = DeliverySettings {
        create_types: CreateTypes { epic: "".into(), ..Default::default() },
        ..Default::default()
    };
    assert_eq!(set_settings_impl(&app, invalid).unwrap_err().code, ERR_INVALID_SETTINGS);
    assert_eq!(std::fs::read_to_string(h.dir.path().join(".synthesis/project.toml")).ok(), before);
    assert!(events.lock().unwrap().is_empty(), "a refusal announces nothing");

    h.write(FLOW, "");
    let wanted = DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: FLOW.into() },
        untyped: Some(MappedKind::Task),
        ..Default::default()
    };
    assert_eq!(set_settings_impl(&app, wanted.clone()).unwrap(), wanted);
    assert_eq!(get_settings_impl(&app).unwrap(), wanted);
    let seen = events.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&seen[0]).unwrap()["kind"], "file");
}

/// DSR-FR-CFQA, DSR-FR-LYMF: each operation refuses with `no_project_open`
/// when no project is open, and logs the refusal.
#[test]
fn no_project_is_refused() {
    static LOG: LogBuffer = LogBuffer::new();
    let h = Harness::with_buffer(&LOG);
    h.close();
    let app = h.handle();
    assert_eq!(get_settings_impl(&app).unwrap_err().code, "no_project_open");
    assert_eq!(
        set_settings_impl(&app, DeliverySettings::default()).unwrap_err().code,
        "no_project_open"
    );
    assert_eq!(get_source_impl(&app).unwrap_err().code, "no_project_open");
    assert_eq!(publish_impl(&app).unwrap_err().code, "no_project_open");
    assert_eq!(export_impl(&app, "a.flow").unwrap_err().code, "no_project_open");
    assert_eq!(milestones_impl(&app, false).unwrap_err().code, "no_project_open");
    let text = records(&LOG);
    for op in [
        "get_delivery_settings",
        "set_delivery_settings",
        "get_delivery_source",
        "publish_delivery_to_github",
        "export_delivery_to_file",
        "load_delivery_milestones",
    ] {
        assert!(text.contains(op), "the refusal of {op} is logged");
    }
}

/// DSR-FR-RCLA: a source that is not `file`, no selected Project, and an
/// invalid configuration each refuse the publish, and nothing changes.
#[test]
fn a_publish_needs_a_file_source_and_a_valid_configuration() {
    let h = Harness::github_plan();
    assert_eq!(publish_impl(&h.handle()).unwrap_err().code, ERR_NOT_FILE_SOURCE);
    file_source(&h, &flow_body());
    h.select_project(None);
    assert_eq!(publish_impl(&h.handle()).unwrap_err().code, "polling_unconfigured");
    h.select_project(Some(PROJECT));
    h.fake.lock().status_field = None;
    assert_eq!(publish_impl(&h.handle()).unwrap_err().code, "status_field_missing");
    assert!(h.fake.writes().is_empty(), "a refused publish changes nothing");
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::File { flow_id: FLOW.into() });
}

/// DSR-FR-GXSE: an invalid Flow is `flow_invalid` with its report; a `blocks`
/// edge on a milestone is refused before anything is created.
#[test]
fn an_invalid_flow_is_not_published() {
    let h = Harness::github_plan();
    file_source(&h, "{ \"version\": 3, \"items\": [ { \"id\": \"a\" } ] }");
    let refused = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(refused.code, ERR_FLOW_INVALID);
    assert!(refused.report.is_some_and(|r| !r.valid));
    let body = json!({ "version": 3, "items": [
        { "id": "m", "kind": "milestone", "name": "M" }, { "id": "t", "kind": "task", "name": "T" } ],
        "edges": [ { "id": "b", "from": "m", "to": "t", "type": "blocks" } ] });
    file_source(&h, &body.to_string());
    assert_eq!(publish_impl(&h.handle()).unwrap_err().code, "milestone_edge_unsupported");
    assert!(h.fake.writes().is_empty());
}

/// DSR-FR-GXSE: containers come before their members, in document order
/// within one depth.
#[test]
fn containers_are_created_first() {
    let doc = parse_valid(&flow_body()).unwrap();
    let order: Vec<&str> = containers_first(&doc).iter().map(|i| i.id.as_str()).collect();
    assert_eq!(order, ["t2", "m1", "e1", "t1"]);
}

/// DSR-FR-GXSE, DSR-FR-PTIT, DSR-FR-WEVP, DSR-FR-EGFF: a publish creates the
/// items containers first and then the `blocks` relations, carries the
/// machine state under the new ids, sets the source to `github`, removes the
/// journal, keeps the file, and returns the counts.
#[test]
fn a_publish_moves_the_plan_to_github() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &flow_body());
    let store = h.machine_store();
    let mut layout = DeliveryLayout::default();
    layout.items.insert(
        "t1".into(),
        LayoutEntry { position: Some(Point { x: 5.0, y: 6.0 }), ..Default::default() },
    );
    save_layout(&store, &format!("flow:{FLOW}"), &layout).unwrap();
    let events = listen(&h.handle(), DELIVERY_SOURCE_CHANGED);

    let result = publish_impl(&h.handle()).expect("published");
    assert_eq!((result.milestones, result.issues, result.relations), (1, 3, 1));
    assert_eq!(result.created_urls.len(), 4);
    let creates: Vec<String> =
        h.fake.writes().into_iter().filter(|c| c.starts_with("create")).collect();
    assert_eq!(creates, ["create_issue", "create_milestone", "create_issue", "create_issue"]);
    // t2 = #101, e1 = #102 in milestone 1, t1 = #103 under #102; t1 blocks t2.
    let issue = |n: u64| h.fake.lock().issues.iter().find(|i| i.number == n).cloned().unwrap();
    assert_eq!(issue(102).milestone, Some(1));
    assert_eq!(issue(103).parent.as_ref().map(|p| p.number), Some(102));
    assert_eq!(issue(103).milestone, Some(1));
    assert!(issue(101).blocked_by.iter().any(|b| b.number == 103));

    let overlay =
        read_delivery_overlay(&h.handle(), &crate::logging::BUFFER, &store, PROJECT).unwrap();
    assert_eq!(overlay.items["issue-103"].draft_id.as_deref(), Some("draft-1"));
    assert_eq!(
        overlay.items["issue-103"].artifact_ids.as_deref(),
        Some(&["specs/a.md".to_string()][..])
    );
    assert_eq!(overlay.items["issue-101"].footprint.as_deref(), Some(&["src/b/".to_string()][..]));
    assert_eq!(overlay.relates.len(), 1);
    assert_eq!(
        (overlay.relates[0].from.as_str(), overlay.relates[0].to.as_str()),
        ("issue-101", "issue-102")
    );
    assert_eq!(overlay.references, ["specs/plan.md"]);
    let github_layout =
        load_layout(&h.handle(), &crate::logging::BUFFER, &store, &format!("github:{PROJECT}"))
            .unwrap();
    assert!(github_layout.items.contains_key("issue-103"));

    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::Github);
    assert!(read_publish_journal(&h.handle(), &crate::logging::BUFFER, &store, FLOW)
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        std::fs::read_to_string(h.dir.path().join(FLOW)).unwrap(),
        flow_body(),
        "the file stays"
    );
    let seen = events.lock().unwrap().clone();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(seen.last().unwrap()).unwrap()["kind"],
        "github"
    );
}

/// DSR-FR-ULON: a failed publish stops, keeps the journal, and names each item
/// it created; a later publish reuses the journal and creates only the rest.
#[test]
fn a_failed_publish_resumes_from_its_journal() {
    let h = Harness::new();
    h.select_project(Some(PROJECT));
    file_source(&h, &flow_body());
    // Three writes succeed: the create of the issue of t2, its add to the
    // Project, and the create of the milestone m1. The create of the issue of
    // e1 is the first write of its edit, so it fails with its own code.
    h.fake.lock().writes_before_failure = Some(3);
    let failed = publish_impl(&h.handle()).unwrap_err();
    assert_eq!(failed.code, crate::github_polling::records::ERR_REQUEST_FAILED);
    let urls = failed.created_urls.clone().unwrap();
    assert_eq!(
        urls,
        [
            "https://github.com/acme/widgets/issues/101",
            "https://github.com/acme/widgets/milestone/1"
        ],
        "the issue of t2 and the milestone m1"
    );
    let store = h.machine_store();
    let journal = read_publish_journal(&h.handle(), &crate::logging::BUFFER, &store, FLOW).unwrap();
    assert_eq!(journal.items.len(), 2);
    assert_eq!((journal.items["t2"].number, journal.items["m1"].number), (101, 1));
    assert!(journal.items.values().all(|e| e.complete), "each create was whole");
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::File { flow_id: FLOW.into() });

    h.fake.lock().writes_before_failure = None;
    h.fake.lock().calls.clear();
    let result = publish_impl(&h.handle()).unwrap();
    assert_eq!((result.milestones, result.issues, result.relations), (0, 2, 1));
    let creates = h.fake.writes().into_iter().filter(|c| c.starts_with("create")).count();
    assert_eq!(creates, 2, "only the rest is created");
    assert_eq!(h.fake.lock().milestones.len(), 1, "no second milestone");
    // The new issues name the journalled items: e1 (#102) is in milestone 1,
    // t1 (#103) is under e1, and t1 blocks t2 (#101).
    let issue = |n: u64| h.fake.lock().issues.iter().find(|i| i.number == n).cloned().unwrap();
    assert_eq!(issue(102).milestone, Some(1));
    assert_eq!(issue(103).parent.map(|p| p.number), Some(102));
    assert_eq!(issue(101).blocked_by.iter().map(|b| b.number).collect::<Vec<_>>(), [103]);
    assert_eq!(h.fake.lock().issues.len(), 3, "no second issue of t2");
}

/// DSR-FR-OIAS: a source that is not `github`, and a path that exists, are
/// refused; a read that fails is the export's error.
#[test]
fn an_export_needs_a_github_source_and_a_new_path() {
    let h = Harness::new();
    assert_eq!(export_impl(&h.handle(), "a.flow").unwrap_err().code, ERR_NOT_GITHUB_SOURCE);
    let h = Harness::github_plan();
    h.write("plans/old.flow", "");
    assert_eq!(export_impl(&h.handle(), "plans/old.flow").unwrap_err().code, ERR_PATH_EXISTS);
    assert_eq!(export_impl(&h.handle(), "plans/x.md").unwrap_err().code, ERR_NOT_A_FLOW);
    h.fake.lock().read_error = Some("github_unreachable".into());
    assert_eq!(export_impl(&h.handle(), "plans/new.flow").unwrap_err().code, "github_unreachable");
    crate::delivery::github::load_plan_impl(&h.handle(), false).unwrap_err();
    assert!(!h.dir.path().join("plans/new.flow").exists());

    // A read cache exists: the failed read answers a stale plan, and the
    // export refuses it with the error of the read and writes nothing.
    let h = Harness::github_plan();
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    h.fake.lock().read_error = Some("github_unreachable".into());
    let stale = crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    assert!(stale.stale, "a load answers stale");
    assert_eq!(export_impl(&h.handle(), "plans/new.flow").unwrap_err().code, "github_unreachable");
    assert!(!h.dir.path().join("plans/new.flow").exists());
    assert_eq!(h.delivery_settings().source, DeliverySourceChoice::Automatic);
    let layout = load_layout(
        &h.handle(),
        &crate::logging::BUFFER,
        &h.machine_store(),
        "flow:plans/new.flow",
    )
    .unwrap();
    assert!(layout.items.is_empty(), "no layout is copied");
}

/// DSR-FR-RZUH, DSR-FR-JWYO: an export whose settings write fails removes the
/// file it wrote, so the source stays GitHub and a retry does not get
/// `path_exists`. The retry completes.
#[cfg(unix)]
#[test]
fn a_failed_export_leaves_no_file_that_blocks_a_retry() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::github_plan();
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    let settings_dir = h.dir.path().join(".synthesis");
    assert!(settings_dir.is_dir(), "the settings folder exists");
    let before = h.delivery_settings().source;
    let mode = |m: u32| {
        std::fs::set_permissions(&settings_dir, std::fs::Permissions::from_mode(m)).unwrap()
    };
    // The settings folder refuses new files: the settings write fails.
    mode(0o555);
    let failed = export_impl(&h.handle(), "plans/out.flow");
    mode(0o755);
    assert_eq!(failed.unwrap_err().code, ERR_SETTINGS_UNREADABLE);
    assert!(!h.dir.path().join("plans/out.flow").exists(), "the new file is removed");
    assert_eq!(h.delivery_settings().source, before, "the source does not change");
    let handle = h.handle();
    let tracker = handle.state::<crate::artifacts::ContentTracker>();
    assert_eq!(tracker.current("plans/out.flow"), None, "the tracker forgets the file");

    let result = export_impl(&h.handle(), "plans/out.flow").expect("the retry completes");
    assert_eq!(result.flow_id, "plans/out.flow");
    assert!(h.dir.path().join("plans/out.flow").exists());
    assert_eq!(
        h.delivery_settings().source,
        DeliverySourceChoice::File { flow_id: "plans/out.flow".into() }
    );
}

/// DSR-FR-RZUH, DSR-FR-JWYO, DSR-FR-EGFF: an export writes a version 3 Flow
/// with the Project's title as its name and every item and edge, copies the
/// layout, sets the source to the new file, and changes nothing on GitHub.
#[test]
fn an_export_moves_the_plan_to_a_file() {
    let h = Harness::github_plan();
    {
        let mut s = h.fake.lock();
        let i = s.issues.iter_mut().find(|i| i.number == 3).unwrap();
        i.blocked_by.push(link(1, "open"));
    }
    h.fake.board_issue(issue(4, Some("Task")), None);
    h.fake
        .lock()
        .issues
        .iter_mut()
        .find(|i| i.number == 4)
        .unwrap()
        .blocked_by
        .push(link(3, "open"));
    let store = h.machine_store();
    let mut layout = DeliveryLayout::default();
    layout
        .items
        .insert("issue-3".into(), LayoutEntry { collapsed: Some(true), ..Default::default() });
    save_layout(&store, &format!("github:{PROJECT}"), &layout).unwrap();
    let edit = crate::delivery::github::edits::GithubDeliveryEdit::LinkDraft {
        item_id: "issue-4".into(),
        draft_id: "d".into(),
    };
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    crate::delivery::github::edits::apply_edit_impl(&h.handle(), edit).unwrap();
    let overlay_before =
        read_delivery_overlay(&h.handle(), &crate::logging::BUFFER, &store, PROJECT).unwrap();
    let events = listen(&h.handle(), DELIVERY_SOURCE_CHANGED);
    h.fake.lock().calls.clear();

    let result = export_impl(&h.handle(), "plans/out.flow").unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({ "flowId": "plans/out.flow", "path": "plans/out.flow" })
    );
    let body = std::fs::read_to_string(h.dir.path().join("plans/out.flow")).unwrap();
    let doc = parse_valid(&body).expect("a valid Flow");
    assert_eq!((doc.version, doc.name.as_deref()), (3, Some("Roadmap")));
    assert_eq!(doc.items.len(), 5);
    assert_eq!(doc.edges.len(), 1, "issue-3 blocks issue-4");
    assert_eq!(
        doc.items.iter().find(|i| i.id == "issue-4").unwrap().draft_id.as_deref(),
        Some("d")
    );
    assert!(!body.contains("https://"), "no tracker reference");
    let copied =
        load_layout(&h.handle(), &crate::logging::BUFFER, &store, "flow:plans/out.flow").unwrap();
    assert_eq!(copied, layout);
    assert_eq!(
        h.delivery_settings().source,
        DeliverySourceChoice::File { flow_id: "plans/out.flow".into() }
    );
    assert!(h.fake.writes().is_empty(), "nothing changes on GitHub");
    assert_eq!(
        read_delivery_overlay(&h.handle(), &crate::logging::BUFFER, &store, PROJECT).unwrap(),
        overlay_before
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&events.lock().unwrap()[0]).unwrap()["kind"],
        "file"
    );
}

/// DSR-FR-FQZD, DSR-FR-BIYW, FLO-FR-MCFR: milestones of a plan come in
/// milestone order from the document order: the dated ones take the dated
/// places by due date, and an undated one keeps its place. They carry leaf
/// counts and blocked leaves; the first that is not done or dropped is
/// current.
#[test]
fn milestones_of_a_flow_are_ordered_and_counted() {
    let body = json!({ "version": 3, "items": [
        { "id": "m-undated", "kind": "milestone", "name": "U" },
        { "id": "m-late", "kind": "milestone", "name": "L", "due": "2027-02-01" },
        { "id": "m-early", "kind": "milestone", "name": "E", "due": "2027-01-01" },
        { "id": "a", "kind": "task", "name": "A", "parentId": "m-late" },
        { "id": "b", "kind": "task", "name": "B", "parentId": "m-early" },
        { "id": "c", "kind": "task", "name": "C", "parentId": "m-late" }
    ], "edges": [ { "id": "e", "from": "b", "to": "c", "type": "blocks" } ] });
    let doc = parse_valid(&body.to_string()).unwrap();
    let states = crate::flow_delivery::delivery_state(&doc, &[]);
    let out = milestones_of(&doc, &states.items);
    let ids: Vec<&str> = out.iter().map(|m| m.item_id.as_str()).collect();
    assert_eq!(ids, ["m-undated", "m-early", "m-late"], "the undated one keeps its place");
    assert_eq!(out[2].blocked_leaves, 1);
    assert_eq!(out[2].leaf_counts.not_started, 2);
    assert_eq!(
        out.iter().filter(|m| m.current).map(|m| m.item_id.as_str()).collect::<Vec<_>>(),
        ["m-undated"],
        "the first not done or dropped in milestone order"
    );
    let value = serde_json::to_value(&out[1]).unwrap();
    assert_eq!(value["leafCounts"]["notStarted"], 1);
    assert_eq!(value["state"], "not_started");
    assert_eq!(value["blockedLeaves"], 0);
}

/// FLO-FR-MCFR: the dated milestones take the dated places by due date, one
/// date keeps the plan order, and each undated milestone keeps its place.
#[test]
fn the_milestone_order_sorts_the_dated_places_alone() {
    let order = |plan: Vec<(&'static str, Option<&'static str>)>| -> Vec<&'static str> {
        milestone_order(plan, |m| m.1).into_iter().map(|m| m.0).collect()
    };
    assert_eq!(order(vec![]), Vec::<&str>::new());
    assert_eq!(
        order(vec![
            ("a", Some("2027-03-01")),
            ("u1", None),
            ("b", Some("2027-01-01")),
            ("c", Some("2027-02-01")),
            ("u2", None),
            ("d", Some("2027-01-01")),
        ]),
        ["b", "u1", "d", "c", "u2", "a"]
    );
    assert_eq!(order(vec![("u1", None), ("u2", None)]), ["u1", "u2"], "no date: plan order");
    let dated = vec![("x", Some("2027-01-01")), ("y", Some("2027-01-01"))];
    assert_eq!(order(dated), ["x", "y"], "one date: plan order");
}

/// DSR-FR-OJLB, DSR-FR-BIYW, DSR-FR-SKPX: the milestones of each source; a
/// GitHub plan carries `readAt` and `stale`, a Flow file neither; a failed
/// read with no cache and an invalid Flow are refused.
#[test]
fn milestones_are_answered_for_each_source() {
    let h = Harness::new();
    let none = serde_json::to_value(milestones_impl(&h.handle(), false).unwrap()).unwrap();
    assert_eq!(
        none,
        json!({ "source": { "kind": "none", "reason": "no_source" }, "planName": null,
                             "milestones": [], "readAt": null, "stale": false })
    );
    file_source(&h, &flow_body());
    let file = milestones_impl(&h.handle(), false).unwrap();
    assert_eq!(file.plan_name.as_deref(), Some("Delivery"));
    assert_eq!(file.milestones.len(), 1);
    assert_eq!((file.read_at.as_deref(), file.stale), (None, false));
    file_source(&h, "{ \"version\": 2 }");
    assert_eq!(milestones_impl(&h.handle(), false).unwrap_err().code, ERR_FLOW_INVALID);

    let h = Harness::github_plan();
    h.fake.lock().read_error = Some("github_unreachable".into());
    assert_eq!(milestones_impl(&h.handle(), false).unwrap_err().code, "github_unreachable");
    h.fake.lock().read_error = None;
    let github = milestones_impl(&h.handle(), true).unwrap();
    assert_eq!(github.plan_name.as_deref(), Some("Roadmap"));
    assert_eq!(github.milestones[0].item_id, "milestone-1");
    assert!(github.milestones[0].current);
    assert!(github.read_at.is_some());
    h.fake.lock().read_error = Some("github_unreachable".into());
    assert!(milestones_impl(&h.handle(), true).unwrap().stale);
}

/// The records of `buffer`, oldest first.
fn record_list(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap()
        .records
}

/// DSR-FR-LYMF: each operation logs its start, its outcome, its counts, and
/// every error; no record carries the token, a Type name, an item name, a
/// description, or a URL.
#[test]
fn operations_log_without_content() {
    static LOG: LogBuffer = LogBuffer::new();
    let h = Harness::with_buffer(&LOG);
    h.select_project(Some(PROJECT));
    let mut settings = DeliverySettings::default();
    settings.create_types.task = "Chore Type".into();
    settings
        .read_entries
        .push(ReadEntry { type_name: "Odd Type".into(), kind: MappedKind::Exclude });
    h.fake.lock().types.push("Chore Type".into());
    let body = flow_body().replace("Second", "Hidden item name");
    let body =
        body.replacen("\"name\":\"Beta\"", "\"name\":\"Beta\",\"description\":\"Hidden text\"", 1);
    assert!(body.contains("Hidden text"), "the milestone has a description");
    file_source(&h, &body);
    let mut stored = h.delivery_settings();
    stored.create_types = settings.create_types.clone();
    stored.read_entries = settings.read_entries.clone();
    set_settings_impl(&h.handle(), stored).unwrap();
    publish_impl(&h.handle()).unwrap();
    milestones_impl(&h.handle(), false).unwrap();
    h.write("plans/taken.flow", "");
    export_impl(&h.handle(), "plans/taken.flow").unwrap_err();
    export_impl(&h.handle(), "plans/out.flow").unwrap();

    let list = record_list(&LOG);
    let one = |message: &str| {
        list.iter()
            .find(|r| r.message == message)
            .unwrap_or_else(|| panic!("no record {message:?}"))
            .clone()
    };
    let field = |r: &crate::logging::LogRecord, key: &str| r.fields.get(key).cloned();
    for start in
        ["delivery settings write started", "delivery publish started", "delivery export started"]
    {
        one(start);
    }
    let saved = one("delivery settings saved");
    assert_eq!(field(&saved, "read_entries"), Some(json!(1)));
    let published = one("delivery published");
    assert_eq!(published.level, LogLevel::Info);
    assert_eq!(
        (field(&published, "milestones"), field(&published, "issues")),
        (Some(json!(1)), Some(json!(3)))
    );
    assert_eq!(field(&published, "relations"), Some(json!(1)));
    assert!(field(&published, "ms").is_some());
    one("delivery milestones loaded");
    // The overlay, layout, and journal records of a publish and an export
    // go to the buffer of the operation too. A successful layout and journal
    // read logs nothing; the overlay write logs.
    one("delivery overlay written");
    let exported = one("delivery exported");
    assert_eq!(field(&exported, "items"), Some(json!(4)));
    let refused = list
        .iter()
        .find(|r| {
            r.message == "delivery source operation refused"
                && field(r, "operation") == Some(json!("export_delivery_to_file"))
        })
        .expect("the refused export is logged");
    assert_eq!(
        (refused.level, field(refused, "code")),
        (LogLevel::Warn, Some(json!(ERR_PATH_EXISTS)))
    );
    let text = serde_json::to_string(&list).unwrap();
    for (index, marker) in [
        SECRET,
        "Chore Type",
        "Odd Type",
        "Hidden item name",
        "Hidden text",
        "specs/a.md",
        "https://",
    ]
    .iter()
    .enumerate()
    {
        assert!(!text.contains(marker), "a record holds sentinel {index}");
    }
}

/// FDL-FR-JLUJ, FDL-FR-JLXF, FDL-FR-BDOD: with a GitHub source,
/// `find_flow_items_for_draft` also searches the plan of the read cache,
/// without a request, and its results come after those of the Flow files. A
/// leaf matches through its draft link, or through its issue where the draft
/// is the shadow draft of that issue. A GitHub result has no artifact id, the
/// Project title, and `delivery` true.
#[test]
fn the_github_cache_is_searched_for_a_draft() {
    let h = Harness::github_plan();
    h.fake.board_issue(issue(4, Some("Task")), None);
    let root = h.root();
    let linked = crate::drafts::create_draft_impl(&root, Some("Linked"), None).unwrap().draft.id;
    let shadow = crate::drafts::create_github_shadow_draft(
        &root,
        "Shadow",
        "",
        crate::drafts::GithubIssueLink {
            repository_host: "github.com".into(),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            issue_number: 3,
            issue_url: "https://github.com/acme/widgets/issues/3".into(),
            project_node_id: PROJECT.into(),
            claim_state: crate::drafts::GithubClaimState::Claimed,
        },
    )
    .unwrap()
    .draft
    .id;
    h.write(
        "plans/side.flow",
        &json!({ "version": 3, "name": "Side", "items": [
            { "id": "s1", "kind": "task", "name": "Side task", "draftId": linked }
        ], "edges": [] })
        .to_string(),
    );
    let find = |id: &str| {
        serde_json::to_value(crate::flow_delivery::find_items_for_draft_in(&h.handle(), &root, id))
            .unwrap()
    };
    assert_eq!(
        find(&linked),
        json!([{ "flowId": "plans/side.flow", "flowName": "Side", "delivery": false,
                 "itemId": "s1", "itemName": "Side task", "kind": "task" }]),
        "no cache: the Flow files alone"
    );
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    let edit = crate::delivery::github::edits::GithubDeliveryEdit::LinkDraft {
        item_id: "issue-4".into(),
        draft_id: linked.clone(),
    };
    crate::delivery::github::edits::apply_edit_impl(&h.handle(), edit).unwrap();
    h.fake.lock().calls.clear();
    assert_eq!(
        find(&linked),
        json!([
            { "flowId": "plans/side.flow", "flowName": "Side", "delivery": false,
              "itemId": "s1", "itemName": "Side task", "kind": "task" },
            { "flowId": null, "flowName": "Roadmap", "delivery": true,
              "itemId": "issue-4", "itemName": "Issue 4", "kind": "task" }
        ]),
        "the GitHub plan comes after the Flow files"
    );
    assert_eq!(
        find(&shadow),
        json!([{ "flowId": null, "flowName": "Roadmap", "delivery": true, "itemId": "issue-3",
                 "itemName": "Issue 3", "kind": "task" }]),
        "issue-3 matches through its issue"
    );
    assert_eq!(find("other"), json!([]));
    assert!(h.fake.calls().is_empty(), "no request: {:?}", h.fake.calls());
    // A file source searches no GitHub plan.
    file_source(&h, "");
    assert!(crate::delivery::github::cached_delivery_plan(&h.handle(), &root).is_none());
    assert_eq!(
        find(&linked),
        json!([{ "flowId": "plans/side.flow", "flowName": "Side", "delivery": false,
                 "itemId": "s1", "itemName": "Side task", "kind": "task" }]),
        "the Flow files alone"
    );
}

/// DSR-FR-NFFP, DSR-FR-CPYM: a change of the source choice moves no plan
/// data: the read cache, the overlay, and the Flow file stay as they are. No
/// settings write, valid or refused, makes a GitHub request.
#[test]
fn a_source_change_moves_no_plan_data() {
    let h = Harness::github_plan();
    crate::delivery::github::load_plan_impl(&h.handle(), true).unwrap();
    let link = crate::delivery::github::edits::GithubDeliveryEdit::LinkDraft {
        item_id: "issue-3".into(),
        draft_id: "d-1".into(),
    };
    crate::delivery::github::edits::apply_edit_impl(&h.handle(), link).unwrap();
    h.write(FLOW, &flow_body());
    let store = h.machine_store();
    let snapshot = |h: &Harness| {
        let cache = std::fs::read_to_string(
            h.store.path().join("delivery/github").join(format!("{PROJECT}.json")),
        )
        .unwrap();
        let overlay =
            read_delivery_overlay(&h.handle(), &crate::logging::BUFFER, &store, PROJECT).unwrap();
        let flow = std::fs::read_to_string(h.dir.path().join(FLOW)).unwrap();
        (cache, overlay, flow)
    };
    let before = snapshot(&h);
    h.fake.lock().calls.clear();
    let to_file = DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: FLOW.into() },
        ..DeliverySettings::default()
    };
    set_settings_impl(&h.handle(), to_file).unwrap();
    assert_eq!(snapshot(&h), before, "the file choice moves nothing");
    let to_github =
        DeliverySettings { source: DeliverySourceChoice::Github, ..DeliverySettings::default() };
    set_settings_impl(&h.handle(), to_github).unwrap();
    assert_eq!(snapshot(&h), before, "the github choice moves nothing");
    let invalid = DeliverySettings {
        create_types: CreateTypes { task: "Epic".into(), ..CreateTypes::default() },
        ..DeliverySettings::default()
    };
    assert_eq!(set_settings_impl(&h.handle(), invalid).unwrap_err().code, ERR_INVALID_SETTINGS);
    let unknown_type = DeliverySettings {
        create_types: CreateTypes { task: "Not On GitHub".into(), ..CreateTypes::default() },
        ..DeliverySettings::default()
    };
    set_settings_impl(&h.handle(), unknown_type).expect("a Type is not checked against GitHub");
    assert!(h.fake.calls().is_empty(), "no GitHub request: {:?}", h.fake.calls());
}
