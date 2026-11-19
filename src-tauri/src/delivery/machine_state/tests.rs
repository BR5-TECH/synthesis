//! The tests of the delivery machine state
//! (`../../../specifications/core/DMS-delivery-machine-state.md`, and
//! `RMS-repository-machine-storage.md` RMS-FR-OCPB).

use std::path::{Path, PathBuf};

use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

mod view;

use super::commands::{load_delivery_layout_with, save_delivery_layout_with};
use super::*;
use crate::delivery::ERR_INVALID_LAYOUT;
use crate::logging::{BufferState, LogBuffer, LogSink};

/// A sink that publishes nothing. The records still reach the buffer.
#[derive(Clone, Default)]
pub(super) struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

/// The text of every record one buffer holds.
pub(super) fn buffer_text(buffer: &LogBuffer) -> String {
    let page = buffer
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..crate::logging::LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the buffer answers");
    page.records.iter().map(|record| format!("{record:?}")).collect::<Vec<_>>().join("\n")
}

pub(super) fn record_count(buffer: &LogBuffer) -> usize {
    buffer.state().buffer_total
}

pub(super) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

/// A worktree and a repository store, apart from each other.
pub(super) struct Fixture {
    pub(super) worktree: TempDir,
    pub(super) store: TempDir,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Fixture { worktree: TempDir::new().unwrap(), store: TempDir::new().unwrap() }
    }

    pub(super) fn store_fs(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(canonical(self.store.path()))
    }

    pub(super) fn files_under(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path.strip_prefix(dir).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        out
    }

    /// A mock app with the worktree open and the store named.
    pub(super) fn app(&self) -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(crate::project::ProjectState::default());
        let root = canonical(self.worktree.path());
        let store = canonical(self.store.path());
        let access = crate::fs::FsAccess::builder()
            .allow_root(&root)
            .allow_root(&store)
            .build()
            .map(std::sync::Arc::new)
            .unwrap();
        let state = app.state::<crate::project::ProjectState>();
        state.set_root_with_access(root, Some(access));
        state.set_store(store);
        app
    }
}

fn layout(value: serde_json::Value) -> DeliveryLayout {
    serde_json::from_value(value).unwrap()
}

/// DMS-FR-DMWD, DMS-FR-TJSJ: a scope is `github:<project-node-id>` or
/// `flow:<artifact-id>`, with a text after the prefix.
#[test]
fn a_scope_is_a_github_project_or_a_flow_file() {
    assert_eq!(parse_scope("github:PVT_kwDO1"), Some(LayoutScope::Github("PVT_kwDO1")));
    assert_eq!(parse_scope("flow:plans/beta.flow"), Some(LayoutScope::Flow("plans/beta.flow")));
    for bad in ["", "github:", "flow:", "file:plans/a.flow", "plans/a.flow", "GITHUB:x"] {
        assert_eq!(parse_scope(bad), None, "{bad:?}");
    }
}

/// DMS contract surface: a digest is the first 32 lowercase hexadecimal
/// characters of the SHA-256 of the text.
#[test]
fn a_digest_is_the_sha256_prefix() {
    let digest = scope_digest("flow:plans/beta.flow");
    assert_eq!(digest.len(), 32);
    assert!(digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    // SHA-256 of the empty string starts e3b0c442...
    assert_eq!(scope_digest(""), "e3b0c44298fc1c149afbf4c8996fb924");
    assert_ne!(scope_digest("flow:a"), scope_digest("github:a"));
}

/// DMS-FR-KITP, RMS-FR-OCPB, DMS-FR-DPXR, DMS-FR-XAZV: a saved layout is the
/// record `delivery/layout/<digest>.json` of the repository store, nothing is
/// written in the worktree, and a load returns it. The record serializes in
/// the frontend shape of `DeliveryLayout`.
#[test]
fn a_layout_round_trips_in_the_repository_store() {
    let fx = Fixture::new();
    let app = fx.app();
    let buffer = LogBuffer::for_one_test();
    let scope = "flow:plans/beta.flow";
    let value = json!({ "items": {
        "m1": { "position": { "x": 40.0, "y": 40.0 }, "size": { "width": 900.0, "height": 520.0 },
                "collapsed": true },
        "t1": { "position": { "x": -20.5, "y": 60.0 } }
    } });
    save_delivery_layout_with(app.handle(), buffer, scope, &layout(value.clone())).unwrap();

    let expected = format!("delivery/layout/{}.json", scope_digest(scope));
    assert_eq!(Fixture::files_under(&canonical(fx.store.path())), vec![expected]);
    assert!(Fixture::files_under(&canonical(fx.worktree.path())).is_empty());

    let loaded = load_delivery_layout_with(app.handle(), buffer, scope).unwrap();
    assert_eq!(serde_json::to_value(&loaded).unwrap(), value);
    // Another scope holds nothing (DMS-FR-DPXR).
    let other = load_delivery_layout_with(app.handle(), buffer, "github:PVT_1").unwrap();
    assert!(other.items.is_empty());
    // DMS-FR-ROMF: the save and the loads log the scope kind, not the scope.
    let text = buffer_text(buffer);
    assert!(text.contains("delivery layout saved"), "{text}");
    assert!(!text.contains("plans/beta.flow") && !text.contains("PVT_1"), "{text}");
}

/// DMS-FR-XAZV: a save replaces the whole layout of the scope, and an empty
/// layout removes the record. Removing an absent record is not an error.
#[test]
fn a_save_replaces_the_layout_and_an_empty_one_removes_it() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "github:PVT_kwDO1";
    save_layout(&store, scope, &layout(json!({ "items": { "a": { "collapsed": true } } })))
        .unwrap();
    save_layout(&store, scope, &layout(json!({ "items": { "b": { "collapsed": false } } })))
        .unwrap();
    let loaded = load_layout(&Silent, LogBuffer::for_one_test(), &store, scope).unwrap();
    assert_eq!(loaded.items.keys().collect::<Vec<_>>(), vec!["b"]);

    save_layout(&store, scope, &DeliveryLayout::default()).unwrap();
    assert!(Fixture::files_under(&canonical(fx.store.path())).is_empty());
    save_layout(&store, scope, &DeliveryLayout::default()).unwrap();
}

/// DMS-FR-TJSJ: a scope not of the form of DMS-FR-DMWD, a position or a size
/// that is not finite, and a size that is not positive are refused with
/// `invalid_layout`, and nothing is written. A stored layout stays as it was.
#[test]
fn an_invalid_layout_is_refused_and_writes_nothing() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    let good = layout(json!({ "items": { "a": { "position": { "x": 1.0, "y": 2.0 } } } }));
    save_layout(&store, scope, &good).unwrap();

    let point =
        |x: f64, y: f64| LayoutEntry { position: Some(Point { x, y }), ..Default::default() };
    let extent = |width: f64, height: f64| LayoutEntry {
        size: Some(Extent { width, height }),
        ..Default::default()
    };
    let refused = [
        point(f64::NAN, 0.0),
        point(0.0, f64::INFINITY),
        extent(f64::NEG_INFINITY, 10.0),
        extent(0.0, 10.0),
        extent(10.0, -1.0),
    ];
    for entry in refused {
        let mut bad = good.clone();
        bad.items.insert("x".into(), entry.clone());
        let err = save_layout(&store, scope, &bad).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_LAYOUT, "{entry:?}");
    }
    for bad_scope in ["", "flow:", "file:a.flow"] {
        let err = save_layout(&store, bad_scope, &good).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_LAYOUT, "{bad_scope:?}");
        // An empty layout for a bad scope is refused too, and removes nothing.
        let err = save_layout(&store, bad_scope, &DeliveryLayout::default()).unwrap_err();
        assert_eq!(err.code, ERR_INVALID_LAYOUT);
    }
    assert_eq!(Fixture::files_under(&canonical(fx.store.path())).len(), 1);
    assert_eq!(load_layout(&Silent, LogBuffer::for_one_test(), &store, scope).unwrap(), good);

    // A negative position is a position, not an error.
    let mut negative = good.clone();
    negative.items.insert("y".into(), point(-5.0, -7.5));
    save_layout(&store, scope, &negative).unwrap();
}

/// DMS-FR-IHML: each command refuses with `no_project_open` when no project
/// is open, and with `store_unavailable` where the store is unavailable. Each
/// refusal is logged with the scope kind and not the scope.
#[test]
fn the_commands_refuse_without_a_project_or_a_store() {
    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    let buffer = LogBuffer::for_one_test();
    let scope = "flow:secret/plan.flow";
    let err = load_delivery_layout_with(app.handle(), buffer, scope).unwrap_err();
    assert_eq!(err.code, "no_project_open");
    let err = save_delivery_layout_with(app.handle(), buffer, scope, &DeliveryLayout::default())
        .unwrap_err();
    assert_eq!(err.code, "no_project_open");

    // A root with no filesystem access cannot reach a store.
    let dir = TempDir::new().unwrap();
    app.state::<crate::project::ProjectState>().set_root_with_access(canonical(dir.path()), None);
    let err = load_delivery_layout_with(app.handle(), buffer, scope).unwrap_err();
    assert_eq!(err.code, "store_unavailable");
    let err = save_delivery_layout_with(app.handle(), buffer, scope, &DeliveryLayout::default())
        .unwrap_err();
    assert_eq!(err.code, "store_unavailable");

    let text = buffer_text(buffer);
    assert_eq!(text.matches("delivery layout operation refused").count(), 4, "{text}");
    assert!(text.contains("\"flow\""), "{text}");
    assert!(!text.contains("secret/plan.flow"), "{text}");
}

/// DMS-FR-ETLQ: a record that cannot be parsed reads as empty, and the read
/// logs one record with the record kind and no content.
#[test]
fn an_unreadable_record_reads_as_empty_and_logs_once() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let scope = "flow:a.flow";
    let digest = scope_digest(scope);
    let dir = canonical(fx.store.path()).join("delivery");
    for (kind, file) in [
        ("layout", digest.clone()),
        ("overlay", "PVT_1".into()),
        ("publish", scope_digest("a.flow")),
    ] {
        std::fs::create_dir_all(dir.join(kind)).unwrap();
        std::fs::write(dir.join(kind).join(format!("{file}.json")), "{ \"footprint\": [\"src/se")
            .unwrap();
    }

    let buffer = LogBuffer::for_one_test();
    assert!(load_layout(&Silent, buffer, &store, scope).unwrap().items.is_empty());
    assert_eq!(record_count(buffer), 1);
    assert_eq!(
        read_delivery_overlay(&Silent, buffer, &store, "PVT_1").unwrap(),
        DeliveryOverlay::default()
    );
    assert_eq!(record_count(buffer), 2);
    assert_eq!(
        read_publish_journal(&Silent, buffer, &store, "a.flow").unwrap(),
        PublishJournal::default()
    );
    assert_eq!(record_count(buffer), 3);
    let text = buffer_text(buffer);
    for kind in ["\"layout\"", "\"overlay\"", "\"publish_journal\""] {
        assert!(text.contains(kind), "{kind} in {text}");
    }
    assert!(!text.contains("src/se"), "{text}");

    // A record that parses but holds another shape reads as empty too.
    std::fs::write(dir.join("layout").join(format!("{digest}.json")), "[1, 2]").unwrap();
    assert!(load_layout(&Silent, buffer, &store, scope).unwrap().items.is_empty());
    assert_eq!(record_count(buffer), 4);
}

/// DMS-FR-EKKY: a layout entry or an overlay entry that names an item the
/// plan does not hold is kept, and a read returns it for the reader to
/// ignore.
#[test]
fn entries_of_unknown_items_are_kept() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let gone = layout(json!({ "items": { "gone-item": { "collapsed": true } } }));
    save_layout(&store, "github:PVT_1", &gone).unwrap();
    assert_eq!(
        load_layout(&Silent, LogBuffer::for_one_test(), &store, "github:PVT_1").unwrap(),
        gone
    );

    let mut overlay = DeliveryOverlay::default();
    overlay.items.insert(
        "gone-item".into(),
        OverlayEntry { draft_id: Some("d1".into()), ..Default::default() },
    );
    write_delivery_overlay(&Silent, LogBuffer::for_one_test(), &store, "PVT_1", &overlay).unwrap();
    let read = read_delivery_overlay(&Silent, LogBuffer::for_one_test(), &store, "PVT_1").unwrap();
    assert_eq!(read, overlay);
}

/// DMS-FR-IHML, RMS-FR-HAJC: a record that the store cannot read for a cause
/// other than its absence is `store_unavailable`, not an empty answer; and a
/// write that cannot complete is `store_unavailable` too.
#[test]
fn an_unreadable_store_is_store_unavailable() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let dir = canonical(fx.store.path()).join("delivery");
    let scope = "flow:a.flow";
    // A folder where each record should be.
    for (kind, file) in [
        ("layout", scope_digest(scope)),
        ("overlay", "PVT_1".to_string()),
        ("publish", scope_digest("a.flow")),
    ] {
        std::fs::create_dir_all(dir.join(kind).join(format!("{file}.json")).join("x")).unwrap();
    }
    let buffer = LogBuffer::for_one_test();
    let code = |e: crate::delivery::DeliveryError| e.code;
    assert_eq!(code(load_layout(&Silent, buffer, &store, scope).unwrap_err()), "store_unavailable");
    assert_eq!(
        code(read_delivery_overlay(&Silent, buffer, &store, "PVT_1").unwrap_err()),
        "store_unavailable"
    );
    assert_eq!(
        code(read_publish_journal(&Silent, buffer, &store, "a.flow").unwrap_err()),
        "store_unavailable"
    );
    let entry = layout(json!({ "items": { "a": { "collapsed": true } } }));
    assert_eq!(code(save_layout(&store, scope, &entry).unwrap_err()), "store_unavailable");
    assert!(dir.join("layout").join(format!("{}.json", scope_digest(scope))).is_dir());
}

/// DMS contract surface: a record names its scope or its Flow. A record of
/// another scope, or of another Flow, at the same digest path reads as empty.
#[test]
fn a_record_of_another_scope_reads_as_empty() {
    let fx = Fixture::new();
    let store = fx.store_fs();
    let dir = canonical(fx.store.path()).join("delivery");
    std::fs::create_dir_all(dir.join("layout")).unwrap();
    std::fs::create_dir_all(dir.join("publish")).unwrap();
    std::fs::write(
        dir.join("layout").join(format!("{}.json", scope_digest("flow:a.flow"))),
        r#"{ "scope": "flow:b.flow", "items": { "x": { "collapsed": true } } }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("publish").join(format!("{}.json", scope_digest("a.flow"))),
        r#"{ "flowId": "b.flow", "items": {}, "edges": { "e1": { "blocked": 2, "blocking": 1 } } }"#,
    )
    .unwrap();
    let buffer = LogBuffer::for_one_test();
    assert!(load_layout(&Silent, buffer, &store, "flow:a.flow").unwrap().items.is_empty());
    assert_eq!(
        read_publish_journal(&Silent, buffer, &store, "a.flow").unwrap(),
        PublishJournal::default()
    );
}
