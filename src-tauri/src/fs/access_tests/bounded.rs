//! The bounded read, the read-time check of a granted path, the `other` entry
//! kind, the omission of a vanished entry, and the instance that is kept while
//! the grants stay the same (FSA-FR-22, FSA-FR-NRQE, FSA-FR-LKTD, FSA-FR-WBKZ).

use std::io;
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::fs::{EntryKind, FsAccessState};

#[cfg(unix)]
fn make_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid, NUL-terminated string that lives for the call.
    let status = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    assert_eq!(status, 0, "the FIFO is created");
}

/// Run `work` on its own thread and fail the test when it does not end, so a
/// read that blocks is a failed test rather than a stuck one.
fn within_five_seconds<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(work());
    });
    receive
        .recv_timeout(Duration::from_secs(5))
        .expect("the operation ended")
}

// FSA-FR-NRQE: a regular file is read whole, up to the limit.
#[test]
fn a_regular_file_is_read_up_to_the_limit() {
    let (_tmp, access, root) = rooted();
    write_raw(&root.join("a.txt"), "twelve bytes");
    assert_eq!(access.read_regular_bytes(root.join("a.txt"), 12).unwrap(), b"twelve bytes");
    assert!(matches!(
        access.read_regular_bytes(root.join("a.txt"), 11),
        Err(FsError::TooLarge { limit: 11, .. })
    ));
    assert!(matches!(
        access.read_regular_bytes(root.join("missing.txt"), 12),
        Err(FsError::NotFound { .. })
    ));
}

// FSA-FR-NRQE: a directory is not a regular file.
#[test]
fn a_directory_is_not_a_regular_file() {
    let (_tmp, access, root) = rooted();
    fs::create_dir_all(root.join("folder")).unwrap();
    assert!(matches!(
        access.read_regular_bytes(root.join("folder"), 100),
        Err(FsError::NotRegular { .. })
    ));
}

// FSA-FR-22, FSA-FR-NRQE: a FIFO is listed as `other`, and a read of it is
// refused without blocking.
#[cfg(unix)]
#[test]
fn a_fifo_is_listed_as_other_and_never_blocks_a_read() {
    let (_tmp, access, root) = rooted();
    make_fifo(&root.join("pipe.txt"));
    write_raw(&root.join("real.txt"), "real");
    let kinds: Vec<(String, EntryKind)> = access
        .list_dir(&root)
        .unwrap()
        .into_iter()
        .map(|e| (e.name, e.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("pipe.txt".to_string(), EntryKind::Other),
            ("real.txt".to_string(), EntryKind::File),
        ]
    );
    assert_eq!(access.file_info(root.join("pipe.txt")).unwrap().kind, EntryKind::Other);
    let path = root.join("pipe.txt");
    let result = within_five_seconds(move || access.read_regular_bytes(&path, 100));
    assert!(matches!(result, Err(FsError::NotRegular { .. })), "{result:?}");
}

// FSA-FR-NRQE: a link at the path is refused, and the target is not read.
#[cfg(unix)]
#[test]
fn a_link_is_refused_by_the_bounded_read() {
    let (_tmp, access, root) = rooted();
    write_raw(&root.join("target.txt"), "target");
    symlink(root.join("target.txt"), root.join("link.txt")).unwrap();
    assert!(matches!(
        access.read_regular_bytes(root.join("link.txt"), 100),
        Err(FsError::SymlinkRefused { .. })
    ));
}

// FSA-FR-22: an entry that disappears between the directory read and its
// description is omitted and counted, and the listing still succeeds.
#[test]
fn a_vanished_entry_is_omitted_and_counted() {
    let (_tmp, access, root) = rooted();
    write_raw(&root.join("stays.md"), "stays");
    write_raw(&root.join("goes.md"), "goes");
    let describe = |entry: &Path| {
        if entry.file_name().is_some_and(|n| n == "goes.md") {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            fs::symlink_metadata(entry)
        }
    };
    let (entries, omitted) = access.list_dir_described(&root, &describe).unwrap();
    assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["stays.md"]);
    assert_eq!(omitted, 1);
    let (_, none) = access.list_dir_reporting(&root).unwrap();
    assert_eq!(none, 0);
}

/// A selection of one file and one folder, built into a documents instance.
struct Granted {
    _tmp: TempDir,
    root: PathBuf,
}

impl Granted {
    fn new() -> Granted {
        let tmp = TempDir::new().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        write_raw(&root.join("granted.md"), "granted");
        write_raw(&root.join("folder/inside.md"), "inside");
        write_raw(&root.join("secret.txt"), "secret");
        Granted { _tmp: tmp, root }
    }
}

// FSA-FR-LKTD, FSA-FR-KAQV: a granted file that is swapped for a link after the
// build reaches nothing, whatever the link points at.
#[cfg(unix)]
#[test]
fn a_granted_file_swapped_for_a_link_is_refused_at_read_time() {
    let g = Granted::new();
    let access = FsAccess::builder()
        .read_only_file(g.root.join("granted.md"))
        .build()
        .unwrap();
    assert_eq!(access.read_text(g.root.join("granted.md")).unwrap(), "granted");

    fs::remove_file(g.root.join("granted.md")).unwrap();
    symlink(g.root.join("secret.txt"), g.root.join("granted.md")).unwrap();

    let path = g.root.join("granted.md");
    assert!(matches!(access.read_bytes(&path), Err(FsError::SymlinkRefused { .. })));
    assert!(matches!(access.read_text(&path), Err(FsError::SymlinkRefused { .. })));
    assert!(matches!(access.sha256_file(&path), Err(FsError::SymlinkRefused { .. })));
    assert!(matches!(
        access.read_regular_bytes(&path, 100),
        Err(FsError::SymlinkRefused { .. })
    ));
    assert!(matches!(
        access.open_file(&path, OpenMode::ReadOnly),
        Err(FsError::SymlinkRefused { .. })
    ));
    // The file is still described as the link it now is.
    assert_eq!(access.file_info(&path).unwrap().kind, EntryKind::Symlink);
}

// FSA-FR-LKTD: a granted folder that is swapped for a link after the build
// reaches nothing.
#[cfg(unix)]
#[test]
fn a_granted_folder_swapped_for_a_link_is_refused_at_read_time() {
    let g = Granted::new();
    let access = FsAccess::builder()
        .read_only_folder(g.root.join("folder"))
        .build()
        .unwrap();
    assert_eq!(access.read_text(g.root.join("folder/inside.md")).unwrap(), "inside");

    fs::remove_dir_all(g.root.join("folder")).unwrap();
    symlink(&g.root, g.root.join("folder")).unwrap();

    assert!(matches!(
        access.read_bytes(g.root.join("folder/secret.txt")),
        Err(FsError::SymlinkRefused { .. })
    ));
    assert!(matches!(
        access.list_dir(g.root.join("folder")),
        Err(FsError::SymlinkRefused { .. })
    ));
}

// FSA-FR-WBKZ: a refresh that finds the same grantable sources keeps the same
// instance, and a changed set builds a new one.
#[test]
fn an_unchanged_source_set_keeps_the_same_instance() {
    let g = Granted::new();
    let state = FsAccessState::default();
    let files = [g.root.join("granted.md")];
    let folders = [g.root.join("folder")];

    state.install_documents(&files, &folders);
    let first = state.documents().unwrap();
    state.install_documents(&files, &folders);
    assert!(Arc::ptr_eq(&first, &state.documents().unwrap()), "same sources, same instance");

    // The order the sources come in does not matter.
    state.install_documents(&[], &folders);
    let narrowed = state.documents().unwrap();
    assert!(!Arc::ptr_eq(&first, &narrowed), "a removed source builds a new instance");
    state.install_documents(&[], &folders);
    assert!(Arc::ptr_eq(&narrowed, &state.documents().unwrap()));
}

// FSA-FR-WBKZ: a source that cannot be granted does not change the instance, and
// the instance changes when that source becomes grantable.
#[test]
fn a_source_becoming_grantable_builds_a_new_instance() {
    let g = Granted::new();
    let state = FsAccessState::default();
    let files = [g.root.join("granted.md"), g.root.join("later.md")];
    let refused = state.install_documents(&files, &[]);
    assert_eq!(refused.len(), 1);
    let first = state.documents().unwrap();

    let refused = state.install_documents(&files, &[]);
    assert_eq!(refused.len(), 1, "the missing source is still reported");
    assert!(Arc::ptr_eq(&first, &state.documents().unwrap()));

    write_raw(&g.root.join("later.md"), "later");
    let refused = state.install_documents(&files, &[]);
    assert!(refused.is_empty());
    let rebuilt = state.documents().unwrap();
    assert!(!Arc::ptr_eq(&first, &rebuilt));
    assert_eq!(rebuilt.read_text(g.root.join("later.md")).unwrap(), "later");

    state.discard_documents();
    assert!(state.documents().is_none());
    state.install_documents(&files, &[]);
    assert!(state.documents().is_some(), "an install after a discard builds again");
}
