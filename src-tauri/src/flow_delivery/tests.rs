//! The tests of the delivery state of a plan (`FDL-flow-delivery.md`).

use serde_json::{json, Value};

use super::*;
use crate::drafts::{BuildState, GithubClaimState, GithubIssueLink};
use crate::graduation::DraftGraduation;

pub(crate) fn draft(
    id: &str,
    status: DraftStatus,
    run: Option<GraduationRunState>,
) -> DraftSummary {
    DraftSummary {
        id: id.into(),
        name: format!("draft {id}"),
        status,
        folder: String::new(),
        inconsistent: false,
        updated_at: "2026-01-01T00:00:00Z".into(),
        prompt_activity_at: None,
        build: BuildState::Idle,
        has_pending_proposal: false,
        graduation: run.map(|state| DraftGraduation {
            run_id: format!("run-{id}"),
            state,
            locked: run_is_open(state),
            graduated: state == GraduationRunState::Completed,
            openspec_change: None,
        }),
        github_issue: None,
        publication: None,
        sequence: None,
        superseded: None,
        publication_unreadable: false,
    }
}

pub(super) fn container(id: &str, kind: &str, parent: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": kind, "name": id });
    if let Some(p) = parent {
        v["parentId"] = json!(p);
    }
    v
}

pub(super) fn task(id: &str, parent: &str, draft: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": "task", "name": id, "parentId": parent });
    if let Some(d) = draft {
        v["draftId"] = json!(d);
    }
    v
}

pub(super) fn doc(items: Vec<Value>, edges: Value) -> FlowDocument {
    let body = json!({ "version": 3, "items": items, "edges": edges });
    parse_valid(&body.to_string()).unwrap_or_else(|r| panic!("fixture invalid: {:?}", r.violations))
}

pub(super) fn skeleton(leaves: Vec<Value>) -> Vec<Value> {
    let mut items = vec![
        container("m1", "milestone", None),
        container("f1", "feature", Some("m1")),
        container("f2", "feature", Some("m1")),
    ];
    items.extend(leaves);
    items
}

pub(super) fn state_of<'a>(s: &'a FlowDeliveryState, id: &str) -> &'a ItemDeliveryState {
    s.items.iter().find(|i| i.item_id == id).unwrap()
}

/// FDL-FR-VGZI, FDL-FR-RMYL: a leaf's state from its draft and its run.
#[test]
fn leaf_states_follow_draft_and_run() {
    let d = doc(
        skeleton(vec![
            task("t0", "f1", None),
            task("t1", "f1", Some("a")),
            task("t2", "f1", Some("b")),
            task("t3", "f1", Some("c")),
            task("t4", "f1", Some("e")),
            task("t5", "f2", Some("f")),
        ]),
        json!([]),
    );
    let drafts = vec![
        draft("a", DraftStatus::Active, None),
        draft("b", DraftStatus::Active, Some(GraduationRunState::Working)),
        draft("c", DraftStatus::Graduated, Some(GraduationRunState::Completed)),
        draft("e", DraftStatus::Archived, None),
        draft("f", DraftStatus::Active, Some(GraduationRunState::Failed)),
    ];
    let s = delivery_state(&d, &drafts);
    let got: Vec<_> =
        ["t0", "t1", "t2", "t3", "t4", "t5"].iter().map(|id| state_of(&s, id).state).collect();
    assert_eq!(
        got,
        vec![
            ItemState::NotStarted,
            ItemState::Drafting,
            ItemState::InProgress,
            ItemState::Done,
            ItemState::Dropped,
            ItemState::Drafting,
        ]
    );
    let t2 = state_of(&s, "t2");
    assert_eq!(t2.draft.as_ref().unwrap().name, "draft b");
    assert_eq!(t2.run.as_ref().unwrap().state, GraduationRunState::Working);
    assert!(state_of(&s, "t0").run.is_none());
}

/// FDL-FR-TTZZ, FDL-FR-BRAQ: a draftId naming no draft is reported missing,
/// and a leaf of a Flow file with no draftId has no draft, even where a
/// GitHub-shadow draft exists.
#[test]
fn missing_drafts_and_no_shadow_for_a_file() {
    let d = doc(skeleton(vec![task("t1", "f1", None), task("t2", "f1", Some("gone"))]), json!([]));
    let s = delivery_state(&d, &[shadow_draft("s", 51)]);
    assert!(state_of(&s, "t1").draft.is_none());
    assert_eq!(state_of(&s, "t1").state, ItemState::NotStarted);
    let t2 = state_of(&s, "t2");
    assert!(t2.draft_missing && t2.draft.is_none());
    assert_eq!(t2.state, ItemState::NotStarted);
}

/// A GitHub-shadow draft of the issue `acme/pay#number` on `GitHub.com`.
pub(super) fn shadow_draft(id: &str, number: u64) -> DraftSummary {
    let mut shadow = draft(id, DraftStatus::GithubShadow, None);
    shadow.github_issue = Some(GithubIssueLink {
        repository_host: "GitHub.com".into(),
        repository_owner: "acme".into(),
        repository_name: "pay".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/pay/issues/{number}"),
        project_node_id: "P".into(),
        claim_state: GithubClaimState::Claimed,
    });
    shadow
}

/// FDL-FR-JYKC, FDL-FR-IJXW: a container's state and its leaf counts.
#[test]
fn container_states_and_counts() {
    let d = doc(
        skeleton(vec![
            task("t1", "f1", Some("a")),
            task("t2", "f1", Some("b")),
            task("t3", "f2", Some("c")),
        ]),
        json!([]),
    );
    let drafts = vec![
        draft("a", DraftStatus::Graduated, None),
        draft("b", DraftStatus::Archived, None),
        draft("c", DraftStatus::Archived, None),
    ];
    let s = delivery_state(&d, &drafts);
    assert_eq!(state_of(&s, "f1").state, ItemState::Done, "done and dropped leaves");
    assert_eq!(state_of(&s, "f2").state, ItemState::Dropped, "only dropped leaves");
    assert_eq!(state_of(&s, "m1").state, ItemState::Done);
    let counts = state_of(&s, "m1").leaf_counts.clone().unwrap();
    assert_eq!((counts.done, counts.dropped, counts.not_started), (1, 2, 0));
    assert!(state_of(&s, "t1").leaf_counts.is_none());

    let empty = doc(vec![container("m9", "milestone", None)], json!([]));
    assert_eq!(state_of(&delivery_state(&empty, &[]), "m9").state, ItemState::NotStarted);

    let mixed = doc(skeleton(vec![task("t1", "f1", None), task("t2", "f1", Some("a"))]), json!([]));
    let s = delivery_state(&mixed, &[draft("a", DraftStatus::Graduated, None)]);
    assert_eq!(state_of(&s, "f1").state, ItemState::InProgress);
}

/// FDL-FR-JYKC: a container with no done leaf is `in_progress` when a leaf is
/// `drafting` or `in_progress`: a `drafting` leaf with an `in_progress` leaf,
/// each of them with a `not_started` leaf, and each of them alone.
#[test]
fn a_container_with_work_but_no_done_leaf_is_in_progress() {
    let running = Some(GraduationRunState::Working);
    // Each case: its name, and each leaf with its draft or none.
    type Leaves = Vec<(&'static str, Option<DraftSummary>)>;
    let cases: Vec<(&str, Leaves)> = vec![
        (
            "drafting and in_progress",
            vec![
                ("a", Some(draft("a", DraftStatus::Active, None))),
                ("b", Some(draft("b", DraftStatus::Active, running))),
            ],
        ),
        (
            "drafting and not_started",
            vec![("a", Some(draft("a", DraftStatus::Active, None))), ("b", None)],
        ),
        (
            "in_progress and not_started",
            vec![("a", Some(draft("a", DraftStatus::Active, running))), ("b", None)],
        ),
        (
            "in_progress and dropped",
            vec![
                ("a", Some(draft("a", DraftStatus::Active, running))),
                ("b", Some(draft("b", DraftStatus::Archived, None))),
            ],
        ),
        ("drafting alone", vec![("a", Some(draft("a", DraftStatus::Active, None)))]),
        ("in_progress alone", vec![("a", Some(draft("a", DraftStatus::Active, running)))]),
    ];
    for (name, leaves) in cases {
        let items: Vec<Value> = leaves
            .iter()
            .map(|(id, d)| task(&format!("t{id}"), "f1", d.as_ref().map(|_| *id)))
            .collect();
        let drafts: Vec<DraftSummary> = leaves.into_iter().filter_map(|(_, d)| d).collect();
        let s = delivery_state(&doc(skeleton(items), json!([])), &drafts);
        let leaf_states: Vec<ItemState> =
            s.items.iter().filter(|i| i.item_id.starts_with('t')).map(|i| i.state).collect();
        assert!(!leaf_states.contains(&ItemState::Done), "{name}: no done leaf");
        assert_eq!(state_of(&s, "f1").state, ItemState::InProgress, "{name}");
        assert_eq!(state_of(&s, "m1").state, ItemState::InProgress, "{name}: at any depth");
    }
}

/// FDL-FR-PXHL, FDL-FR-PRED: blocked by an edge into the item or an ancestor
/// whose `from` is not done or dropped; relates edges never block.
#[test]
fn blocked_items() {
    let d = doc(
        skeleton(vec![
            task("t1", "f1", Some("a")),
            task("t2", "f2", None),
            task("t3", "f2", Some("c")),
        ]),
        json!([
            { "id": "e1", "from": "f1", "to": "f2", "type": "blocks" },
            { "id": "e2", "from": "t1", "to": "t2", "type": "blocks" }
        ]),
    );
    let drafts =
        vec![draft("a", DraftStatus::Active, None), draft("c", DraftStatus::Graduated, None)];
    let s = delivery_state(&d, &drafts);
    let t2 = state_of(&s, "t2");
    assert!(t2.blocked);
    assert_eq!(
        t2.blocked_by,
        vec!["f1".to_string(), "t1".to_string()],
        "each blocker once, edge order"
    );
    assert!(!state_of(&s, "t3").blocked, "a done item is never blocked");
    assert!(state_of(&s, "f2").blocked);
    assert!(!state_of(&s, "t1").blocked);

    let done =
        vec![draft("a", DraftStatus::Archived, None), draft("c", DraftStatus::Graduated, None)];
    let s = delivery_state(&d, &done);
    assert!(!state_of(&s, "t2").blocked, "a dropped blocker blocks nothing");

    let related = doc(
        skeleton(vec![task("t1", "f1", None), task("t2", "f2", None)]),
        json!([{ "id": "e1", "from": "t1", "to": "t2", "type": "relates" }]),
    );
    assert!(!state_of(&delivery_state(&related, &[]), "t2").blocked);
}

/// FDL-FR-PBSD: the same inputs give the same answer, in document order.
#[test]
fn pure_and_in_document_order() {
    let d = doc(skeleton(vec![task("t1", "f1", None)]), json!([]));
    let a = delivery_state(&d, &[]);
    assert_eq!(a, delivery_state(&d, &[]));
    let ids: Vec<_> = a.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids, vec!["m1", "f1", "f2", "t1"]);
}

/// FDL-FR-BDOD, FDL-FR-JLXF: the items a draft is worked for, across Flows in
/// path order, skipping invalid Flows (a version 2 Flow among them). Each
/// result carries the Flow's artifact id and name, the item's id, name, and
/// kind, and whether the Flow is the project's delivery.
#[test]
fn items_for_a_draft_across_flows() {
    let body = |name: &str, draft_id: &str| {
        let mut items = skeleton(vec![task("t1", "f1", Some(draft_id))]);
        items.push(task("t9", "f2", None));
        json!({ "version": 3, "name": name, "items": items, "edges": [] }).to_string()
    };
    let flows = vec![
        ("a.flow".to_string(), body("Alpha", "x")),
        ("b.flow".to_string(), body("Beta", "x").replace("\"version\":3", "\"version\":2")),
        ("c.flow".to_string(), body("Gamma", "x")),
        ("d.flow".to_string(), body("Delta", "y")),
    ];
    let drafts = vec![draft("x", DraftStatus::Active, None), draft("y", DraftStatus::Active, None)];
    let got = items_for_draft(&flows, &drafts, "x", Some("c.flow"));
    assert_eq!(
        got.iter()
            .map(|m| (
                m.flow_id.as_deref(),
                m.flow_name.as_str(),
                m.item_id.as_str(),
                m.kind,
                m.delivery
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("a.flow"), "Alpha", "t1", "task", false),
            (Some("c.flow"), "Gamma", "t1", "task", true)
        ]
    );
    assert_eq!(got[0].item_name, "t1");
    assert!(items_for_draft(&flows, &drafts, "nope", None).is_empty());
    assert!(items_for_draft(&flows, &drafts, "x", None).iter().all(|m| !m.delivery));
}

/// FDL-FR-JLXF: the serde form of a result carries `flowId`, `flowName`,
/// `delivery`, `itemId`, `itemName`, and `kind`, and a result with no artifact
/// id carries `flowId: null`.
#[test]
fn a_match_serializes_with_a_nullable_flow_id() {
    let m = FlowItemMatch {
        flow_id: None,
        flow_name: "Payments".into(),
        delivery: true,
        item_id: "t1".into(),
        item_name: "Tokenize".into(),
        kind: "task",
    };
    assert_eq!(
        serde_json::to_value(&m).unwrap(),
        json!({ "flowId": null, "flowName": "Payments", "delivery": true, "itemId": "t1",
                "itemName": "Tokenize", "kind": "task" })
    );
}

/// FDL-FR-AZQM: an invalid body is refused with the report; FDL-FR-VTOQ: the
/// no-project refusal carries its code.
#[test]
fn errors_carry_their_codes() {
    let err = FlowReadError::invalid(parse_valid(r#"{"version":1}"#).unwrap_err());
    assert_eq!(err.code, "flow_invalid");
    assert_eq!(err.report.unwrap().violations[0].code, "unknown_version");
    assert_eq!(FlowReadError::no_project().code, "no_project_open");
    assert!(delivery_state(&parse_valid("").unwrap(), &[]).items.is_empty());
}

/// FDL-FR-JLXF, DSR-FR-HOEW: the Flow file that is the delivery is the one
/// that the stored delivery settings choose with `file`. The choices
/// `automatic` and `github`, and a store that cannot be read, choose no file.
#[test]
fn the_delivery_file_comes_from_the_stored_settings() {
    use crate::delivery::settings::DeliverySettings;
    let app = tauri::test::mock_app();
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    assert_eq!(delivery_flow_id(app.handle(), &root), None, "no stored choice");

    let mut settings = DeliverySettings {
        source: DeliverySourceChoice::File { flow_id: "plans/beta.flow".into() },
        ..DeliverySettings::default()
    };
    crate::project_settings::save_delivery_settings_to(&root, &settings).unwrap();
    assert_eq!(delivery_flow_id(app.handle(), &root).as_deref(), Some("plans/beta.flow"));

    settings.source = DeliverySourceChoice::Github;
    crate::project_settings::save_delivery_settings_to(&root, &settings).unwrap();
    assert_eq!(delivery_flow_id(app.handle(), &root), None);

    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = [valid").unwrap();
    assert_eq!(delivery_flow_id(app.handle(), &root), None, "an unreadable store");
}
