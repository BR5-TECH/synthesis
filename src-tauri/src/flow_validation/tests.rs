//! The tests of the Flow document schema and of its structural rules
//! (`../../specifications/core/FGV-flow-graph-validation.md`).

use super::*;

fn codes(body: &str) -> Vec<&'static str> {
    validate(body).violations.iter().map(|v| v.code).collect()
}

const CANONICAL: &str = r#"{
      "version": 1,
      "name": "Onboarding review",
      "description": "Reviews a new joiner's first PR.",
      "loops": [
        { "id": "l1", "name": "Review cycle", "artifactIds": ["prompts/stop-when-clean.md"],
          "maxPasses": 5, "position": { "x": 320, "y": 60 },
          "size": { "width": 480, "height": 260 } }
      ],
      "nodes": [
        { "id": "n1", "name": "Collect context", "artifactIds": ["prompts/gather-repo.md"],
          "prompt": "Read the repo.", "position": { "x": 120, "y": 80 } },
        { "id": "n2", "name": "Draft", "parentId": "l1", "position": { "x": 10, "y": 40 } },
        { "id": "n3", "name": "Critique", "parentId": "l1", "position": { "x": 200, "y": 40 } }
      ],
      "edges": [
        { "id": "e1", "from": "n1", "to": "l1", "label": "ok" },
        { "id": "e2", "from": "n2", "to": "n3" },
        { "id": "e3", "from": "n3", "to": "n2", "label": "again" }
      ]
    }"#;

/// FGV-FR-05.
#[test]
fn empty_and_whitespace_bodies_are_the_empty_graph() {
    assert!(validate("").valid);
    assert!(validate("   \n\t \n").valid);
    assert!(validate("").violations.is_empty());
}

/// FGV-FR-08, FLO-FR-03 / FGV-FR-01: the canonical document validates, and so does the
/// same one with `loops` omitted entirely.
#[test]
fn canonical_document_validates() {
    let report = validate(CANONICAL);
    assert!(report.valid, "unexpected violations: {:?}", report.violations);

    let without_loops = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}}],"edges":[]}"#;
    assert!(validate(without_loops).valid);
}

/// FGV-FR-03 / FGV-FR-06: reported alone, though the text also names no version.
#[test]
fn unparseable_body_yields_only_not_json() {
    assert_eq!(codes("{ not json"), vec!["not_json"]);
}

/// FGV-FR-07.
#[test]
fn unknown_version_is_reported_alone() {
    assert_eq!(codes(r#"{"version":7,"nodes":[],"edges":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"nodes":[],"edges":[]}"#), vec!["unknown_version"]);
    // A body that parses to something other than an object claims no version.
    assert_eq!(codes("[]"), vec!["unknown_version"]);
}

/// FGV-FR-04 / FGV-FR-08.
#[test]
fn missing_field_and_unknown_field_are_reported_against_their_element() {
    let body = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a"},
            {"id":"n2","name":"b","promt":"typo","position":{"x":0,"y":0}}
        ],"edges":[]}"#;
    let report = validate(body);
    assert!(!report.valid);
    let shape = report
        .violations
        .iter()
        .find(|v| v.code == "shape" && v.element_id.as_deref() == Some("n1"))
        .expect("shape violation on n1");
    assert!(shape.message.contains("position"), "{}", shape.message);
    let unknown = report
        .violations
        .iter()
        .find(|v| v.code == "unknown_field")
        .expect("unknown_field violation");
    assert_eq!(unknown.element_id.as_deref(), Some("n2"));
    assert!(unknown.message.contains("promt"), "{}", unknown.message);
}

/// FGV-FR-08: the singular `artifactId` is schema history, not an unknown
/// field, so a document still carrying one opens and is migrated on write.
#[test]
fn legacy_singular_artifact_id_is_not_an_unknown_field() {
    let body = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a","artifactId":"a.md","position":{"x":0,"y":0}}
        ],"edges":[]}"#;
    assert!(validate(body).valid);
}

/// FGV-FR-09: one id space across nodes and loops.
#[test]
fn duplicate_id_spans_nodes_and_loops() {
    let body = r#"{"version":1,"name":"x","loops":[
            {"id":"x1","name":"l","position":{"x":0,"y":0},"size":{"width":10,"height":10}}
        ],"nodes":[
            {"id":"x1","name":"n","position":{"x":0,"y":0}}
        ],"edges":[]}"#;
    assert!(codes(body).contains(&"duplicate_id"));
}

/// FGV-FR-10: an endpoint may name a loop.
#[test]
fn dangling_endpoint_is_reported_and_a_loop_endpoint_is_not() {
    let body = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"l","position":{"x":0,"y":0},"size":{"width":10,"height":10}}
        ],"nodes":[
            {"id":"n1","name":"n","position":{"x":0,"y":0}}
        ],"edges":[{"id":"e1","from":"n1","to":"n9"}]}"#;
    let report = validate(body);
    let v = report
        .violations
        .iter()
        .find(|v| v.code == "dangling_edge_endpoint")
        .expect("dangling endpoint");
    assert_eq!(v.edge_id.as_deref(), Some("e1"));

    let ok = body.replace(r#""to":"n9""#, r#""to":"l1""#);
    assert!(validate(&ok).valid, "{:?}", validate(&ok).violations);
}

/// FGV-FR-11.
#[test]
fn self_edge_and_duplicate_pair_are_reported_but_the_reverse_pair_is_not() {
    let body = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","position":{"x":0,"y":0}},
            {"id":"n3","name":"c","position":{"x":0,"y":0}}
        ],"edges":[
            {"id":"e1","from":"n1","to":"n1"},
            {"id":"e2","from":"n2","to":"n3"},
            {"id":"e3","from":"n2","to":"n3"},
            {"id":"e4","from":"n3","to":"n2"}
        ]}"#;
    let report = validate(body);
    let by_code = |c: &str| {
        report
            .violations
            .iter()
            .filter(|v| v.code == c)
            .collect::<Vec<_>>()
    };
    assert_eq!(by_code("self_edge").len(), 1);
    let dup = by_code("duplicate_edge");
    assert_eq!(dup.len(), 1);
    assert_eq!(dup[0].edge_id.as_deref(), Some("e3"));
}

/// FGV-FR-12: the whole of the containment rule on edges.
#[test]
fn cross_container_edge_is_reported_and_an_edge_to_the_loop_is_not() {
    let body = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"l","position":{"x":0,"y":0},"size":{"width":10,"height":10}}
        ],"nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","parentId":"l1","position":{"x":0,"y":0}}
        ],"edges":[{"id":"e1","from":"n1","to":"n2"}]}"#;
    let report = validate(body);
    let v = report
        .violations
        .iter()
        .find(|v| v.code == "cross_container_edge")
        .expect("cross-container edge");
    assert_eq!(v.edge_id.as_deref(), Some("e1"));

    let ok = body.replace(r#""to":"n2""#, r#""to":"l1""#);
    assert!(validate(&ok).valid, "{:?}", validate(&ok).violations);
}

/// FGV-FR-13.
#[test]
fn containment_cycle_and_dangling_parent_are_reported() {
    let cyclic = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"a","parentId":"l2","position":{"x":0,"y":0},"size":{"width":1,"height":1}},
            {"id":"l2","name":"b","parentId":"l1","position":{"x":0,"y":0},"size":{"width":1,"height":1}}
        ],"nodes":[],"edges":[]}"#;
    let report = validate(cyclic);
    let cycles: Vec<_> = report
        .violations
        .iter()
        .filter(|v| v.code == "containment_cycle")
        .collect();
    assert_eq!(cycles.len(), 2, "one per loop in the chain");

    let node_parent = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","parentId":"n1","position":{"x":0,"y":0}}
        ],"edges":[]}"#;
    assert!(codes(node_parent).contains(&"dangling_parent"));
}

/// FGV-FR-14.
#[test]
fn max_passes_must_be_a_positive_whole_number_when_present() {
    let with = |v: &str| {
        format!(
            r#"{{"version":1,"name":"x","loops":[{{"id":"l1","name":"l","maxPasses":{v},
                "position":{{"x":0,"y":0}},"size":{{"width":1,"height":1}}}}],
                "nodes":[],"edges":[]}}"#
        )
    };
    assert!(codes(&with("0")).contains(&"invalid_max_passes"));
    assert!(codes(&with(r#""five""#)).contains(&"invalid_max_passes"));
    assert!(codes(&with("2.5")).contains(&"invalid_max_passes"));
    assert!(validate(&with("5")).valid);

    let without = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"l","position":{"x":0,"y":0},"size":{"width":1,"height":1}}
        ],"nodes":[],"edges":[]}"#;
    assert!(validate(without).valid);
}

/// FGV-FR-15: the deliberate list of what is NOT a violation.
#[test]
fn unresolved_references_empty_names_and_edge_cycles_are_not_violations() {
    let body = r#"{"version":1,"name":"","loops":[
            {"id":"l1","name":"","position":{"x":0,"y":0},"size":{"width":1,"height":1}}
        ],"nodes":[
            {"id":"n1","name":"","artifactIds":["prompts/deleted.md"],"position":{"x":-99,"y":-99}},
            {"id":"n2","name":"b","position":{"x":0,"y":0}},
            {"id":"n3","name":"c","position":{"x":0,"y":0}}
        ],"edges":[
            {"id":"e1","from":"n1","to":"n2"},
            {"id":"e2","from":"n2","to":"n3"},
            {"id":"e3","from":"n3","to":"n1"}
        ]}"#;
    let report = validate(body);
    assert!(report.valid, "unexpected: {:?}", report.violations);
}

/// FGV-FR-03 / FGV-FR-04: every violation in one report, in a
/// stable order, document-level before element before edge.
#[test]
fn reports_every_violation_in_a_stable_order() {
    let body = r#"{"version":1,"name":"x","extra":1,"loops":[
            {"id":"l1","name":"l","position":{"x":0,"y":0},"size":{"width":1,"height":1}}
        ],"nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n1","name":"dup","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","parentId":"l1","position":{"x":0,"y":0}}
        ],"edges":[
            {"id":"e1","from":"n1","to":"n2"},
            {"id":"e2","from":"n1","to":"nope"}
        ]}"#;
    let report = validate(body);
    let got: Vec<&str> = report.violations.iter().map(|v| v.code).collect();
    assert!(got.contains(&"unknown_field"));
    assert!(got.contains(&"duplicate_id"));
    assert!(got.contains(&"cross_container_edge"));
    assert!(got.contains(&"dangling_edge_endpoint"));
    // Document-level first, edge-level last.
    assert_eq!(got.first(), Some(&"unknown_field"));
    assert!(report
        .violations
        .last()
        .is_some_and(|v| v.edge_id.is_some()));
    // Stable across runs.
    assert_eq!(validate(body), report);
}

/// `describe` is what a refused write says (PST-FR-28): every violation,
/// not the first. The fixture carries three so a `describe` that emitted
/// only the first would fail rather than pass by coincidence.
#[test]
fn describe_lists_every_violation() {
    let one = validate(
        r#"{"version":1,"name":"x","nodes":[
                {"id":"n1","name":"a","position":{"x":0,"y":0}}
            ],"edges":[{"id":"e1","from":"n1","to":"n1"}]}"#,
    );
    assert!(describe(&one).contains("1 problem"), "{}", describe(&one));

    let many = validate(
        r#"{"version":1,"name":"x","loops":[
                {"id":"l1","name":"L","position":{"x":0,"y":0},"size":{"width":9,"height":9}}
            ],"nodes":[
                {"id":"n1","name":"a","position":{"x":0,"y":0}},
                {"id":"n2","name":"b","parentId":"l1","position":{"x":0,"y":0}}
            ],"edges":[
                {"id":"e1","from":"n1","to":"n1"},
                {"id":"e2","from":"n1","to":"n2"},
                {"id":"e3","from":"n1","to":"nope"}
            ]}"#,
    );
    assert_eq!(many.violations.len(), 3, "{:?}", many.violations);
    let text = describe(&many);
    assert!(text.contains("3 problems"), "{text}");
    for v in &many.violations {
        assert!(text.contains(&v.message), "missing {:?} in {text}", v.message);
    }
}

/// FGV-FR-09, second sentence: edge ids share the uniqueness requirement
/// among themselves, and the violation names the EDGE — which is what the
/// frontend's violation list renders against.
#[test]
fn a_duplicate_edge_id_is_reported_against_the_edge() {
    let body = r#"{"version":1,"name":"x","nodes":[
            {"id":"n1","name":"a","position":{"x":0,"y":0}},
            {"id":"n2","name":"b","position":{"x":0,"y":0}},
            {"id":"n3","name":"c","position":{"x":0,"y":0}}
        ],"edges":[
            {"id":"e1","from":"n1","to":"n2"},
            {"id":"e1","from":"n2","to":"n3"}
        ]}"#;
    let report = validate(body);
    let v = report
        .violations
        .iter()
        .find(|v| v.code == "duplicate_id")
        .expect("duplicate edge id");
    assert_eq!(v.edge_id.as_deref(), Some("e1"));
    assert_eq!(v.element_id, None);
}

/// A `parentId` naming the loop itself closes the chain in one step
/// (FGV-FR-13), which the two-loop fixture above does not reach.
#[test]
fn a_loop_that_parents_itself_is_a_containment_cycle() {
    let body = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"L","parentId":"l1","position":{"x":0,"y":0},
             "size":{"width":9,"height":9}}
        ],"nodes":[],"edges":[]}"#;
    assert!(codes(body).contains(&"containment_cycle"));
}

/// FGV-FR-01: a legitimately nested document validates. The containment walk
/// is otherwise exercised only by its failure cases, so a regression that
/// flagged real nesting would go unnoticed.
#[test]
fn a_nested_document_validates() {
    let body = r#"{"version":1,"name":"x","loops":[
            {"id":"l1","name":"Outer","position":{"x":0,"y":0},"size":{"width":400,"height":300}},
            {"id":"l2","name":"Inner","parentId":"l1","position":{"x":10,"y":60},
             "size":{"width":200,"height":120}}
        ],"nodes":[
            {"id":"n1","name":"p","parentId":"l1","position":{"x":250,"y":60}},
            {"id":"n2","name":"q","parentId":"l2","position":{"x":5,"y":5}},
            {"id":"n3","name":"r","parentId":"l2","position":{"x":100,"y":5}}
        ],"edges":[
            {"id":"e1","from":"n2","to":"n3"},
            {"id":"e2","from":"n1","to":"l2"}
        ]}"#;
    let report = validate(body);
    assert!(report.valid, "unexpected: {:?}", report.violations);
}

/// An `artifactIds` entry names a path-derived node key (ASC-FR-13), so an
/// empty one is malformed. This is also where the two implementations of the
/// rule set would otherwise disagree: `parseFlowDocument` refuses it, and a
/// body this module called valid would then fail to deserialize.
#[test]
fn an_empty_artifact_id_is_a_shape_violation() {
    let with = |ids: &str| {
        format!(
            r#"{{"version":1,"name":"x","nodes":[{{"id":"n1","name":"a",
                "artifactIds":{ids},"position":{{"x":0,"y":0}}}}],"edges":[]}}"#
        )
    };
    assert!(codes(&with(r#"[""]"#)).contains(&"shape"));
    assert!(codes(&with("[7]")).contains(&"shape"));
    // A reference to an artifact that no longer exists is NOT a violation
    // (FGV-FR-15) — only a malformed one is.
    assert!(validate(&with(r#"["prompts/deleted.md"]"#)).valid);
}

/// FGV-FR-15, FLO-FR-37 / FGV-FR-08: a loop carries artifact references on exactly a
/// node's terms, and carries no inline prompt — what it is run under is the
/// artifact it references (`../ui/FLO-flow.md` FLO-FR-37).
#[test]
fn a_loop_carries_artifact_references_and_no_prompt() {
    let with = |field: &str| {
        format!(
            r#"{{"version":1,"name":"x","loops":[{{"id":"l1","name":"c",{field}
                "position":{{"x":0,"y":0}},"size":{{"width":300,"height":200}}}}],
                "nodes":[],"edges":[]}}"#
        )
    };
    assert!(validate(&with(r#""artifactIds":["prompts/stop.md"],"#)).valid);
    // The same rule a node's list obeys, and the same code.
    assert!(codes(&with(r#""artifactIds":[""],"#)).contains(&"shape"));
    assert!(codes(&with(r#""prompt":"do it","#)).contains(&"unknown_field"));
    // The singular form is a node's history alone: no loop was ever written
    // with it, so it is a misspelling here rather than a legacy field.
    assert!(codes(&with(r#""artifactId":"prompts/stop.md","#)).contains(&"unknown_field"));
    // And the field it replaced is no longer part of the schema.
    assert!(codes(&with(r#""exitCondition":"until clean","#)).contains(&"unknown_field"));
}

/// The wire shape the frontend reads (`src/types.ts::FlowViolation`): camel
/// case, and the two id fields omitted rather than nulled when absent. A
/// rename here would otherwise surface in the UI as an empty violation list.
#[test]
fn the_report_serializes_in_the_shape_the_frontend_reads() {
    let report = validate("{ not json");
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["valid"], serde_json::json!(false));
    let first = &json["violations"][0];
    assert_eq!(first["code"], serde_json::json!("not_json"));
    assert!(first["message"].is_string());
    assert!(first.get("elementId").is_none(), "absent ids are omitted");
    assert!(first.get("edgeId").is_none());

    let named = validate(
        r#"{"version":1,"name":"x","nodes":[
                {"id":"n1","name":"a","position":{"x":0,"y":0}}
            ],"edges":[{"id":"e1","from":"n1","to":"n1"}]}"#,
    );
    let edge = &serde_json::to_value(&named).unwrap()["violations"][0];
    assert_eq!(edge["edgeId"], serde_json::json!("e1"));
}

/// FGV-FR-08, FLO-FR-03 / FGV-FR-01: this module is the single definition of the shape,
/// and the Flow tab serializes to it. Every Flow committed to this
/// repository was written by that serializer, so validating them is what
/// catches the two implementations drifting apart — in either direction.
///
/// `resources/flows` holds this project's own workflows and covers what they
/// happen to use; `fixtures/flows` covers the rest, today the whole of the
/// loop half of the schema. The other side of this contract is
/// `src/state/flowFixtures.test.ts`, which sweeps the same two directories
/// and asserts each body re-serializes to itself byte for byte — so a shape
/// this test calls valid is one the editor would actually write.
#[test]
fn every_committed_flow_validates() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    for dir_name in ["resources/flows", "fixtures/flows"] {
        let dir = root.join(dir_name);
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{dir_name}: {e}"));
        let mut checked = 0;
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("flow") {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap();
            let report = validate(&body);
            assert!(
                report.valid,
                "{} does not validate: {:?}",
                path.display(),
                report.violations
            );
            checked += 1;
        }
        // A sweep over an empty directory passes silently, which would make
        // the drift guard read as green while guarding nothing.
        assert!(checked > 0, "no Flow fixtures found in {}", dir.display());
    }
}

/// FGV-FR-01, FGV-FR-08, FLO-FR-03: the loop half of the schema, which no workflow committed under
/// `resources/flows` uses. Named separately from the sweep above so a
/// deleted fixture fails as the missing coverage it is rather than as one
/// fewer file nobody counted.
#[test]
fn the_loop_fixture_exercises_every_field_a_loop_carries() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join("fixtures/flows/Loops.flow");
    let body = std::fs::read_to_string(&path).expect("fixtures/flows/Loops.flow");
    assert!(validate(&body).valid);
    for field in ["\"artifactIds\"", "\"maxPasses\"", "\"parentId\""] {
        assert!(body.contains(field), "the loop fixture no longer carries {field}");
    }
    // The fields the schema does NOT define, which a document written by an
    // older editor would carry (`../ui/FLO-flow.md` FLO-FR-37).
    assert!(!body.contains("exitCondition"));
}
