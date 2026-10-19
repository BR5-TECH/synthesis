//! EAC-FR-NDDZ: the catalog guard, macOS alone.
//!
//! The runtime library opens and closes plain descriptors on its catalog file.
//! On macOS that releases every POSIX lock this process holds on the file,
//! SQLite's own shared lock included, and SQLite does not take it again. A VM
//! process that then closes its last connection gets the exclusive lock,
//! checkpoints, and deletes the catalog's WAL and shared-memory files under the
//! connections of this process, which then read the catalog as damaged.
//!
//! The guard is a second process of this executable. It holds a read lock on
//! SQLite's shared lock range of the catalog file, so no other process gets the
//! exclusive lock, and the WAL and shared-memory files stay. Ordinary reads and
//! writes of a WAL catalog take no exclusive lock on the file, so the guard
//! stops none of them. It ends when its standard input closes, which is also
//! when this process ends.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::fs::{FsAccess, OpenMode};

/// The argument that starts this executable as a guard. The catalog path
/// follows it.
pub const GUARD_ARG: &str = "--sandbox-catalog-guard";

/// The line the guard writes once it holds the lock.
pub const LOCKED_LINE: &str = "synthesis-catalog-guard: locked";

/// SQLite's shared lock range in the default layout of the bundled SQLite:
/// `PENDING_BYTE + 2` (`0x40000000 + 2`), 510 bytes long.
pub const SHARED_FIRST: u64 = 0x4000_0002;
pub const SHARED_SIZE: u64 = 510;

/// How long a start waits for the guard to take its lock.
pub const LOCK_BOUND: Duration = Duration::from_secs(10);

/// The exit codes of a guard that could not do its work.
const EXIT_USAGE: i32 = 64;
const EXIT_OPEN_FAILED: i32 = 2;
const EXIT_LOCK_FAILED: i32 = 3;

/// The guard's entry point: `Some(exit code)` where this process was started
/// as a guard, and `None` where it is the application.
pub fn main_from_args() -> Option<i32> {
    Some(match guard_catalog_arg(std::env::args_os().skip(1))? {
        Some(catalog) => hold(&catalog),
        None => EXIT_USAGE,
    })
}

/// The catalog a guard holds, read from the arguments after the program name.
/// `None` where the arguments do not start a guard, and `Some(None)` where they
/// start one and name no catalog.
fn guard_catalog_arg(mut args: impl Iterator<Item = OsString>) -> Option<Option<PathBuf>> {
    if args.next()? != GUARD_ARG {
        return None;
    }
    Some(args.next().map(PathBuf::from))
}

/// Hold the read lock on `catalog` until standard input closes.
pub fn hold(catalog: &Path) -> i32 {
    let Ok(fs) = FsAccess::builder().read_only_file(catalog).build() else {
        return EXIT_OPEN_FAILED;
    };
    let Ok(handle) = fs.open_file(catalog, OpenMode::ReadOnly) else {
        return EXIT_OPEN_FAILED;
    };
    if handle.lock_shared_range(SHARED_FIRST, SHARED_SIZE).is_err() {
        return EXIT_LOCK_FAILED;
    }
    let mut out = std::io::stdout();
    if writeln!(out, "{LOCKED_LINE}").and_then(|()| out.flush()).is_err() {
        return EXIT_LOCK_FAILED;
    }
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
    drop(handle);
    0
}

/// Why a guard could not start.
#[derive(Debug)]
pub enum GuardError {
    /// The process could not be started.
    Spawn(String),
    /// The process ended before it held the lock, with this exit code.
    Ended(Option<i32>),
    /// The process did not hold the lock within the bound, and was ended.
    NoLock,
}

impl GuardError {
    /// The stage that failed, for the record of EAC-FR-PIHS.
    pub fn stage(&self) -> &'static str {
        match self {
            GuardError::Spawn(_) => "spawn",
            GuardError::Ended(Some(EXIT_OPEN_FAILED)) => "open",
            GuardError::Ended(Some(EXIT_LOCK_FAILED)) => "lock",
            GuardError::Ended(_) => "ended",
            GuardError::NoLock => "lock_wait",
        }
    }
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Spawn(message) => write!(f, "the guard could not start: {message}"),
            GuardError::Ended(Some(code)) => write!(f, "the guard ended with code {code}"),
            GuardError::Ended(None) => write!(f, "the guard ended on a signal"),
            GuardError::NoLock => write!(f, "the guard did not take its lock in time"),
        }
    }
}

/// A running guard. Dropping it ends the process.
pub struct CatalogGuard {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl CatalogGuard {
    /// The production command: this executable, started as a guard.
    pub fn command(catalog: &Path) -> Result<Command, GuardError> {
        let exe = std::env::current_exe().map_err(|e| GuardError::Spawn(e.to_string()))?;
        let mut command = Command::new(exe);
        command.arg(GUARD_ARG).arg(catalog);
        Ok(command)
    }

    /// Start `command` and wait at most `bound` for it to hold its lock. This
    /// call blocks.
    pub fn start(mut command: Command, bound: Duration) -> Result<Self, GuardError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = command.spawn().map_err(|e| GuardError::Spawn(e.to_string()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let mut guard = CatalogGuard { child, stdin };
        let Some(stdout) = stdout else {
            guard.end();
            return Err(GuardError::Spawn("the guard has no output".to_string()));
        };
        let (locked, waiting) = mpsc::channel();
        // The reader drains the output until the guard ends, so the guard
        // never blocks on a full pipe.
        std::thread::spawn(move || {
            let mut locked = Some(locked);
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.trim() == LOCKED_LINE {
                    if let Some(locked) = locked.take() {
                        let _ = locked.send(());
                    }
                }
            }
        });
        match waiting.recv_timeout(bound) {
            Ok(()) => Ok(guard),
            // The output closes when the guard ends, so its own exit code is
            // read before anything could kill it.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(GuardError::Ended(guard.reap())),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                guard.end();
                Err(GuardError::NoLock)
            }
        }
    }

    /// The process ID of the guard.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// `Some(exit code)` where the guard has ended, and `None` while it runs.
    pub fn exited(&mut self) -> Option<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            Ok(None) => None,
            Err(_) => Some(None),
        }
    }

    /// Wait for a guard that is ending on its own, and return its exit code.
    pub(crate) fn reap(&mut self) -> Option<i32> {
        self.stdin.take();
        self.child.wait().ok().and_then(|status| status.code())
    }

    /// End the guard and return its exit code.
    pub fn end(&mut self) -> Option<i32> {
        self.stdin.take();
        let _ = self.child.kill();
        self.child.wait().ok().and_then(|status| status.code())
    }
}

impl Drop for CatalogGuard {
    fn drop(&mut self) {
        self.end();
    }
}

#[cfg(test)]
pub(crate) mod tests;
