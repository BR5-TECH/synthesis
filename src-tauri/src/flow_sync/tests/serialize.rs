//! FTS-FR-VFNM: the Flow a sync writes has the bytes the Flow tab writes.

use crate::flow_sync::serialize::write_flow;
use crate::flow_validation::model::{parse_valid, Position};

/// A plan as `JSON.stringify(doc, null, 2) + "\n"` writes it: whole numbers
/// with no fraction, a fraction, a negative number, escapes, text that is not
/// ASCII, `null` base values, empty lists, and `sync.ignored` after the links.
const CANONICAL: &str = r#"{
  "version": 2,
  "name": "Payments v2",
  "description": "Card payments for the \"EU\" launch.\nLine two\ttab",
  "references": [
    "docs/payments-hld.md"
  ],
  "items": [
    {
      "id": "m1",
      "kind": "milestone",
      "name": "Karten – Zahlungen ✓",
      "description": "Accept **Visa**.",
      "artifactIds": [
        "specifications/core/PAY-payments.md"
      ],
      "due": "2027-03-31",
      "position": {
        "x": 40,
        "y": -12.5
      },
      "size": {
        "width": 1200,
        "height": 700.25
      }
    },
    {
      "id": "ep1",
      "kind": "epic",
      "name": "Beta",
      "parentId": "m1",
      "position": {
        "x": 0,
        "y": 60
      },
      "size": {
        "width": 900,
        "height": 520
      }
    },
    {
      "id": "x1",
      "kind": "defect",
      "name": "Tokenize",
      "description": "",
      "parentId": "ep1",
      "localOnly": false,
      "draftId": "dr_7f3a",
      "footprint": [
        "src-tauri/src/payments/"
      ],
      "position": {
        "x": 0.1,
        "y": 1e-7
      }
    }
  ],
  "edges": [],
  "sync": {
    "tracker": "github",
    "links": [
      {
        "itemId": "ep1",
        "ref": {
          "kind": "issue",
          "host": "github.com",
          "owner": "acme",
          "repo": "payments",
          "number": 42,
          "url": "https://github.com/acme/payments/issues/42"
        },
        "base": {
          "name": "Card payments",
          "description": "",
          "state": "open",
          "type": null,
          "parent": null,
          "milestone": null,
          "blockedBy": []
        }
      },
      {
        "itemId": "m1",
        "ref": {
          "kind": "milestone",
          "host": "github.com",
          "owner": "acme",
          "repo": "payments",
          "number": 3,
          "url": "https://github.com/acme/payments/milestone/3"
        },
        "base": {
          "name": "Beta",
          "description": "",
          "state": "closed",
          "due": null
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
}
"#;

/// FTS-FR-VFNM: canonical JSON in, parse, write: the same bytes out, so a
/// plan that a sync does not change keeps its bytes and its Git history.
#[test]
fn a_canonical_flow_round_trips_byte_for_byte() {
    let doc = parse_valid(CANONICAL).expect("the canonical plan is valid");
    assert_eq!(write_flow(&doc), CANONICAL);
}

/// FTS-FR-VFNM: a plan the sync wrote reads back to the same plan and is
/// written with the same bytes again.
#[test]
fn a_written_plan_is_written_again_with_the_same_bytes() {
    let doc = super::plan().1;
    let first = write_flow(&doc);
    let again = parse_valid(&first).expect("the written plan is valid");
    assert_eq!(write_flow(&again), first);
    assert!(first.ends_with("}\n"));
    assert!(first.contains("\"x\": 20,"), "a whole number has no fraction");
}

/// FTS-FR-VFNM: a whole `f64` is written as JavaScript writes it: no
/// fraction, `-0` as `0`, and a large whole number without an exponent.
#[test]
fn whole_numbers_are_written_without_a_fraction() {
    let mut doc = super::plan().1;
    let t1 = doc.items.iter_mut().find(|i| i.id == "t1").unwrap();
    t1.position = Position { x: -0.0, y: 1e20 };
    let text = write_flow(&doc);
    assert!(text.contains("\"x\": 0,"), "{text}");
    assert!(text.contains("\"y\": 100000000000000000000\n"), "{text}");
    t1_position(&mut doc, 2.5, -40.0);
    let text = write_flow(&doc);
    assert!(text.contains("\"x\": 2.5,") && text.contains("\"y\": -40\n"), "{text}");
}

fn t1_position(doc: &mut crate::flow_validation::model::FlowDocument, x: f64, y: f64) {
    let t1 = doc.items.iter_mut().find(|i| i.id == "t1").unwrap();
    t1.position = Position { x, y };
}

/// FTS-FR-VFNM: numbers are written as ECMAScript `Number::toString` writes
/// them.
#[test]
fn numbers_are_written_as_javascript_writes_them() {
    use crate::flow_sync::serialize::js_number;
    for (value, text) in [
        (0.0, "0"),
        (-0.0, "0"),
        (40.0, "40"),
        (-40.0, "-40"),
        (12.5, "12.5"),
        (0.1, "0.1"),
        (0.000001, "0.000001"),
        (1e-7, "1e-7"),
        (1.5e-7, "1.5e-7"),
        (123456.789, "123456.789"),
        (1e20, "100000000000000000000"),
        (1e21, "1e+21"),
        (1.25e22, "1.25e+22"),
    ] {
        assert_eq!(js_number(value), text, "{value:e}");
    }
}

/// FTS-FR-VFNM: the fields the Flow tab leaves out are left out: an empty
/// list and an empty document description; the document name is written
/// always.
#[test]
fn empty_fields_are_left_out_as_the_flow_tab_does() {
    let mut doc = super::plan().1;
    doc.name = None;
    doc.description = Some(String::new());
    doc.references = Some(Vec::new());
    let t1 = doc.items.iter_mut().find(|i| i.id == "t1").unwrap();
    t1.artifact_ids = Some(Vec::new());
    t1.footprint = Some(Vec::new());
    let text = write_flow(&doc);
    assert!(text.contains("\"name\": \"\","));
    for gone in ["\n  \"description\": \"\"", "references", "artifactIds", "footprint"] {
        assert!(!text.contains(gone), "{gone}");
    }
}

/// FTS-FR-VFNM / FGV-FR-01: the example document of
/// FGV-flow-graph-validation.md, which starts at a milestone and ignores one
/// issue, reads as a plan, and the plan it is written as reads back to the
/// same plan, `sync.ignored` included.
#[test]
fn the_specification_example_reads_back_as_the_same_plan() {
    let doc = parse_valid(crate::flow_validation::tests::CANONICAL).expect("the example is valid");
    let written = write_flow(&doc);
    let again = parse_valid(&written).expect("the written example is valid");
    assert_eq!(again, doc);
    assert_eq!(write_flow(&again), written);
    let ignored = written.find("\"ignored\"").expect("the ignored list is written");
    assert!(written.find("\"links\"").unwrap() < ignored, "ignored comes after links");
}

/// FTS-FR-VFNM / FTS-FR-YHVD: an empty `sync.ignored` is left out, as the
/// Flow tab leaves it out.
#[test]
fn an_empty_ignored_list_is_left_out() {
    let mut doc = super::plan().1;
    doc.sync.as_mut().unwrap().ignored = Vec::new();
    assert!(!write_flow(&doc).contains("ignored"));
}
