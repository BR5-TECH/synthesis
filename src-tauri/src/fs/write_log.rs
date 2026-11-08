//! A record of the paths that the write primitives wrote, in the order they
//! wrote them. Only test builds have it.
//!
//! A test that must prove the order of several writes cannot compare file
//! times: a file system stamps times in coarse steps, and two writes in one
//! call can get the same stamp. The write primitives in this module tree write
//! every file that the code under test writes, so the log is the order itself
//! and does not depend on any clock.
//!
//! The log is per thread. A test thread sees only the writes it made itself,
//! so tests that run in parallel do not mix their records.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static WRITES: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// Start a record on this thread. A record that was already started is dropped.
pub(crate) fn start() {
    WRITES.with(|w| *w.borrow_mut() = Some(Vec::new()));
}

/// End the record on this thread and return its paths, oldest first. The paths
/// are the ones that the primitives wrote to, as the file system resolved them.
pub(crate) fn finish() -> Vec<PathBuf> {
    WRITES.with(|w| w.borrow_mut().take()).unwrap_or_default()
}

/// Add one path to the record, when a record is on.
pub(super) fn note(path: &Path) {
    WRITES.with(|w| {
        if let Some(writes) = w.borrow_mut().as_mut() {
            writes.push(path.to_path_buf());
        }
    });
}
