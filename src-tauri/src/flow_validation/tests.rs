//! The tests of the Flow document schema, version 2: the document-level rules
//! and the per-field rules (`../../specifications/core/FGV-flow-graph-validation.md`).
//! The structural rules are tested in `tests_rules.rs`.

use serde_json::{json, Value};

use super::*;

pub(super) fn codes(body: &str) -> Vec<&'static str> {
    validate(body).violations.iter().map(|v| v.code).collect()
}

/// A version-2 document holding `items` and `edges`, serialized.
pub(super) fn doc(items: Value, edges: Value) -> String {
    json!({ "version": 2, "name": "Plan", "items": items, "edges": edges }).to_string()
}

pub(super) fn item(id: &str, kind: &str, parent: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": kind, "name": id, "position": { "x": 0, "y": 0 } });
    if let Some(p) = parent {
        v["parentId"] = json!(p);
    }
    if !matches!(kind, "task" | "defect") {
        v["size"] = json!({ "width": 100, "height": 100 });
    }
    v
}

pub(super) fn edge(id: &str, from: &str, to: &str, ty: &str) -> Value {
    json!({ "id": id, "from": from, "to": to, "type": ty })
}

/// The example document of the spec's contract surface, as the spec writes
/// it.
pub(crate) const CANONICAL: &str = r#"{
  "version": 2,
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
      "due": "2027-03-31",
      "position": { "x": 40, "y": 40 },
      "size": { "width": 900, "height": 520 }
    },
    {
      "id": "f1",
      "kind": "feature",
      "name": "Card capture",
      "parentId": "m1",
      "position": { "x": 20, "y": 60 },
      "size": { "width": 420, "height": 300 }
    },
    {
      "id": "t1",
      "kind": "task",
      "name": "Tokenize card numbers",
      "parentId": "f1",
      "draftId": "dr_7f3a",
      "footprint": ["src-tauri/src/payments/", "specifications/core/PAY-payments.md"],
      "position": { "x": 20, "y": 60 }
    },
    {
      "id": "t2",
      "kind": "task",
      "name": "Card form",
      "parentId": "f1",
      "localOnly": true,
      "position": { "x": 220, "y": 60 }
    },
    {
      "id": "x1",
      "kind": "defect",
      "name": "Decline message is wrong",
      "parentId": "m1",
      "position": { "x": 460, "y": 60 }
    }
  ],
  "edges": [
    { "id": "e1", "from": "t1", "to": "t2", "type": "blocks" },
    { "id": "e2", "from": "t2", "to": "x1", "type": "relates" }
  ],
  "sync": {
    "tracker": "github",
    "links": [
      {
        "itemId": "f1",
        "ref": {
          "kind": "issue",
          "host": "github.com",
          "owner": "acme",
          "repo": "payments",
          "number": 42,
          "url": "https://github.com/acme/payments/issues/42"
        },
        "base": {
          "name": "Card capture",
          "description": "",
          "type": "Feature",
          "parent": null,
          "milestone": 3,
          "blockedBy": [],
          "state": "open"
        }
      }
    ],
    "ignored": [
      {
        "kind": "issue",
        "host": "github.com",
        "owner": "acme",
        "repo": "payments",
        "number": 47,
        "url": "https://github.com/acme/payments/issues/47"
      }
    ]
  }
}"#;

/// FGV-FR-05.
#[test]
fn empty_and_whitespace_bodies_are_the_empty_plan() {
    assert!(validate("").valid);
    assert!(validate("   \n\t \n").valid);
}

/// FGV-FR-01, FGV-FR-08: the canonical document validates, and so does a
/// document that holds no items, no edges, and no sync section.
#[test]
fn canonical_document_validates() {
    let report = validate(CANONICAL);
    assert!(report.valid, "unexpected violations: {:?}", report.violations);
    assert!(validate(r#"{"version":2}"#).valid);
}

/// FGV-FR-06: reported alone.
#[test]
fn unparseable_body_yields_only_not_json() {
    assert_eq!(codes("{ not json"), vec!["not_json"]);
}

/// FGV-FR-07: version 1, any other version, and no version are each reported
/// alone as an unknown version.
#[test]
fn every_version_but_two_is_unknown_and_alone() {
    let v1 = r#"{"version":1,"name":"x","nodes":[],"edges":[],"bogus":1}"#;
    assert_eq!(codes(v1), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"version":3,"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"{"items":[]}"#), vec!["unknown_version"]);
    assert_eq!(codes(r#"[1,2]"#), vec!["unknown_version"]);
}

/// FGV-FR-08: an unknown field anywhere is `unknown_field`, and a field of the
/// wrong type or outside its enumeration is `shape`.
#[test]
fn unknown_fields_and_wrong_types_are_reported() {
    let mut i = item("m1", "milestone", None);
    i["prompt"] = json!("old");
    let body = json!({ "version": 2, "loops": [], "items": [i], "edges": [] }).to_string();
    assert_eq!(codes(&body), vec!["unknown_field", "unknown_field"]);

    let bad_kind = doc(
        json!([{ "id": "x", "kind": "loop", "name": "x", "position": {"x":0,"y":0} }]),
        json!([]),
    );
    assert_eq!(codes(&bad_kind), vec!["shape"]);

    let mut no_name = item("m1", "milestone", None);
    no_name.as_object_mut().unwrap().remove("name");
    no_name["localOnly"] = json!("yes");
    assert_eq!(codes(&doc(json!([no_name]), json!([]))), vec!["shape", "shape"]);

    let bad_type = doc(
        json!([item("a", "task", None), item("b", "task", None)]),
        json!([edge("e", "a", "b", "label")]),
    );
    assert!(codes(&bad_type).contains(&"shape"));
}

/// FGV-FR-MZIA: fields a kind does not carry, and a container without a size.
#[test]
fn fields_not_allowed_for_kind() {
    let mut f = item("f1", "feature", None);
    f["draftId"] = json!("d");
    f["footprint"] = json!(["src/"]);
    f["due"] = json!("2027-01-01");
    let mut t = item("t1", "task", Some("f1"));
    t["size"] = json!({ "width": 1, "height": 1 });
    let report = validate(&doc(json!([f, t]), json!([])));
    let got: Vec<_> =
        report.violations.iter().map(|v| (v.code, v.element_id.clone().unwrap())).collect();
    assert_eq!(
        got,
        vec![
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "f1".to_string()),
            ("invalid_parent_kind", "f1".to_string()),
            ("field_not_allowed_for_kind", "t1".to_string()),
        ]
    );

    let mut m = item("m1", "milestone", None);
    m.as_object_mut().unwrap().remove("size");
    let report = validate(&doc(json!([m]), json!([])));
    assert!(report.violations.iter().any(|v| v.code == "shape" && v.message.contains("size")));
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
        "version": 2,
        "references": ["gone.md"],
        "items": [d, item("f1", "feature", Some("m1")), t,
                  item("f2", "feature", Some("m1")), item("f3", "feature", Some("m1"))],
        "edges": [edge("e1", "f1", "f2", "relates"), edge("e2", "f2", "f3", "relates"),
                  edge("e3", "f3", "f1", "relates")],
        "sync": { "tracker": "github", "links": [
            { "itemId": "removed-item",
              "ref": { "kind": "issue", "host": "github.com", "owner": "o", "repo": "r",
                       "number": 9, "url": "u" },
              "base": { "name": "x", "description": "", "state": "closed" } } ] }
    });
    let report = validate(&body.to_string());
    assert!(report.valid, "unexpected violations: {:?}", report.violations);
}

/// FGV-FR-08: the sync section's shape, its links, references, and bases.
#[test]
fn sync_section_shape() {
    let body = |sync: Value| {
        json!({ "version": 2, "items": [item("m1", "milestone", None)], "edges": [], "sync": sync })
            .to_string()
    };
    assert_eq!(codes(&body(json!({ "tracker": "jira", "links": [] }))), vec!["shape"]);
    assert_eq!(codes(&body(json!({ "tracker": "github" }))), vec!["shape"]);

    let link = json!({ "itemId": "m1",
        "ref": { "kind": "milestone", "host": "h", "owner": "o", "repo": "r", "number": 3, "url": "u" },
        "base": { "name": "Beta", "description": "", "state": "open", "due": "2027-03-31" } });
    assert!(validate(&body(json!({ "tracker": "github", "links": [link.clone()] }))).valid);

    let mut issue_field = link.clone();
    issue_field["base"]["blockedBy"] = json!([]);
    let report = validate(&body(json!({ "tracker": "github", "links": [issue_field] })));
    assert_eq!(report.violations[0].code, "unknown_field");
    assert_eq!(report.violations[0].link_index, Some(0));

    let mut bad = link;
    bad["ref"].as_object_mut().unwrap().remove("url");
    bad["base"]["state"] = json!("merged");
    bad["base"]["due"] = json!("soon");
    assert_eq!(
        codes(&body(json!({ "tracker": "github", "links": [bad] }))),
        vec!["shape", "shape", "shape"]
    );
}

/// FGV-FR-03 / FGV-FR-04: every violation is reported, not the first one:
/// document, then items, then edges, then sync links; each group in
/// document order.
#[test]
fn violations_are_grouped_and_ordered() {
    let mut t2 = item("t2", "task", None);
    t2["due"] = json!("2027-01-01");
    let body = json!({
        "version": 2,
        "bogus": true,
        "items": [item("t1", "task", Some("nowhere")), t2],
        "edges": [edge("e1", "t1", "t1", "blocks"), edge("e2", "t1", "zz", "relates")],
        "sync": { "tracker": "github", "links": [
            { "itemId": "t1",
              "ref": { "kind": "milestone", "host": "h", "owner": "o", "repo": "r", "number": 1, "url": "u" },
              "base": { "name": "", "description": "", "state": "open" } } ] }
    });
    let report = validate(&body.to_string());
    let got: Vec<_> = report
        .violations
        .iter()
        .map(|v| (v.code, v.element_id.clone().or(v.edge_id.clone()), v.link_index))
        .collect();
    assert_eq!(
        got,
        vec![
            ("unknown_field", None, None),
            ("dangling_parent", Some("t1".into()), None),
            ("field_not_allowed_for_kind", Some("t2".into()), None),
            ("invalid_parent_kind", Some("t2".into()), None),
            ("self_edge", Some("e1".into()), None),
            ("dangling_edge_endpoint", Some("e2".into()), None),
            ("tracker_kind_mismatch", None, Some(0)),
        ]
    );
    assert_eq!(validate(&body.to_string()), report, "the order is stable");
}

/// FGV-FR-02: the refusal message names every violation.
#[test]
fn describe_lists_every_violation() {
    let report = validate(&doc(json!([item("t1", "task", None)]), json!([])));
    assert_eq!(report.violations.len(), 1);
    let text = describe(&report);
    assert!(text.contains("1 problem in the plan"), "{text}");
    assert!(text.contains("t1"), "{text}");
}
