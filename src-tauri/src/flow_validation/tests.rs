//! The tests of the Flow document schema, version 3: the document-level rules
//! and the per-field rules (`../../specifications/core/FGV-flow-graph-validation.md`).
//! The structural rules are tested in `tests_rules.rs`.

use serde_json::{json, Value};

use super::*;

pub(super) fn codes(body: &str) -> Vec<&'static str> {
    validate(body).violations.iter().map(|v| v.code).collect()
}

/// A version-3 document holding `items` and `edges`, serialized.
pub(super) fn doc(items: Value, edges: Value) -> String {
    json!({ "version": 3, "name": "Plan", "items": items, "edges": edges }).to_string()
}

pub(super) fn item(id: &str, kind: &str, parent: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": kind, "name": id });
    if let Some(p) = parent {
        v["parentId"] = json!(p);
    }
    v
}

pub(super) fn edge(id: &str, from: &str, to: &str, ty: &str) -> Value {
    json!({ "id": id, "from": from, "to": to, "type": ty })
}

/// The example document of the spec's contract surface, as the spec writes
/// it.
pub(crate) const CANONICAL: &str = r#"{
  "version": 3,
  "name": "Payments v2",
  "description": "Card payments for the EU launch.",
  "references": ["docs/payments-hld.md"],
  "items": [
    {
      "id": "m1",
      "kind": "milestone",
      "name": "Beta",
      "description": "Accept **Visa** and **Mastercard**.",
      "artifactIds": ["specifications/core/PAY-payments.md"],
      "due": "2027-03-31"
    },
    {
      "id": "f1",
      "kind": "feature",
      "name": "Card capture",
      "parentId": "m1"
    },
    {
      "id": "t1",
      "kind": "task",
      "name": "Tokenize card numbers",
      "parentId": "f1",
      "draftId": "dr_7f3a",
      "footprint": ["src-tauri/src/payments/", "specifications/core/PAY-payments.md"]
    },
    {
      "id": "t2",
      "kind": "task",
      "name": "Card form",
      "parentId": "f1"
    },
    {
      "id": "x1",
      "kind": "defect",
      "name": "Decline message is wrong"
    }
  ],
  "edges": [
    { "id": "e1", "from": "t1", "to": "t2", "type": "blocks" },
    { "id": "e2", "from": "t2", "to": "x1", "type": "relates" }
  ]
}"#;

/// FGV-FR-05.
#[test]
fn empty_and_whitespace_bodies_are_the_empty_plan() {
    assert!(validate("").valid);
    assert!(validate("   \n\t \n").valid);
}

/// FGV-FR-01, FGV-FR-08: the canonical document validates, and so does a
/// document that holds no items and no edges.
#[test]
fn canonical_document_validates() {
    let report = validate(CANONICAL);
    assert!(report.valid, "unexpected violations: {:?}", report.violations);
    assert!(validate(r#"{"version":3}"#).valid);
}

/// FGV-FR-06: reported alone.
#[test]
fn unparseable_body_yields_only_not_json() {
    assert_eq!(codes("{ not json"), vec!["not_json"]);
}

/// FGV-FR-07: version 1, version 2, any other version, and no version are
/// each reported alone as an unknown version.
#[test]
fn every_version_but_three_is_unknown_and_alone() {
    let v1 = r#"{"version":1,"name":"x","nodes":[],"edges":[],"bogus":1}"#;
    assert_eq!(codes(v1), vec!["unknown_version"]);
    let v2 = r#"{"version":2,"items":[{"id":"m1","kind":"milestone","name":"m",
        "position":{"x":0,"y":0},"size":{"width":1,"height":1}}],"sync":{}}"#;
    assert_eq!(codes(v2), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"version":4,"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"version":"3","items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"version":3.5,"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"version":null,"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"[1,2]"#), vec!["unknown_version"]);
}

/// FGV-FR-08: an unknown field anywhere is `unknown_field`, and a field of the
/// wrong type or outside its enumeration is `shape`.
#[test]
fn unknown_fields_and_wrong_types_are_reported() {
    let mut i = item("m1", "milestone", None);
    i["prompt"] = json!("old");
    let body = json!({ "version": 3, "loops": [], "items": [i], "edges": [] }).to_string();
    assert_eq!(codes(&body), vec!["unknown_field", "unknown_field"]);

    let bad_kind = doc(json!([{ "id": "x", "kind": "loop", "name": "x" }]), json!([]));
    assert_eq!(codes(&bad_kind), vec!["shape"]);

    let mut no_name = item("m1", "milestone", None);
    no_name.as_object_mut().unwrap().remove("name");
    no_name["description"] = json!(3);
    assert_eq!(codes(&doc(json!([no_name]), json!([]))), vec!["shape", "shape"]);

    let bad_type = doc(
        json!([item("a", "task", None), item("b", "task", None)]),
        json!([edge("e", "a", "b", "label")]),
    );
    assert!(codes(&bad_type).contains(&"shape"));
}

/// FGV-FR-01, FGV-FR-08: version 3 drops the canvas facts and the tracker
/// section. `position`, `size`, and `localOnly` on an item, and `sync` on the
/// document, are fields the schema does not define.
#[test]
fn canvas_and_tracker_fields_are_unknown() {
    let mut m = item("m1", "milestone", None);
    m["position"] = json!({ "x": 0, "y": 0 });
    m["size"] = json!({ "width": 10, "height": 10 });
    let mut t = item("t1", "task", None);
    t["localOnly"] = json!(true);
    t["collapsed"] = json!(true);
    let body = json!({ "version": 3, "items": [m, t], "edges": [],
                       "sync": { "tracker": "github", "links": [] } })
    .to_string();
    let report = validate(&body);
    let got: Vec<_> = report.violations.iter().map(|v| (v.code, v.element_id.clone())).collect();
    assert_eq!(
        got,
        vec![
            ("unknown_field", None),
            ("unknown_field", Some("m1".to_string())),
            ("unknown_field", Some("m1".to_string())),
            ("unknown_field", Some("t1".to_string())),
            ("unknown_field", Some("t1".to_string())),
        ]
    );
    assert!(report.violations[0].message.contains("`sync`"), "{:?}", report.violations[0]);
}

/// FGV-FR-MZIA: fields a kind does not carry. A container needs no size.
#[test]
fn fields_not_allowed_for_kind() {
    let mut f = item("f1", "feature", None);
    f["draftId"] = json!("d");
    f["footprint"] = json!(["src/"]);
    f["due"] = json!("2027-01-01");
    let mut t = item("t1", "task", Some("f1"));
    t["due"] = json!("2027-01-01");
    let report = validate(&doc(json!([f, t]), json!([])));
    let got: Vec<_> =
        report.violations.iter().map(|v| (v.code, v.element_id.clone().unwrap())).collect();
    assert_eq!(
        got,
        vec![
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "t1".to_string()),
        ]
    );

    // `draftId` and `footprint` on a milestone and an epic, and `due` on an
    // epic and a defect.
    let mut m = item("m1", "milestone", None);
    m["draftId"] = json!("d1");
    let mut e = item("e1", "epic", None);
    e["footprint"] = json!(["src/"]);
    e["due"] = json!("2027-01-01");
    let mut x = item("x1", "defect", None);
    x["due"] = json!("2027-01-01");
    let report = validate(&doc(json!([m, e, x]), json!([])));
    let got: Vec<_> =
        report.violations.iter().map(|v| (v.code, v.element_id.clone().unwrap())).collect();
    assert_eq!(
        got,
        vec![
            ("field_not_allowed_for_kind", "m1".to_string()),
            ("field_not_allowed_for_kind", "e1".to_string()),
            ("field_not_allowed_for_kind", "e1".to_string()),
            ("field_not_allowed_for_kind", "x1".to_string()),
        ]
    );

    let mut m = item("m1", "milestone", None);
    m["due"] = json!("2027-01-01");
    let containers = doc(
        json!([m, item("e1", "epic", Some("m1")), item("f1", "feature", Some("e1"))]),
        json!([]),
    );
    assert!(validate(&containers).valid, "{:?}", validate(&containers).violations);
}

/// FGV-FR-RQEM: empty, absolute, climbing, and repeated footprint entries.
#[test]
fn invalid_footprint_paths() {
    for bad in ["", "/etc/passwd", "\\\\share\\x", "C:/x", "src/../../x", ".."] {
        let mut t = item("t1", "task", None);
        t["footprint"] = json!([bad]);
        assert!(
            codes(&doc(json!([t]), json!([]))).contains(&"invalid_footprint_path"),
            "{bad:?} must be refused"
        );
    }
    let mut t = item("t1", "task", None);
    t["footprint"] = json!(["src/a.rs", "src/a.rs"]);
    assert!(codes(&doc(json!([t]), json!([]))).contains(&"invalid_footprint_path"));

    let mut ok = item("t1", "task", None);
    ok["footprint"] = json!(["src/", "src/a..b.rs", "docs/x.md"]);
    assert!(!codes(&doc(json!([ok]), json!([]))).contains(&"invalid_footprint_path"));
}

/// FGV-FR-RRND: a due date is a real calendar date in `YYYY-MM-DD`.
#[test]
fn invalid_due_dates() {
    for bad in ["2027-02-30", "2027-13-01", "27-01-01", "2027-1-01", "tomorrow", "2027-02-29"] {
        let mut m = item("m1", "milestone", None);
        m["due"] = json!(bad);
        let body = doc(json!([m]), json!([]));
        assert_eq!(codes(&body), vec!["invalid_due_date"], "{bad}");
    }
    let mut m = item("m1", "milestone", None);
    m["due"] = json!("2028-02-29");
    assert!(validate(&doc(json!([m]), json!([]))).valid);
}

/// FGV-FR-JDGO: a repeated reference, in an item or in the document.
#[test]
fn duplicate_references() {
    let mut t = item("m1", "milestone", None);
    t["artifactIds"] = json!(["a.md", "a.md"]);
    let mut body: Value = serde_json::from_str(&doc(json!([t]), json!([]))).unwrap();
    body["references"] = json!(["hld.md", "hld.md"]);
    let report = validate(&body.to_string());
    let got: Vec<_> = report.violations.iter().map(|v| (v.code, v.element_id.is_some())).collect();
    assert_eq!(got, vec![("duplicate_reference", false), ("duplicate_reference", true)]);
}

/// FGV-FR-15: what is not a violation.
#[test]
fn deliberate_non_violations() {
    let mut d = item("m1", "milestone", None);
    d["name"] = json!("");
    d["artifactIds"] = json!(["gone/file.md"]);
    let mut t = item("t1", "task", Some("f1"));
    t["draftId"] = json!("no-such-draft");
    t["footprint"] = json!(["no/such/path.rs"]);
    let body = json!({
        "version": 3,
        "references": ["gone.md"],
        "items": [d, item("f1", "feature", Some("m1")), t,
                  item("f2", "feature", Some("m1")), item("f3", "feature", Some("m1")),
                  item("e9", "epic", None)],
        "edges": [edge("e1", "f1", "f2", "relates"), edge("e2", "f2", "f3", "relates"),
                  edge("e3", "f3", "f1", "relates")]
    });
    let report = validate(&body.to_string());
    assert!(report.valid, "unexpected violations: {:?}", report.violations);
}

/// FGV-FR-03 / FGV-FR-04: every violation is reported, not the first one:
/// document, then items, then edges; each group in document order.
#[test]
fn violations_are_grouped_and_ordered() {
    let mut t2 = item("t2", "task", Some("f1"));
    t2["due"] = json!("2027-01-01");
    let body = json!({
        "version": 3,
        "bogus": true,
        "references": "not a list",
        "items": [item("t1", "task", Some("nowhere")), t2, item("f1", "feature", None),
                  item("m1", "milestone", Some("f1"))],
        "edges": [edge("e1", "t1", "t1", "blocks"), edge("e2", "t1", "zz", "relates")]
    });
    let report = validate(&body.to_string());
    let got: Vec<_> = report
        .violations
        .iter()
        .map(|v| (v.code, v.element_id.clone().or(v.edge_id.clone())))
        .collect();
    assert_eq!(
        got,
        vec![
            ("unknown_field", None),
            ("shape", None),
            ("dangling_parent", Some("t1".into())),
            ("field_not_allowed_for_kind", Some("t2".into())),
            ("invalid_parent_kind", Some("m1".into())),
            ("self_edge", Some("e1".into())),
            ("dangling_edge_endpoint", Some("e2".into())),
        ]
    );
    assert_eq!(validate(&body.to_string()), report, "the order is stable");
}

/// FGV-FR-03: the refusal message names every violation.
#[test]
fn describe_lists_every_violation() {
    let report = validate(&doc(
        json!([item("f1", "feature", None), item("m1", "milestone", Some("f1"))]),
        json!([]),
    ));
    assert_eq!(report.violations.len(), 1);
    let text = describe(&report);
    assert!(text.contains("1 problem in the plan"), "{text}");
    assert!(text.contains("m1"), "{text}");
}
