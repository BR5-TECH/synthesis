//! The tests of the delivery state of a plan (`FDL-flow-delivery.md`).

use serde_json::{json, Value};

use super::*;
use crate::drafts::{BuildState, GithubClaimState, GithubIssueLink};
use crate::graduation::DraftGraduation;

fn draft(id: &str, status: DraftStatus, run: Option<GraduationRunState>) -> DraftSummary {
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
        }),
        github_issue: None,
        publication: None,
        sequence: None,
        superseded: None,
        publication_unreadable: false,
    }
}

fn container(id: &str, kind: &str, parent: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": kind, "name": id, "position": {"x":0,"y":0},
                        "size": {"width":10,"height":10} });
    if let Some(p) = parent {
        v["parentId"] = json!(p);
    }
    v
}

fn task(id: &str, parent: &str, draft: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": "task", "name": id, "parentId": parent, "position": {"x":0,"y":0} });
    if let Some(d) = draft {
        v["draftId"] = json!(d);
    }
    v
}

fn doc(items: Vec<Value>, edges: Value, sync: Option<Value>) -> FlowDocument {
    let mut body = json!({ "version": 2, "items": items, "edges": edges });
    if let Some(s) = sync {
        body["sync"] = s;
    }
    parse_valid(&body.to_string()).unwrap_or_else(|r| panic!("fixture invalid: {:?}", r.violations))
}

fn skeleton(leaves: Vec<Value>) -> Vec<Value> {
    let mut items = vec![
        container("m1", "milestone", None),
        container("f1", "feature", Some("m1")),
        container("f2", "feature", Some("m1")),
    ];
    items.extend(leaves);
    items
}

fn state_of<'a>(s: &'a FlowDeliveryState, id: &str) -> &'a ItemDeliveryState {
    s.items.iter().find(|i| i.item_id == id).unwrap()
}

fn issue_link(item: &str, number: u64, state: &str) -> Value {
    json!({ "itemId": item,
        "ref": { "kind": "issue", "host": "github.com", "owner": "acme", "repo": "pay", "number": number,
                 "url": format!("https://github.com/acme/pay/issues/{number}") },
        "base": { "name": item, "description": "", "state": state } })
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
        None,
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

/// FDL-FR-VGZI: a closed tracker item is done, whatever its draft says.
#[test]
fn closed_on_tracker_is_done() {
    let d = doc(
        skeleton(vec![task("t1", "f1", Some("a"))]),
        json!([]),
        Some(
            json!({ "tracker": "github", "links": [issue_link("t1", 7, "closed"), issue_link("f2", 8, "closed")] }),
        ),
    );
    let s =
        delivery_state(&d, &[draft("a", DraftStatus::Active, Some(GraduationRunState::Working))]);
    assert_eq!(state_of(&s, "t1").state, ItemState::Done);
    assert_eq!(
        state_of(&s, "f2").state,
        ItemState::Done,
        "FDL-FR-JYKC: a closed container is done"
    );
}

/// FDL-FR-TTZZ, FDL-FR-BRAQ: a GitHub-shadow draft is found by its issue; a
/// draftId naming no draft is reported missing.
#[test]
fn shadow_drafts_and_missing_drafts() {
    let d = doc(
        skeleton(vec![task("t1", "f1", None), task("t2", "f1", Some("gone"))]),
        json!([]),
        Some(json!({ "tracker": "github", "links": [issue_link("t1", 51, "open")] })),
    );
    let mut shadow = draft("s", DraftStatus::GithubShadow, None);
    shadow.github_issue = Some(GithubIssueLink {
        repository_host: "GitHub.com".into(),
        repository_owner: "acme".into(),
        repository_name: "pay".into(),
        issue_number: 51,
        issue_url: "https://github.com/acme/pay/issues/51".into(),
        project_node_id: "P".into(),
        claim_state: GithubClaimState::Claimed,
    });
    let s = delivery_state(&d, &[shadow]);
    assert_eq!(state_of(&s, "t1").draft.as_ref().unwrap().id, "s");
    assert_eq!(state_of(&s, "t1").state, ItemState::Drafting);
    let t2 = state_of(&s, "t2");
    assert!(t2.draft_missing && t2.draft.is_none());
    assert_eq!(t2.state, ItemState::NotStarted);
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
        None,
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

    let empty = doc(vec![container("m9", "milestone", None)], json!([]), None);
    assert_eq!(state_of(&delivery_state(&empty, &[]), "m9").state, ItemState::NotStarted);

    let mixed =
        doc(skeleton(vec![task("t1", "f1", None), task("t2", "f1", Some("a"))]), json!([]), None);
    let s = delivery_state(&mixed, &[draft("a", DraftStatus::Graduated, None)]);
    assert_eq!(state_of(&s, "f1").state, ItemState::InProgress);
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
        None,
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
        None,
    );
    assert!(!state_of(&delivery_state(&related, &[]), "t2").blocked);
}

/// FDL-FR-PBSD: the same inputs give the same answer, in document order.
#[test]
fn pure_and_in_document_order() {
    let d = doc(skeleton(vec![task("t1", "f1", None)]), json!([]), None);
    let a = delivery_state(&d, &[]);
    assert_eq!(a, delivery_state(&d, &[]));
    let ids: Vec<_> = a.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids, vec!["m1", "f1", "f2", "t1"]);
}

/// FDL-FR-BDOD, FDL-FR-JLXF: the items a draft is worked for, across Flows in
/// path order, skipping invalid Flows, and matching shadow drafts by issue.
#[test]
fn items_for_a_draft_across_flows() {
    let body = |name: &str, draft_id: &str| {
        let mut items = skeleton(vec![task("t1", "f1", Some(draft_id))]);
        items.push(task("t9", "f2", None));
        json!({ "version": 2, "name": name, "items": items, "edges": [] }).to_string()
    };
    let flows = vec![
        ("a.flow".to_string(), body("Alpha", "x")),
        ("b.flow".to_string(), r#"{"version":1}"#.to_string()),
        ("c.flow".to_string(), body("Gamma", "x")),
        ("d.flow".to_string(), body("Delta", "y")),
    ];
    let drafts = vec![draft("x", DraftStatus::Active, None), draft("y", DraftStatus::Active, None)];
    let got = items_for_draft(&flows, &drafts, "x");
    assert_eq!(
        got.iter()
            .map(|m| (m.flow_id.as_str(), m.flow_name.as_str(), m.item_id.as_str(), m.kind))
            .collect::<Vec<_>>(),
        vec![("a.flow", "Alpha", "t1", "task"), ("c.flow", "Gamma", "t1", "task")]
    );
    assert_eq!(got[0].item_name, "t1");
    assert!(items_for_draft(&flows, &drafts, "nope").is_empty());
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
