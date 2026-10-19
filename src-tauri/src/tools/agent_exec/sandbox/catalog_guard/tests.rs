//! EAC-FR-NDDZ, EAC-FR-PIHS, FSA-FR-ZANF: the catalog guard as a process.
//!
//! A test cannot start this executable as the application, because the test
//! binary is not the application. It starts the test binary instead, which
//! runs `guard_child_entry` alone and holds the lock as the guard does.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::*;
use crate::fs::{CreateMode, FsAccess};

/// The variable that makes `guard_child_entry` act as the guard.
const CHILD_ENV: &str = "SYNTHESIS_CATALOG_GUARD_TEST";

/// The entry the started test binary runs: the guard, where the variable names
/// a catalog, and nothing otherwise.
#[test]
fn guard_child_entry() {
    if let Some(catalog) = std::env::var_os(CHILD_ENV) {
        std::process::exit(hold(Path::new(&catalog)));
    }
}

/// The command that starts the test binary as the guard of `catalog`.
pub(crate) fn test_command(catalog: &Path) -> Command {
    let module = module_path!();
    let module = module.split_once("::").map_or(module, |(_, rest)| rest);
    let entry = format!("{module}::guard_child_entry");
    let mut command = Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args(["--exact", entry.as_str(), "--nocapture", "--test-threads=1", "-q"])
        .env(CHILD_ENV, catalog);
    command
}

/// A catalog file in a directory of its own.
pub(crate) fn catalog() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().canonicalize().expect("a canonical directory");
    let path = root.join("msb.db");
    let fs = FsAccess::builder().allow_root(&root).build().expect("access");
    fs.create_file(&path, CreateMode::Exclusive).expect("the catalog file");
    (dir, path)
}

/// Open `path` for reading and writing, as a process that wants to write to
/// the catalog does.
fn writer(path: &Path) -> std::fs::File {
    std::fs::OpenOptions::new().read(true).write(true).open(path).expect("the catalog")
}

fn range_lock(kind: libc::c_short) -> libc::flock {
    // SAFETY: `flock` is a plain C struct, and all zero bits is a valid value.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = kind;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock.l_start = SHARED_FIRST as libc::off_t;
    lock.l_len = SHARED_SIZE as libc::off_t;
    lock
}

/// The process that holds a lock which stops a write lock on the shared range,
/// as SQLite's exclusive lock asks for it.
fn holder_of_shared_range(path: &Path) -> Option<i32> {
    let file = writer(path);
    let mut lock = range_lock(libc::F_WRLCK as libc::c_short);
    // SAFETY: the descriptor is open for the call, and `lock` outlives it.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) };
    assert_ne!(result, -1, "the lock query failed: {}", std::io::Error::last_os_error());
    (lock.l_type != libc::F_UNLCK as libc::c_short).then_some(lock.l_pid)
}

// EAC-FR-NDDZ, FSA-FR-ZANF: the guard holds a read lock on SQLite's shared range,
// so no process can take the exclusive lock, and the lock ends with the guard.
#[test]
fn a_guard_holds_the_shared_range_until_it_ends() {
    let (_dir, path) = catalog();
    let mut guard = CatalogGuard::start(test_command(&path), LOCK_BOUND).expect("the guard");
    let pid = i32::try_from(guard.pid()).expect("a process ID");
    assert_eq!(holder_of_shared_range(&path), Some(pid), "the guard holds the range");
    assert_eq!(guard.exited(), None, "the guard runs");
    drop(guard);
    assert_eq!(holder_of_shared_range(&path), None, "an ended guard holds nothing");
}

// EAC-FR-NDDZ: the guard ends when its standard input closes, which is what
// happens when the application ends.
#[test]
fn a_guard_ends_when_its_input_closes() {
    let (_dir, path) = catalog();
    let mut guard = CatalogGuard::start(test_command(&path), LOCK_BOUND).expect("the guard");
    guard.stdin.take();
    let status = guard.child.wait().expect("the guard ended");
    assert_eq!(status.code(), Some(0), "the guard ends on its own and cleanly");
    assert_eq!(holder_of_shared_range(&path), None);
}

// EAC-FR-PIHS: a guard that cannot open the catalog ends, and the start says
// which stage failed.
#[test]
fn a_guard_without_a_catalog_ends_at_the_open() {
    let (dir, _path) = catalog();
    let missing = dir.path().canonicalize().expect("canonical").join("absent.db");
    let error = CatalogGuard::start(test_command(&missing), LOCK_BOUND)
        .err()
        .expect("no guard starts without a catalog");
    assert!(matches!(error, GuardError::Ended(Some(EXIT_OPEN_FAILED))), "{error:?}");
    assert_eq!(error.stage(), "open");
}

// EAC-FR-PIHS: a guard that cannot take its lock within the bound is ended,
// and the start reports it. The writer here holds the range for the whole
// test, so the guard never gets it, however fast or slow the machine is.
#[test]
fn a_guard_that_cannot_take_its_lock_is_ended() {
    let (_dir, path) = catalog();
    let held = writer(&path);
    let lock = range_lock(libc::F_WRLCK as libc::c_short);
    // SAFETY: the descriptor is open for the call, and `lock` outlives it.
    let result = unsafe { libc::fcntl(held.as_raw_fd(), libc::F_SETLK, &lock) };
    assert_ne!(result, -1, "the write lock: {}", std::io::Error::last_os_error());

    let error = CatalogGuard::start(test_command(&path), Duration::from_millis(200))
        .err()
        .expect("no guard holds a range a writer holds");
    assert!(matches!(error, GuardError::NoLock), "{error:?}");
    assert_eq!(error.stage(), "lock_wait");
    drop(held);
}

// EAC-FR-PIHS: a command that cannot start is a spawn failure.
#[test]
fn a_guard_that_cannot_start_reports_the_spawn() {
    let error = CatalogGuard::start(Command::new("/nonexistent/synthesis"), LOCK_BOUND)
        .err()
        .expect("nothing starts");
    assert!(matches!(error, GuardError::Spawn(_)), "{error:?}");
    assert_eq!(error.stage(), "spawn");
}

// EAC-FR-NDDZ: only the guard argument starts a guard, and the catalog path
// follows it. The application's own arguments, a macOS `-psn_` argument
// among them, start the application.
#[test]
fn only_the_guard_argument_starts_a_guard() {
    let args = |list: &[&str]| list.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
    assert_eq!(guard_catalog_arg(args(&[]).into_iter()), None);
    assert_eq!(guard_catalog_arg(args(&["-psn_0_12345"]).into_iter()), None);
    assert_eq!(guard_catalog_arg(args(&["/h/db/msb.db", GUARD_ARG]).into_iter()), None);
    assert_eq!(guard_catalog_arg(args(&[GUARD_ARG]).into_iter()), Some(None));
    assert_eq!(
        guard_catalog_arg(args(&[GUARD_ARG, "/h/db/msb.db"]).into_iter()),
        Some(Some(PathBuf::from("/h/db/msb.db")))
    );
}

// EAC-FR-NDDZ: a shipped build starts its own executable with the guard
// argument and the catalog path.
#[test]
fn the_production_guard_is_this_executable_with_the_guard_argument() {
    let command = CatalogGuard::command(Path::new("/h/db/msb.db")).expect("a command");
    assert_eq!(command.get_program(), std::env::current_exe().expect("this executable"));
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, [std::ffi::OsStr::new(GUARD_ARG), std::ffi::OsStr::new("/h/db/msb.db")]);
}
