//! The tests of the structural rules of a Flow document and of its typed model
//! (`../../specifications/core/FGV-flow-graph-validation.md`).

use serde_json::{json, Value};

use super::model::{parse_valid, EdgeType, Hierarchy, Kind, WaitGraph};
use super::tests::{codes, doc, edge, item, CANONICAL};
use super::*;

/// A valid skeleton: deliverable d1 > milestone m1 > features f1, f2, f3, each
/// with one task (t1, t2, t3).
fn skeleton() -> Vec<Value> {
    vec![
        item("d1", "deliverable", None),
        item("m1", "milestone", Some("d1")),
        item("f1", "feature", Some("m1")),
        item("f2", "feature", Some("m1")),
        item("f3", "feature", Some("m1")),
        item("t1", "task", Some("f1")),
        item("t2", "task", Some("f2")),
        item("t3", "task", Some("f3")),
    ]
}

fn plan(edges: Value) -> String {
    doc(Value::Array(skeleton()), edges)
}

fn edge_codes(body: &str) -> Vec<(&'static str, String)> {
    validate(body)
        .violations
        .iter()
        .filter_map(|v| v.edge_id.clone().map(|e| (v.code, e)))
        .collect()
}

/// FGV-FR-09: one id space for items, and one for edges.
#[test]
fn duplicate_ids() {
    let mut items = skeleton();
    items.push(item("t1", "defect", Some("f1")));
    let body = doc(
        Value::Array(items),
        json!([edge("e1", "t1", "t2", "relates"), edge("e1", "t2", "t3", "relates")]),
    );
    assert_eq!(codes(&body), vec!["duplicate_id", "duplicate_id"]);
}

/// FGV-FR-10.
#[test]
fn dangling_edge_endpoints() {
    let body = plan(json!([
        edge("e1", "t1", "ghost", "blocks"),
        edge("e2", "ghost", "spirit", "relates")
    ]));
    assert_eq!(
        edge_codes(&body),
        vec![
            ("dangling_edge_endpoint", "e1".into()),
            ("dangling_edge_endpoint", "e2".into()),
            ("dangling_edge_endpoint", "e2".into()),
        ]
    );
}

/// FGV-FR-11: a self-edge, and more than one edge between two items whatever
/// the edges' directions and types.
#[test]
fn self_and_duplicate_edges() {
    let body = plan(json!([
        edge("e1", "t1", "t1", "relates"),
        edge("e2", "t1", "t2", "blocks"),
        edge("e3", "t2", "t1", "relates"),
        edge("e4", "t1", "t2", "relates"),
    ]));
    assert_eq!(
        edge_codes(&body),
        vec![
            ("self_edge", "e1".into()),
            ("duplicate_edge", "e3".into()),
            ("duplicate_edge", "e4".into()),
        ]
    );
}

/// FGV-FR-13: a parent naming nothing or a leaf is dangling; a chain of parents
/// that closes on itself is a containment cycle on each item in it.
#[test]
fn dangling_parents_and_containment_cycles() {
    let mut items = skeleton();
    items.push(item("t4", "task", Some("t1")));
    items.push(item("t5", "task", Some("nothing")));
    let body = doc(Value::Array(items), json!([]));
    assert_eq!(codes(&body), vec!["dangling_parent", "dangling_parent"]);

    let body =
        doc(json!([item("e1", "epic", Some("e2")), item("e2", "epic", Some("e1"))]), json!([]));
    assert_eq!(codes(&body), vec!["containment_cycle", "containment_cycle"]);
}

/// FGV-FR-YHFX: every kind in every container, against the permitted table.
#[test]
fn hierarchy_table() {
    let kinds = ["deliverable", "milestone", "epic", "feature", "task", "defect"];
    let permitted: &[(&str, &[&str])] = &[
        ("deliverable", &["top"]),
        ("milestone", &["deliverable"]),
        ("epic", &["milestone"]),
        ("feature", &["milestone", "epic"]),
        ("task", &["feature"]),
        ("defect", &["feature", "epic", "milestone"]),
    ];
    for (kind, allowed) in permitted {
        for container in ["top", "deliverable", "milestone", "epic", "feature"] {
            let mut items = vec![];
            let parent = if container == "top" {
                None
            } else {
                items.push(item("c", container, None));
                Some("c")
            };
            items.push(item("x", kind, parent));
            let report = validate(&doc(Value::Array(items), json!([])));
            let refused = report
                .violations
                .iter()
                .any(|v| v.code == "invalid_parent_kind" && v.element_id.as_deref() == Some("x"));
            assert_eq!(refused, !allowed.contains(&container), "{kind} in {container}");
        }
    }
    assert_eq!(kinds.len(), permitted.len());
}

/// FGV-FR-KBLT: an edge between an item and its ancestor or descendant.
#[test]
fn edges_within_a_lineage() {
    let body = plan(json!([
        edge("e1", "f1", "t1", "blocks"),
        edge("e2", "t2", "d1", "relates"),
        edge("e3", "f1", "t2", "blocks"),
    ]));
    assert_eq!(
        edge_codes(&body),
        vec![("edge_within_lineage", "e1".into()), ("edge_within_lineage", "e2".into())]
    );
}

/// FGV-FR-PMTY: a direct cycle flags each of its edges.
#[test]
fn direct_dependency_cycle() {
    let body = plan(json!([
        edge("e1", "t1", "t2", "blocks"),
        edge("e2", "t2", "t3", "blocks"),
        edge("e3", "t3", "t1", "blocks"),
    ]));
    assert_eq!(
        edge_codes(&body),
        vec![
            ("dependency_cycle", "e1".into()),
            ("dependency_cycle", "e2".into()),
            ("dependency_cycle", "e3".into()),
        ]
    );
}

/// FGV-FR-PMTY: a cycle that runs through the hierarchy. `t1` (in `f1`) blocks
/// the feature `f2`, and `t2` (in `f2`) blocks the feature `f1`: each task waits
/// for the other.
#[test]
fn dependency_cycle_through_the_hierarchy() {
    let body = plan(json!([edge("e1", "t1", "f2", "blocks"), edge("e2", "t2", "f1", "blocks")]));
    assert_eq!(
        edge_codes(&body),
        vec![("dependency_cycle", "e1".into()), ("dependency_cycle", "e2".into())]
    );

    // The same shape that does not close: f1 blocks f2, and t2 blocks t3.
    let body = plan(json!([edge("e1", "f1", "f2", "blocks"), edge("e2", "t2", "t3", "blocks")]));
    assert!(validate(&body).valid, "{:?}", validate(&body).violations);

    // A blocks edge out of a container into a sibling's leaf, and back to the
    // container, closes.
    let body = plan(json!([edge("e1", "f1", "t2", "blocks"), edge("e2", "t2", "f1", "relates")]));
    assert!(edge_codes(&body).iter().any(|(c, _)| *c == "duplicate_edge"));
    let body = plan(json!([edge("e1", "f1", "t2", "blocks"), edge("e2", "f2", "t1", "blocks")]));
    assert_eq!(
        edge_codes(&body),
        vec![("dependency_cycle", "e1".into()), ("dependency_cycle", "e2".into())]
    );
}

/// FGV-FR-PMTY: only the edges on the cycle are flagged, not the ones that lead
/// into it.
#[test]
fn only_cycle_edges_are_flagged() {
    let body = plan(json!([
        edge("e0", "t3", "t1", "blocks"),
        edge("e1", "t1", "t2", "blocks"),
        edge("e2", "t2", "t1", "relates"),
    ]));
    assert!(edge_codes(&body).iter().all(|(c, _)| *c != "dependency_cycle"));
}

/// FGV-FR-CTFX.
#[test]
fn duplicate_draft_links() {
    let mut items = skeleton();
    items[5]["draftId"] = json!("dr1");
    items[6]["draftId"] = json!("dr1");
    items[7]["draftId"] = json!("dr2");
    let report = validate(&doc(Value::Array(items), json!([])));
    let got: Vec<_> =
        report.violations.iter().map(|v| (v.code, v.element_id.clone().unwrap())).collect();
    assert_eq!(got, vec![("duplicate_draft_link", "t2".to_string())]);
}

fn link(item_id: &str, kind: &str, number: u64) -> Value {
    let base = if kind == "milestone" {
        json!({ "name": "x", "description": "", "state": "open" })
    } else {
        json!({ "name": "x", "description": "", "state": "open", "blockedBy": [] })
    };
    json!({ "itemId": item_id,
        "ref": { "kind": kind, "host": "github.com", "owner": "o", "repo": "r", "number": number,
                 "url": format!("https://github.com/o/r/issues/{number}") },
        "base": base })
}

/// FGV-FR-LWPD: duplicate links, by item and by reference, and kind mismatches.
#[test]
fn sync_link_rules() {
    let body = |links: Value| {
        json!({ "version": 2, "items": skeleton(), "edges": [],
                "sync": { "tracker": "github", "links": links } })
        .to_string()
    };
    let report = validate(&body(json!([
        link("f1", "issue", 1),
        link("f1", "issue", 2),
        link("f2", "issue", 1),
        link("m1", "issue", 3),
        link("t1", "milestone", 4),
        link("m1", "milestone", 5),
    ])));
    let got: Vec<_> = report.violations.iter().map(|v| (v.code, v.link_index.unwrap())).collect();
    assert_eq!(
        got,
        vec![
            ("duplicate_tracker_link", 1),
            ("duplicate_tracker_link", 2),
            ("tracker_kind_mismatch", 3),
            ("tracker_kind_mismatch", 4),
            ("duplicate_tracker_link", 5),
        ]
    );
}

/// FGV-FR-01: a valid document reads into the typed model.
#[test]
fn parse_valid_reads_the_typed_model() {
    let doc = parse_valid(CANONICAL).expect("the canonical document is valid");
    assert_eq!(doc.items.len(), 6);
    assert_eq!(doc.items[1].kind, Kind::Milestone);
    assert_eq!(doc.items[1].due.as_deref(), Some("2027-03-31"));
    assert_eq!(doc.items[3].footprint.as_ref().unwrap().len(), 2);
    assert_eq!(doc.edges[0].edge_type, EdgeType::Blocks);
    let sync = doc.sync.as_ref().unwrap();
    assert_eq!(sync.links[0].reference.number, 42);
    assert_eq!(sync.links[0].base.milestone, Some(Some(3)));

    assert!(parse_valid("").unwrap().items.is_empty());
    assert!(parse_valid(r#"{"version":1}"#).is_err());
}

/// FGV-FR-PMTY, FOA-FR-XOAR: the waiting relation the typed model exposes.
#[test]
fn wait_graph_orders_leaves_through_containers() {
    let body = plan(json!([edge("e1", "f1", "f2", "blocks"), edge("e2", "t2", "t3", "blocks")]));
    let doc = parse_valid(&body).unwrap();
    let hierarchy = Hierarchy::new(&doc.items);
    let blocks: Vec<(&str, &str, &str)> = doc
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks)
        .map(|e| (e.id.as_str(), e.from.as_str(), e.to.as_str()))
        .collect();
    let graph = WaitGraph::new(&hierarchy, &blocks);
    assert!(graph.waits_for("t2", "t1"), "t2 sits in f2, which f1 blocks");
    assert!(graph.waits_for("t3", "t1"), "transitively through t2");
    assert!(!graph.waits_for("t1", "t3"));
    assert!(!graph.waits_for("t1", "t1"));
    assert!(graph.cycle_edges().is_empty());
    assert_eq!(hierarchy.leaves_under("m1"), vec!["t1", "t2", "t3"]);
    assert_eq!(hierarchy.ancestors("t1"), vec!["f1", "m1", "d1"]);
}

/// FGV-FR-PMTY: a container that holds no leaf counts as a leaf, so a ring of
/// empty deliverables that block each other is a cycle.
#[test]
fn empty_containers_in_a_ring_are_a_cycle() {
    let body = doc(
        json!([
            item("d1", "deliverable", None),
            item("d2", "deliverable", None),
            item("d3", "deliverable", None)
        ]),
        json!([
            edge("e1", "d1", "d2", "blocks"),
            edge("e2", "d2", "d3", "blocks"),
            edge("e3", "d3", "d1", "blocks")
        ]),
    );
    assert_eq!(
        edge_codes(&body),
        vec![
            ("dependency_cycle", "e1".into()),
            ("dependency_cycle", "e2".into()),
            ("dependency_cycle", "e3".into()),
        ]
    );
    let open_chain = doc(
        json!([
            item("d1", "deliverable", None),
            item("d2", "deliverable", None),
            item("d3", "deliverable", None)
        ]),
        json!([edge("e1", "d1", "d2", "blocks"), edge("e2", "d2", "d3", "blocks")]),
    );
    assert!(validate(&open_chain).valid);
}
