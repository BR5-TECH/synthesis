//! The tests of the view of a delivery canvas
//! (`../../../../specifications/core/DMS-delivery-machine-state.md` DMS-FR-TXMH,
//! DMS-FR-JBKN, DMS-FR-TICX, DMS-FR-XUYV, DMS-FR-ETLQ, DMS-FR-IHML,
//! DMS-FR-ROMF).

use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

use super::super::commands::{load_delivery_view_with, save_delivery_view_with};
use super::super::*;
use super::{buffer_text, canonical, record_count, Fixture, Silent};
use crate::delivery::ERR_INVALID_VIEW;
use crate::logging::LogBuffer;

fn view(value: serde_json::Value) -> DeliveryView {
    serde_json::from_value(value).unwrap()
}

fn view_path(scope: &str) -> String {
    format!("delivery/view/{}.json", scope_digest(scope))
}

// DMS-FR-TXMH, DMS-FR-JBKN, DMS-FR-TICX, RMS-FR-OCPB, DMS-FR-ROMF
/// A saved view is the record `delivery/view/<digest>.json` of the store,
/// nothing is written in the worktree, and a load returns it in the frontend
/// shape of `DeliveryView`. The log holds the scope kind and counts, and no
/// scope.
#[test]
fn a_view_round_trips_in_the_repository_store() {
    let fx = Fixture::new();
    let app = fx.app();
    let buffer = LogBuffer::for_one_test();
    let scope = "flow:plans/secret-view.flow";
    let value = json!({ "hidden": ["done", "in review"], "blockedOnly": true });
    save_delivery_view_with(app.handle(), buffer, scope, &value).unwrap();

    assert_eq!(Fixture::files_under(&canonical(fx.store.path())), vec![view_path(scope)]);
    assert!(Fixture::files_under(&canonical(fx.worktree.path())).is_empty());

    let loaded = load_delivery_view_with(app.handle(), buffer, scope).unwrap();
    assert_eq!(serde_json::to_value(&loaded).unwrap()["blockedOnly"], json!(true));
    let mut hidden: Vec<_> =
        serde_json::to_value(&loaded).unwrap()["hidden"].as_array().unwrap().clone();
    hidden.sort_by_key(|v| v.as_str().unwrap().to_string());
    assert_eq!(hidden, vec![json!("done"), json!("in review")]);

    let text = buffer_text(buffer);
    assert!(text.contains("delivery view saved"), "{text}");
    assert!(text.contains("delivery view loaded"), "{text}");
    assert!(text.contains("\"flow\""), "{text}");
    assert!(!text.contains("secret-view"), "{text}");
}

// DMS-FR-TXMH, DMS-FR-JBKN
/// Each board status is a word of the contract, and a scope with no stored
/// view reads the empty view.
#[test]
fn every_board_status_is_accepted_and_the_default_is_empty() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    let scope = "github:PVT_kwDO1";
    assert_eq!(load_view(&Silent, buffer, &store, scope).unwrap(), DeliveryView::default());

    let all = json!({ "hidden": ["backlog", "ready", "in progress", "in review", "done"] });
    save_view(&store, scope, &view(all.clone())).unwrap();
    let loaded = load_view(&Silent, buffer, &store, scope).unwrap();
    assert_eq!(serde_json::to_value(&loaded).unwrap(), {
        let mut expected = all;
        expected["blockedOnly"] = json!(false);
        expected
    });
    // Another scope still reads the empty view.
    assert_eq!(
        load_view(&Silent, buffer, &store, "github:PVT_other").unwrap(),
        DeliveryView::default()
    );
}

// DMS-FR-TXMH
/// `hidden` is a set: a status given twice is stored once.
#[test]
fn a_repeated_status_is_stored_once() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    save_view(&store, scope, &view(json!({ "hidden": ["done", "done", "ready"] }))).unwrap();
    let loaded = load_view(&Silent, LogBuffer::for_one_test(), &store, scope).unwrap();
    assert_eq!(loaded.hidden, vec![BoardStatus::Ready, BoardStatus::Done]);
}

// DMS-FR-TICX
/// A save replaces the whole view, and the empty view removes the record.
/// Removing an absent record is not an error.
#[test]
fn a_save_replaces_the_view_and_the_empty_view_removes_it() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    save_view(&store, scope, &view(json!({ "hidden": ["done"] }))).unwrap();
    save_view(&store, scope, &view(json!({ "blockedOnly": true }))).unwrap();
    let loaded = load_view(&Silent, LogBuffer::for_one_test(), &store, scope).unwrap();
    assert_eq!(loaded, DeliveryView { hidden: vec![], blocked_only: true });

    save_view(&store, scope, &DeliveryView::default()).unwrap();
    assert!(Fixture::files_under(&canonical(fx.store.path())).is_empty());
    save_view(&store, scope, &DeliveryView::default()).unwrap();
}

// DMS-FR-TICX
/// A scope not of the form of DMS-FR-DMWD, and a status that is not a board
/// status, are refused with `invalid_view`. Nothing is written, and a stored
/// view stays as it was.
#[test]
fn an_invalid_scope_or_status_is_refused_and_writes_nothing() {
    let fx = Fixture::new();
    let app = fx.app();
    let buffer = LogBuffer::for_one_test();
    let scope = "flow:a.flow";
    let good = json!({ "hidden": ["done"] });
    save_delivery_view_with(app.handle(), buffer, scope, &good).unwrap();

    for bad_scope in ["", "flow:", "github:", "file:a.flow"] {
        let err = save_delivery_view_with(app.handle(), buffer, bad_scope, &good).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_VIEW, "{bad_scope:?}");
        let err = save_view(&fx.store_fs(), bad_scope, &DeliveryView::default()).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_VIEW, "{bad_scope:?}");
    }
    for bad in [
        json!({ "hidden": ["blocked"] }),
        json!({ "hidden": ["Done"] }),
        json!({ "hidden": ["in_progress"] }),
        json!({ "hidden": "done" }),
        json!({ "blockedOnly": "yes" }),
        json!(null),
    ] {
        let err = save_delivery_view_with(app.handle(), buffer, scope, &bad).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_VIEW, "{bad}");
    }
    assert_eq!(Fixture::files_under(&canonical(fx.store.path())), vec![view_path(scope)]);
    let loaded = load_delivery_view_with(app.handle(), buffer, scope).unwrap();
    assert_eq!(loaded.hidden, vec![BoardStatus::Done]);
    let text = buffer_text(buffer);
    assert_eq!(text.matches("delivery view operation refused").count(), 10, "{text}");
}

// DMS-FR-XUYV
/// The layout and the view of a scope are separate records. An empty layout
/// save leaves the view, and a view save leaves the layout.
#[test]
fn the_layout_and_the_view_are_separate_records() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let buffer = LogBuffer::for_one_test();
    let scope = "github:PVT_1";
    let stored_layout: DeliveryLayout =
        serde_json::from_value(json!({ "items": { "a": { "collapsed": true } } })).unwrap();
    save_layout(&store, scope, &stored_layout).unwrap();
    save_view(&store, scope, &view(json!({ "hidden": ["done"] }))).unwrap();

    save_layout(&store, scope, &DeliveryLayout::default()).unwrap();
    assert_eq!(load_view(&Silent, buffer, &store, scope).unwrap().hidden, vec![BoardStatus::Done]);
    assert_eq!(Fixture::files_under(&canonical(fx.store.path())), vec![view_path(scope)]);

    save_layout(&store, scope, &stored_layout).unwrap();
    save_view(&store, scope, &DeliveryView::default()).unwrap();
    assert_eq!(load_layout(&Silent, buffer, &store, scope).unwrap(), stored_layout);
    save_view(&store, scope, &view(json!({ "blockedOnly": true }))).unwrap();
    assert_eq!(load_layout(&Silent, buffer, &store, scope).unwrap(), stored_layout);
}

// DMS-FR-ETLQ
/// A view record that cannot be parsed, or that holds an unknown status,
/// reads as the empty view, and the read logs one record with the kind `view`
/// and no content.
#[test]
fn an_unreadable_view_reads_as_empty_and_logs_once() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    let dir = canonical(fx.store.path()).join("delivery").join("view");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{}.json", scope_digest(scope)));
    std::fs::write(&file, "{ \"scope\": \"flow:a.flow\", \"view\": { \"hidden\": [\"secret-wo")
        .unwrap();

    let buffer = LogBuffer::for_one_test();
    assert_eq!(load_view(&Silent, buffer, &store, scope).unwrap(), DeliveryView::default());
    assert_eq!(record_count(buffer), 1);
    let text = buffer_text(buffer);
    assert!(text.contains("\"view\""), "{text}");
    assert!(!text.contains("secret-wo"), "{text}");

    std::fs::write(&file, r#"{ "scope": "flow:a.flow", "view": { "hidden": ["archived"] } }"#)
        .unwrap();
    assert_eq!(load_view(&Silent, buffer, &store, scope).unwrap(), DeliveryView::default());
    assert_eq!(record_count(buffer), 2);
}

// DMS contract surface
/// A record of another scope at the same digest path reads as the empty view.
#[test]
fn a_view_of_another_scope_reads_as_empty() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let dir = canonical(fx.store.path()).join("delivery").join("view");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.json", scope_digest("flow:a.flow"))),
        r#"{ "scope": "flow:b.flow", "view": { "hidden": ["done"] } }"#,
    )
    .unwrap();
    let loaded = load_view(&Silent, LogBuffer::for_one_test(), &store, "flow:a.flow").unwrap();
    assert_eq!(loaded, DeliveryView::default());
}

// DMS-FR-IHML, DMS-FR-ROMF
/// Each view command refuses with `no_project_open` when no project is open,
/// and with `store_unavailable` where the store is unavailable. Each refusal
/// is logged with the scope kind and not the scope.
#[test]
fn the_view_commands_refuse_without_a_project_or_a_store() {
    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    let buffer = LogBuffer::for_one_test();
    let scope = "flow:secret/plan.flow";
    let empty = json!({ "hidden": [], "blockedOnly": false });
    let err = load_delivery_view_with(app.handle(), buffer, scope).unwrap_err();
    assert_eq!(err.code, "no_project_open");
    let err = save_delivery_view_with(app.handle(), buffer, scope, &empty).unwrap_err();
    assert_eq!(err.code, "no_project_open");

    let dir = TempDir::new().unwrap();
    app.state::<crate::project::ProjectState>().set_root_with_access(canonical(dir.path()), None);
    let err = load_delivery_view_with(app.handle(), buffer, scope).unwrap_err();
    assert_eq!(err.code, "store_unavailable");
    let err = save_delivery_view_with(app.handle(), buffer, scope, &empty).unwrap_err();
    assert_eq!(err.code, "store_unavailable");

    let text = buffer_text(buffer);
    assert_eq!(text.matches("delivery view operation refused").count(), 4, "{text}");
    assert!(text.contains("\"flow\""), "{text}");
    assert!(!text.contains("secret/plan.flow"), "{text}");
}

// DMS-FR-IHML, RMS-FR-HAJC
/// A view record that the store cannot read for a cause other than its
/// absence is `store_unavailable`, and so is a write that cannot complete.
#[test]
fn an_unreadable_view_store_is_store_unavailable() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    let path = canonical(fx.store.path())
        .join("delivery")
        .join("view")
        .join(format!("{}.json", scope_digest(scope)));
    std::fs::create_dir_all(path.join("x")).unwrap();
    let buffer = LogBuffer::for_one_test();
    assert_eq!(load_view(&Silent, buffer, &store, scope).unwrap_err().code, "store_unavailable");
    let err = save_view(&store, scope, &view(json!({ "hidden": ["done"] }))).unwrap_err();
    assert_eq!(err.code, "store_unavailable");
    assert!(path.is_dir());
}
