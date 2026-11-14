//! The tests of the overlay of a GitHub plan and of the publish journal
//! (`../../../specifications/core/DMS-delivery-machine-state.md` DMS-FR-WAUS,
//! DMS-FR-QFEA, DMS-FR-VEHN, DMS-FR-RVJQ, DMS-FR-ROMF).

use serde_json::json;

use super::tests::{buffer_text, canonical, Fixture, Silent};
use super::*;
use crate::delivery::ERR_INVALID_OVERLAY;
use crate::logging::LogBuffer;

fn overlay(value: serde_json::Value) -> DeliveryOverlay {
    serde_json::from_value(value).unwrap()
}

/// DMS-FR-WAUS, DMS-FR-KITP: the overlay of a Project holds each item's
/// `artifactIds`, `footprint`, and `draftId`, the `relates` edges, and the
/// plan's `references`. It is the record `delivery/overlay/<node-id>.json` of
/// the store, and a read returns what the write stored, in the shape of the
/// DMS contract surface.
#[test]
fn an_overlay_round_trips_by_project_node_id() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let value = json!({
        "items": {
            "I_1": { "artifactIds": ["specifications/core/PAY-payments.md"],
                     "footprint": ["src-tauri/src/payments/"], "draftId": "dr_7f3a" },
            "I_2": { "footprint": ["src/pay/form.tsx"] }
        },
        "relates": [ { "from": "I_1", "to": "I_2" } ],
        "references": ["docs/payments-hld.md"]
    });
    let buffer = LogBuffer::for_one_test();
    write_delivery_overlay(&Silent, buffer, &store, "PVT_kwDO1", &overlay(value.clone())).unwrap();
    assert_eq!(
        Fixture::files_under(&canonical(fx.store.path())),
        vec!["delivery/overlay/PVT_kwDO1.json".to_string()]
    );
    let read = read_delivery_overlay(&Silent, buffer, &store, "PVT_kwDO1").unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap(), value);
    // Another Project has its own overlay, which is empty.
    assert_eq!(
        read_delivery_overlay(&Silent, buffer, &store, "PVT_other").unwrap(),
        DeliveryOverlay::default()
    );
    // DMS-FR-ROMF: the write logs counts, and no path or id.
    let text = buffer_text(buffer);
    assert!(text.contains("delivery overlay written"), "{text}");
    for secret in ["src-tauri/src/payments/", "dr_7f3a", "PAY-payments", "payments-hld"] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}

/// DMS-FR-QFEA: an overlay that breaks FGV-FR-RQEM, FGV-FR-JDGO, FGV-FR-CTFX,
/// or FGV-FR-11 is refused with `invalid_overlay` and the violations, and
/// nothing is written. A stored overlay stays as it was.
#[test]
fn an_overlay_that_breaks_a_field_rule_is_refused() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let good = overlay(json!({ "items": { "I_1": { "draftId": "d1" } } }));
    let buffer = LogBuffer::for_one_test();
    write_delivery_overlay(&Silent, buffer, &store, "PVT_1", &good).unwrap();

    let bad = overlay(json!({
        "items": {
            "I_1": { "footprint": ["/etc/passwd", "src/", "src/"], "draftId": "d1" },
            "I_2": { "artifactIds": ["a.md", "a.md", ""], "footprint": ["../up"], "draftId": "d1" }
        },
        "relates": [ { "from": "I_1", "to": "I_1" }, { "from": "I_1", "to": "I_2" },
                     { "from": "I_2", "to": "I_1" } ],
        "references": ["hld.md", "hld.md"]
    }));
    let err = write_delivery_overlay(&Silent, buffer, &store, "PVT_1", &bad).unwrap_err();
    assert_eq!(err.code, ERR_INVALID_OVERLAY);
    let report = err.report.expect("the violations");
    assert!(!report.valid);
    let got: Vec<_> = report.violations.iter().map(|v| (v.code, v.element_id.clone())).collect();
    assert_eq!(
        got,
        vec![
            ("duplicate_reference", None),
            ("invalid_footprint_path", Some("I_1".to_string())),
            ("invalid_footprint_path", Some("I_1".to_string())),
            ("duplicate_reference", Some("I_2".to_string())),
            ("shape", Some("I_2".to_string())),
            ("invalid_footprint_path", Some("I_2".to_string())),
            ("duplicate_draft_link", Some("I_2".to_string())),
            ("self_edge", None),
            ("duplicate_edge", None),
        ]
    );
    assert_eq!(read_delivery_overlay(&Silent, buffer, &store, "PVT_1").unwrap(), good);

    // DMS-FR-ROMF: the refusal names the codes, never a path or an id.
    let text = buffer_text(buffer);
    assert!(text.contains("invalid_footprint_path"), "{text}");
    assert!(!text.contains("/etc/passwd") && !text.contains("hld.md"), "{text}");
}

/// DMS-FR-QFEA, FGV-FR-15: what is not a violation of an overlay: an empty
/// overlay, entries with no fields, a footprint path that names nothing on
/// disk, and a `relates` edge to an item the plan does not hold.
#[test]
fn a_sound_overlay_validates() {
    assert!(validate_overlay(&DeliveryOverlay::default()).valid);
    let sound = overlay(json!({
        "items": { "I_1": {}, "I_2": { "footprint": ["no/such/path.rs", "src/a..b.rs"] } },
        "relates": [ { "from": "I_1", "to": "gone" } ],
        "references": []
    }));
    let report = validate_overlay(&sound);
    assert!(report.valid, "{:?}", report.violations);
}

/// DMS-FR-QFEA: a node ID that is not one plain path segment is refused on a
/// write and reads as an empty overlay, so no record escapes its folder.
#[test]
fn a_node_id_must_be_one_path_segment() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    for bad in ["", "../escape", "a/b", "PVT 1", "x.json"] {
        let err = write_delivery_overlay(&Silent, buffer, &store, bad, &DeliveryOverlay::default())
            .unwrap_err();
        assert_eq!(err.code, ERR_INVALID_OVERLAY, "{bad:?}");
        assert_eq!(
            read_delivery_overlay(&Silent, buffer, &store, bad).unwrap(),
            DeliveryOverlay::default()
        );
    }
    assert!(Fixture::files_under(&canonical(fx.store.path())).is_empty());
}

/// DMS-FR-VEHN: the overlay and the journal have no Tauri command; the two
/// layout commands are the only commands of the module.
#[test]
fn only_the_layout_has_commands() {
    let names = crate::command_names::COMMAND_NAMES;
    assert!(names.contains(&"load_delivery_layout"));
    assert!(names.contains(&"save_delivery_layout"));
    for absent in [
        "read_delivery_overlay",
        "write_delivery_overlay",
        "read_publish_journal",
        "write_publish_journal",
        "remove_publish_journal",
    ] {
        assert!(!names.contains(&absent), "{absent}");
    }
}

/// DMS-FR-RVJQ: the publish journal of a Flow maps each item id and each edge
/// id to what a publish created for it. It is the record
/// `delivery/publish/<digest>.json`, a later read returns it, and a removal
/// deletes the record; removing an absent journal is not an error.
#[test]
fn a_publish_journal_round_trips_and_is_removed() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    let flow_id = "plans/beta.flow";
    assert_eq!(
        read_publish_journal(&Silent, buffer, &store, flow_id).unwrap(),
        PublishJournal::default()
    );

    let mut journal = PublishJournal::default();
    journal.items.insert(
        "m1".into(),
        CreatedItem {
            kind: CreatedItemKind::Milestone,
            number: 3,
            url: "https://github.com/acme/pay/milestone/3".into(),
            node_id: None,
            complete: true,
        },
    );
    journal.items.insert(
        "t1".into(),
        CreatedItem {
            kind: CreatedItemKind::Issue,
            number: 42,
            url: "https://github.com/acme/pay/issues/42".into(),
            node_id: Some("I_kwDO42".into()),
            complete: true,
        },
    );
    journal.edges.insert("e1".into(), CreatedRelation { blocked: 43, blocking: 42 });
    write_publish_journal(&store, flow_id, &journal).unwrap();
    assert_eq!(
        Fixture::files_under(&canonical(fx.store.path())),
        vec![format!("delivery/publish/{}.json", scope_digest(flow_id))]
    );
    assert_eq!(read_publish_journal(&Silent, buffer, &store, flow_id).unwrap(), journal);
    assert_eq!(
        read_publish_journal(&Silent, buffer, &store, "plans/other.flow").unwrap(),
        PublishJournal::default()
    );

    // A journal grows by a whole-file write of the larger journal.
    journal.edges.insert("e2".into(), CreatedRelation { blocked: 44, blocking: 43 });
    write_publish_journal(&store, flow_id, &journal).unwrap();
    assert_eq!(read_publish_journal(&Silent, buffer, &store, flow_id).unwrap().edges.len(), 2);

    remove_publish_journal(&store, flow_id).unwrap();
    assert!(Fixture::files_under(&canonical(fx.store.path())).is_empty());
    remove_publish_journal(&store, flow_id).unwrap();
}

/// DMS-FR-RVJQ, DSR-FR-ULON: an entry of a partial create keeps
/// `complete: false`; a complete entry writes no such field, and an entry
/// with no such field, as an older journal has, reads as complete.
#[test]
fn a_journal_entry_records_a_partial_create() {
    let partial = CreatedItem {
        kind: CreatedItemKind::Issue,
        number: 7,
        url: "https://github.com/acme/pay/issues/7".into(),
        node_id: None,
        complete: false,
    };
    let text = serde_json::to_value(&partial).unwrap();
    assert_eq!(text["complete"], json!(false));
    assert_eq!(serde_json::from_value::<CreatedItem>(text).unwrap(), partial);
    let whole = CreatedItem { complete: true, ..partial.clone() };
    assert!(serde_json::to_value(&whole).unwrap().get("complete").is_none());
    let older: CreatedItem = serde_json::from_value(
        json!({ "kind": "issue", "number": 7, "url": "https://github.com/acme/pay/issues/7" }),
    )
    .unwrap();
    assert_eq!(older, whole);
}

/// DMS-FR-WAUS: the overlay holds the milestone order, a list of milestone
/// item ids from first to last, as `milestoneOrder`. An overlay of an earlier
/// version, with no such field, reads with an empty order, and an empty order
/// writes no field.
#[test]
fn the_overlay_holds_the_milestone_order() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    let value = json!({ "items": {}, "relates": [], "references": [],
                        "milestoneOrder": ["milestone-2", "milestone-1"] });
    write_delivery_overlay(&Silent, buffer, &store, "PVT_1", &overlay(value.clone())).unwrap();
    let read = read_delivery_overlay(&Silent, buffer, &store, "PVT_1").unwrap();
    assert_eq!(read.milestone_order, ["milestone-2", "milestone-1"]);
    assert_eq!(serde_json::to_value(&read).unwrap(), value);

    let earlier = overlay(json!({ "items": {}, "relates": [], "references": [] }));
    assert!(earlier.milestone_order.is_empty(), "an earlier overlay reads with no order");
    let written = serde_json::to_value(&earlier).unwrap();
    assert!(written.get("milestoneOrder").is_none(), "an empty order writes no field");
}

/// DMS-FR-WAUS, DMS-FR-QFEA: a milestone order that names an id more than
/// once is refused with `invalid_overlay`, and nothing is written.
#[test]
fn a_milestone_order_with_a_repeated_id_is_refused() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    let bad = overlay(json!({ "milestoneOrder": ["milestone-1", "milestone-2", "milestone-1"] }));
    let report = validate_overlay(&bad);
    assert!(!report.valid);
    let codes: Vec<&str> = report.violations.iter().map(|v| v.code).collect();
    assert_eq!(codes, [DUPLICATE_MILESTONE_ORDER]);
    let refused = write_delivery_overlay(&Silent, buffer, &store, "PVT_1", &bad).unwrap_err();
    assert_eq!(refused.code, ERR_INVALID_OVERLAY);
    assert!(Fixture::files_under(&canonical(fx.store.path())).is_empty(), "nothing is written");
}
