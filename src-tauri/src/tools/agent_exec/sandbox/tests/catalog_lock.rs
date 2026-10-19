//! EAC-FR-NDDZ, EAC-FR-SQWO: the catalog's locks and shared memory on macOS,
//! against the real runtime library and a catalog of a home of its own. No VM
//! boots.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use microsandbox::backend::LocalBackend;

use super::super::catalog_guard::{
    tests::test_command, CatalogGuard, LOCK_BOUND, SHARED_FIRST, SHARED_SIZE,
};
use super::super::micro::{close_catalog, names_catalog_failure};

/// The variable that makes `holder_probe_entry` report who holds the shared
/// range of a catalog.
const PROBE_ENV: &str = "SYNTHESIS_CATALOG_HOLDER_PROBE";

/// A lock is visible only from another process, so the test binary is started
/// again to look. It prints `holder <pid>` or `holder none`.
#[test]
fn holder_probe_entry() {
    let Some(catalog) = std::env::var_os(PROBE_ENV) else { return };
    let file = std::fs::OpenOptions::new().read(true).write(true).open(catalog).expect("catalog");
    // SAFETY: `flock` is a plain C struct, and all zero bits is a valid value.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock.l_start = SHARED_FIRST as libc::off_t;
    lock.l_len = SHARED_SIZE as libc::off_t;
    // SAFETY: the descriptor is open for the call, and `lock` outlives it.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) };
    assert_ne!(result, -1, "the lock query failed");
    if lock.l_type == libc::F_UNLCK as libc::c_short {
        println!("holder none");
    } else {
        println!("holder {}", lock.l_pid);
    }
    std::process::exit(0);
}

/// The process that holds SQLite's shared range of `catalog`, as another
/// process sees it.
fn holder(catalog: &Path) -> Option<u32> {
    let module = module_path!();
    let module = module.split_once("::").map_or(module, |(_, rest)| rest);
    let entry = format!("{module}::holder_probe_entry");
    let output = Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", entry.as_str(), "--nocapture", "--test-threads=1", "-q"])
        .env(PROBE_ENV, catalog)
        .output()
        .expect("the probe ran");
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find_map(|line| line.strip_prefix("holder ")).unwrap_or_else(|| {
        panic!(
            "the probe printed no holder line; status {:?}, stdout {text}, stderr {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
    });
    line.parse().ok()
}

/// A runtime home of its own, and the catalog file in it.
fn home() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = dir.path().canonicalize().expect("a canonical home");
    let catalog = home.join("db").join("msb.db");
    (dir, home, catalog)
}

/// A backend on `home` that reads no configuration of the machine it runs on.
fn builder(home: &Path) -> microsandbox::backend::LocalBackendBuilder {
    LocalBackend::builder().home(home).config_path(home.join("config.json"))
}

async fn backend(home: &Path) -> Result<LocalBackend, String> {
    builder(home).build().await.map_err(|e| e.to_string())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime")
}

// EAC-FR-NDDZ: once the runtime library opens its catalog, no process holds
// SQLite's shared range, this one included. The library closed a descriptor
// on the catalog and so released this process's lock. A guard then holds the
// range, so no other process can take the exclusive lock that deletes the WAL
// and shared-memory files. Where a later runtime library keeps the lock, the
// first assertion fails, and the guard is worth a second look.
#[test]
fn the_guard_holds_the_catalog_lock_that_the_runtime_library_releases() {
    let (_dir, home, catalog) = home();
    let rt = runtime();
    let opened = rt.block_on(backend(&home)).expect("the catalog opens");
    assert_eq!(holder(&catalog), None, "the open catalog holds no shared lock");

    let guard = CatalogGuard::start(test_command(&catalog), LOCK_BOUND).expect("the guard");
    assert_eq!(holder(&catalog), Some(guard.pid()), "the guard holds the shared range");
    drop(guard);
    assert_eq!(holder(&catalog), None);
    drop(opened);
}

// EAC-FR-SQWO: where the WAL and shared-memory files are replaced while this
// process holds catalog connections, a new connection beside them fails. Once
// the earlier connections close, a new one opens.
#[test]
fn a_new_catalog_connection_opens_once_the_earlier_ones_close() {
    let (_dir, home, catalog) = home();
    let rt = runtime();
    rt.block_on(async {
        let earlier = backend(&home).await.expect("the catalog opens");
        for suffix in ["db-wal", "db-shm"] {
            std::fs::remove_file(catalog.with_extension(suffix)).expect("the file is replaced");
        }
        let beside = backend(&home).await;
        assert!(
            beside.as_ref().is_err_and(|e| names_catalog_failure(e)),
            "a new connection beside the earlier ones reads the damage: {:?}",
            beside.as_ref().err()
        );

        assert!(close_catalog(&earlier, Duration::from_secs(10)).await, "the earlier ones close");
        backend(&home).await.expect("a new connection opens once the earlier ones closed");
    });
}

// EAC-FR-SQWO: a backend whose catalog cannot open has nothing to close.
#[test]
fn a_catalog_that_never_opened_has_nothing_to_close() {
    let (dir, _home, _catalog) = home();
    let file = dir.path().canonicalize().expect("canonical").join("not-a-home");
    std::fs::write(&file, b"a file where the home's directory must stand").expect("the file");
    let rt = runtime();
    let config = dir.path().canonicalize().expect("canonical").join("config.json");
    let backend = LocalBackend::builder()
        .home(&file)
        .config_path(config)
        .build_lazy()
        .expect("a lazy backend");
    assert!(rt.block_on(close_catalog(&backend, Duration::from_secs(10))));
}
