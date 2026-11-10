//! FTS-FR-VFNM, FTS-FR-AIDI: the one Flow write of a sync, and a Flow that
//! changed on disk while the sync ran.

use std::cell::{Cell, RefCell};

use super::*;
use crate::flow_sync::serialize::write_flow;
use crate::flow_sync::FlowWrite;

/// A sync that changes the Flow and creates one issue, and the body it read.
fn changing_sync() -> (String, SyncOutcome) {
    let (tracker, mut d) = plan();
    d.items.push(serde_json::from_value(item("t9", "task", "Card form", Some("f1"))).unwrap());
    let read = write_flow(&d);
    let out = sync(&tracker, d);
    assert!(out.doc_changed);
    (read, out)
}

/// FTS-FR-VFNM: a Flow that did not change on disk is written once, with the
/// body of the new plan.
#[test]
fn an_unchanged_flow_is_written_once() {
    let (read, out) = changing_sync();
    let expected = write_flow(&out.doc);
    let saved = RefCell::new(Vec::new());
    let result = FlowWrite {
        read: &read,
        reread: || Some(read.clone()),
        save: |body: &str| {
            saved.borrow_mut().push(body.to_string());
            true
        },
        changed: || panic!("the Flow did not change"),
    }
    .finish(out)
    .unwrap();
    assert_eq!(saved.into_inner(), vec![expected]);
    assert_eq!(result.created, 1);
}

/// FTS-FR-AIDI: a Flow that changed on disk while the sync ran, or that can
/// no longer be read, is not overwritten. The call returns `write_failed`
/// with the URL of each item it created.
#[test]
fn a_flow_that_changed_during_the_sync_is_not_overwritten() {
    for now in [Some("{\"version\":2,\"name\":\"edited\"}\n".to_string()), None] {
        let (read, out) = changing_sync();
        let saves = Cell::new(0);
        let reported = Cell::new(false);
        let error = FlowWrite {
            read: &read,
            reread: || now.clone(),
            save: |_: &str| {
                saves.set(saves.get() + 1);
                true
            },
            changed: || reported.set(true),
        }
        .finish(out)
        .unwrap_err();
        assert_eq!(error.code, ERR_WRITE_FAILED, "{now:?}");
        assert_eq!(error.created_urls, vec![url_of(100)]);
        assert_eq!(saves.get(), 0, "nothing is written");
        assert!(reported.get());
    }
}

/// FTS-FR-AIDI: a save that fails returns `write_failed` with the created
/// URLs.
#[test]
fn a_failed_save_is_write_failed() {
    let (read, out) = changing_sync();
    let error = FlowWrite {
        read: &read,
        reread: || Some(read.clone()),
        save: |_: &str| false,
        changed: || panic!("the Flow did not change"),
    }
    .finish(out)
    .unwrap_err();
    assert_eq!(
        (error.code.as_str(), error.created_urls.clone()),
        (ERR_WRITE_FAILED, vec![url_of(100)])
    );
}

/// FTS-FR-VFNM: a Flow that nothing changed is not written and not read
/// again.
#[test]
fn a_flow_nothing_changed_is_not_written() {
    let (tracker, d) = plan();
    let read = write_flow(&d);
    let out = sync(&tracker, d);
    let result = FlowWrite {
        read: &read,
        reread: || panic!("no second read"),
        save: |_: &str| panic!("no write"),
        changed: || panic!("no change"),
    }
    .finish(out)
    .unwrap();
    assert_eq!(result, SyncResult::default());
}

/// FTS-FR-VFNM / FTS-FR-AIDI: a tracker write that failed is returned with
/// its code and the created URLs after the Flow is written.
#[test]
fn a_tracker_write_error_is_returned_after_the_flow_write() {
    let (tracker, mut d) = plan();
    let tracker = tracker.failing("set_parent", 0);
    d.items.push(serde_json::from_value(item("t9", "task", "Card form", Some("f1"))).unwrap());
    let read = write_flow(&d);
    let out = sync(&tracker, d);
    let saved = Cell::new(false);
    let error = FlowWrite {
        read: &read,
        reread: || Some(read.clone()),
        save: |_: &str| {
            saved.set(true);
            true
        },
        changed: || panic!("the Flow did not change"),
    }
    .finish(out)
    .unwrap_err();
    assert!(saved.get(), "the link of the created issue is kept");
    assert_eq!(
        (error.code.as_str(), error.created_urls.clone()),
        (ERR_GITHUB_UNREACHABLE, vec![url_of(100)])
    );
}
