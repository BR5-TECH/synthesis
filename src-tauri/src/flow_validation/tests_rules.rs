//! The tests of the structural rules of a Flow document and of its typed model
//! (`../../specifications/core/FGV-flow-graph-validation.md`).

use serde_json::{json, Value};

use super::model::{parse_valid, EdgeType, Hierarchy, Kind, WaitGraph};
use super::tests::{codes, doc, edge, item, CANONICAL};
use super::*;

/// A valid skeleton: milestone m1 > features f1, f2, f3, each with one task
/// (t1, t2, t3).
fn skeleton() -> Vec<Value> {
    vec![
        item("m1", "milestone", None),
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
    let kinds = ["milestone", "epic", "feature", "task", "defect"];
    let permitted: &[(&str, &[&str])] = &[
        ("milestone", &["top"]),
        ("epic", &["milestone"]),
        ("feature", &["milestone", "epic"]),
        ("task", &["feature"]),
        ("defect", &["feature", "epic", "milestone"]),
    ];
    for (kind, allowed) in permitted {
        for container in ["top", "milestone", "epic", "feature"] {
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
        edge("e2", "t2", "m1", "relates"),
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
    items[4]["draftId"] = json!("dr1");
    items[5]["draftId"] = json!("dr1");
    items[6]["draftId"] = json!("dr2");
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
    assert_eq!(doc.items.len(), 5);
    assert_eq!((doc.items[0].kind, doc.items[0].parent_id.as_deref()), (Kind::Milestone, None));
    assert_eq!(doc.items[0].due.as_deref(), Some("2027-03-31"));
    assert_eq!(doc.items[2].footprint.as_ref().unwrap().len(), 2);
    assert_eq!(doc.edges[0].edge_type, EdgeType::Blocks);
    let sync = doc.sync.as_ref().unwrap();
    assert_eq!(sync.links[0].reference.number, 42);
    assert_eq!(sync.links[0].base.milestone, Some(Some(3)));
    assert_eq!(sync.ignored.iter().map(|r| r.number).collect::<Vec<_>>(), vec![47]);

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
    assert_eq!(hierarchy.ancestors("t1"), vec!["f1", "m1"]);
}

/// FGV-FR-PMTY: a container that holds no leaf counts as a leaf, so a ring of
/// empty milestones that block each other is a cycle.
#[test]
fn empty_containers_in_a_ring_are_a_cycle() {
    let body = doc(
        json!([
            item("m1", "milestone", None),
            item("m2", "milestone", None),
            item("m3", "milestone", None)
        ]),
        json!([
            edge("e1", "m1", "m2", "blocks"),
            edge("e2", "m2", "m3", "blocks"),
            edge("e3", "m3", "m1", "blocks")
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
            item("m1", "milestone", None),
            item("m2", "milestone", None),
            item("m3", "milestone", None)
        ]),
        json!([edge("e1", "m1", "m2", "blocks"), edge("e2", "m2", "m3", "blocks")]),
    );
    assert!(validate(&open_chain).valid);
}

/// FGV-FR-08 / FGV-FR-01: `deliverable` is no item kind any more, so a
/// document that holds one is `shape`.
#[test]
fn a_deliverable_is_outside_the_kind_enumeration() {
    let body = doc(json!([item("d1", "deliverable", None)]), json!([]));
    let report = validate(&body);
    assert_eq!(codes(&body), vec!["shape"]);
    assert_eq!(report.violations[0].element_id.as_deref(), Some("d1"));
    assert!(report.violations[0].message.contains("deliverable"));
}

fn ignored(number: u64) -> Value {
    json!({ "kind": "issue", "host": "github.com", "owner": "o", "repo": "r", "number": number,
            "url": format!("https://github.com/o/r/issues/{number}") })
}

/// FGV-FR-TLMV / FGV-FR-04: an ignored reference that repeats an earlier
/// one, or that a sync link names, is `duplicate_tracker_link`; one of
/// another kind, repository, or owner repeats nothing. It is a
/// violation of the sync section as a whole: it carries no item, edge, or
/// link index, its message names the entry, and it comes after the
/// violations of the links.
#[test]
fn ignored_references_repeat_nothing() {
    let body = |links: Value, ignore: Value| {
        json!({ "version": 2, "items": skeleton(), "edges": [],
                "sync": { "tracker": "github", "links": links, "ignored": ignore } })
        .to_string()
    };
    assert!(
        validate(&body(json!([link("f1", "issue", 1)]), json!([ignored(2), ignored(3)]))).valid
    );
    // The same number as a milestone is another reference.
    let mut milestone = ignored(1);
    milestone["kind"] = json!("milestone");
    assert!(validate(&body(json!([link("f1", "issue", 1)]), json!([milestone.clone()]))).valid);
    // The same number in another repository, or of another owner, is
    // another reference.
    let mut other_repo = ignored(1);
    other_repo["repo"] = json!("other");
    other_repo["url"] = json!("https://github.com/o/other/issues/1");
    let mut other_owner = ignored(1);
    other_owner["owner"] = json!("someone");
    other_owner["url"] = json!("https://github.com/someone/r/issues/1");
    assert!(
        validate(&body(json!([link("f1", "issue", 1)]), json!([other_repo, other_owner]))).valid
    );
    // An ignored milestone that a milestone link names repeats it.
    let report = validate(&body(json!([link("m1", "milestone", 1)]), json!([milestone])));
    let got: Vec<_> = report
        .violations
        .iter()
        .map(|v| (v.code, v.link_index, v.element_id.clone(), v.edge_id.clone()))
        .collect();
    assert_eq!(got, vec![("duplicate_tracker_link", None, None, None)]);
    assert!(report.violations[0].message.contains("sync link"), "{:?}", report.violations[0]);

    let report = validate(&body(
        json!([link("f1", "issue", 1), link("f2", "issue", 1)]),
        json!([ignored(2), ignored(1), ignored(2)]),
    ));
    let got: Vec<_> = report
        .violations
        .iter()
        .map(|v| (v.code, v.link_index, v.element_id.clone(), v.edge_id.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("duplicate_tracker_link", Some(1), None, None),
            ("duplicate_tracker_link", None, None, None),
            ("duplicate_tracker_link", None, None, None),
        ]
    );
    assert!(report.violations[1].message.contains("ignored entry 1"), "{:?}", report.violations[1]);
    assert!(report.violations[1].message.contains("sync link"));
    assert!(report.violations[2].message.contains("ignored entry 2"), "{:?}", report.violations[2]);
    assert!(report.violations[2].message.contains("earlier ignored entry"));
}

/// FGV-FR-08: `sync.ignored` is a list of references; anything else is
/// `shape`, and an entry with a field the schema does not define is
/// `unknown_field`.
#[test]
fn ignored_references_have_the_reference_shape() {
    let body = |ignore: Value| {
        json!({ "version": 2, "items": skeleton(), "edges": [],
                "sync": { "tracker": "github", "links": [], "ignored": ignore } })
        .to_string()
    };
    assert_eq!(codes(&body(json!({}))), vec!["shape"]);
    let mut no_number = ignored(1);
    no_number.as_object_mut().unwrap().remove("number");
    assert_eq!(codes(&body(json!([no_number]))), vec!["shape"]);
    let mut extra = ignored(1);
    extra["title"] = json!("x");
    assert_eq!(codes(&body(json!([extra]))), vec!["unknown_field"]);
    assert_eq!(codes(&body(json!(["issue 1"]))), vec!["shape"]);
}
