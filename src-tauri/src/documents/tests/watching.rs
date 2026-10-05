//! The watch over the selected paths (DCL-FR-ZYQC, DCL-FR-XHSJ).

use super::*;
use crate::documents::model::{Availability, DocumentSource};
use crate::documents::watcher::{watch_targets, DocumentsWatcher, WatchTarget, DEBOUNCE};
use crate::documents::log::NullLog;

fn source(kind: SourceKind, path: &str, status: Availability) -> DocumentSource {
    DocumentSource {
        kind,
        path: path.to_string(),
        status,
        reason: None,
    }
}

// DCL-FR-ZYQC: a folder is watched recursively, a file through its parent
// folder, an unavailable source not at all, and a path once however many sources
// reach it.
#[test]
fn targets_follow_the_available_sources() {
    let targets = watch_targets(&[
        source(SourceKind::Folder, "/refs/a/", Availability::Available),
        source(SourceKind::File, "/refs/b/one.md", Availability::Available),
        source(SourceKind::File, "/refs/b/two.md", Availability::Available),
        source(SourceKind::File, "/gone/three.md", Availability::Unavailable),
        source(SourceKind::File, "/top.md", Availability::Available),
    ]);
    assert_eq!(
        targets,
        [
            WatchTarget { path: "/".into(), recursive: false },
            WatchTarget { path: "/refs/a".into(), recursive: true },
            WatchTarget { path: "/refs/b".into(), recursive: false },
        ]
    );
}

// DCL-FR-ZYQC: a change that reaches a watched path is reported once after the
// debounce, a burst of writes costs a refresh for each path and not for each
// event, and stopping the watch ends the reports.
#[test]
fn a_change_under_a_watched_folder_is_reported_and_stop_ends_it() {
    let tmp = TempDir::new().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap();
    let watcher = DocumentsWatcher::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let sender = Mutex::new(sender);
    watcher.watch(
        vec![WatchTarget { path: root.clone(), recursive: true }],
        &NullLog,
        move || {
            let _ = sender.lock().unwrap().send(());
        },
    );
    assert!(watcher.is_armed());
    std::fs::create_dir_all(root.join("sub")).unwrap();
    for i in 0..5 {
        std::fs::write(root.join(format!("sub/f{i}.md")), "x").unwrap();
    }
    receiver
        .recv_timeout(std::time::Duration::from_secs(15))
        .expect("the burst of changes is reported");
    // Wait until the reports stop. A quiet time of three debounces is past the
    // last report that the burst can cause.
    let mut reports = 1;
    while receiver.recv_timeout(DEBOUNCE * 3).is_ok() {
        reports += 1;
    }
    // The debouncer reports one batch of the paths that were quiet for a
    // debounce. How many batches a burst falls in depends on how late the
    // operating system and the debouncer threads run, so a count of "one" would
    // depend on the load of the machine. What does not depend on it: a path is
    // reported once, so the burst costs at most one refresh for each of the
    // paths it touched (the folder, the sub folder, and five files), and not one
    // for each of the many events it caused.
    const PATHS_TOUCHED: usize = 7;
    assert!(
        reports <= PATHS_TOUCHED,
        "a burst touching {PATHS_TOUCHED} paths costs at most that many refreshes, got {reports}"
    );

    watcher.stop();
    assert!(!watcher.is_armed());
    assert!(watcher.targets().is_empty());
    while receiver.try_recv().is_ok() {}
    std::fs::write(root.join("after-stop.md"), "x").unwrap();
    assert!(receiver.recv_timeout(std::time::Duration::from_millis(1500)).is_err());
}

// DCL-FR-ZYQC: a watch with nothing to watch is not armed.
#[test]
fn no_targets_means_no_watch() {
    let watcher = DocumentsWatcher::default();
    watcher.watch(Vec::new(), &NullLog, || {});
    assert!(!watcher.is_armed());
}
