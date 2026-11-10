//! The OpenSpec CLI: where it is, how it runs, and how many run at once
//! (`OSM-openspec-mode.md` OSM-FR-EZKW, OSM-FR-ZNVZ, OSM-FR-HHUD,
//! OSM-FR-ITZI, OSM-FR-ZIMO, OSM-FR-QHRV).
//!
//! The machine and the child process are behind two traits, so every rule of
//! the module can be tested without Node and without the CLI.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime};

/// OSM-FR-ZIMO: the longest that one CLI process may run.
pub const CLI_TIMEOUT: Duration = Duration::from_secs(30);

/// OSM-FR-ZNVZ: how long a `failed` version check is kept.
pub const FAILED_CHECK_TTL: Duration = Duration::from_secs(30);

/// OSM-FR-QHRV: how many CLI processes may run at the same time.
pub const MAX_CONCURRENT: usize = 4;

/// OSM-FR-ZNVZ: the first major version that gives `status --json`.
pub const MIN_MAJOR: u64 = 1;

/// OSM-FR-HHUD: the variables a CLI process gets beside `PATH`. Telemetry off,
/// and no color codes in the JSON it prints.
pub const CHILD_VARIABLES: [(&str, &str); 3] =
    [("OPENSPEC_TELEMETRY", "0"), ("DO_NOT_TRACK", "1"), ("NO_COLOR", "1")];

/// OSM-FR-EZKW: the executable names, per platform.
pub fn executable_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["openspec.cmd"]
    } else {
        &["openspec"]
    }
}

// ---------------------------------------------------------------------------
// The machine, behind a trait
// ---------------------------------------------------------------------------

/// The three questions the module asks of the machine about an executable
/// outside the project.
pub trait CliProbe: Send + Sync {
    /// Is there an executable regular file at this path?
    fn is_executable(&self, path: &Path) -> bool;
    /// When was it last changed? `None` where the platform does not say.
    fn modified(&self, path: &Path) -> Option<SystemTime>;
    /// The names of the folders directly in `dir`. Empty when `dir` is not a
    /// folder.
    fn subfolders(&self, dir: &Path) -> Vec<String>;
}

/// The production probe.
///
/// FSA-FR-19 exception, deliberate, on the terms of
/// `crate::agentic::RealFileProbe`: the OpenSpec CLI is an executable that the
/// user installed somewhere on the machine, outside every project root, and
/// the shared `FsAccess` instance would refuse every real path. Nothing is
/// read: these calls ask the OS for metadata and folder names only.
pub struct RealCliProbe;

impl CliProbe for RealCliProbe {
    #[cfg(unix)]
    fn is_executable(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_executable(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn modified(&self, path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    }

    fn subfolders(&self, dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }
}

/// The parts of the application environment that the search reads.
#[derive(Clone, Debug, Default)]
pub struct CliEnvironment {
    pub path_var: Option<String>,
    pub home: Option<PathBuf>,
}

impl CliEnvironment {
    /// The environment of this process, read once at construction.
    pub fn from_process() -> Self {
        CliEnvironment {
            path_var: std::env::var("PATH").ok(),
            home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution (OSM-FR-EZKW)
// ---------------------------------------------------------------------------

/// OSM-FR-EZKW: the directories of AIC-FR-03, then the per-user Node
/// directories, in the order of the contract surface.
pub fn search_directories(env: &CliEnvironment, probe: &dyn CliProbe) -> Vec<PathBuf> {
    let mut dirs =
        crate::agentic::detection_directories(env.path_var.as_deref(), env.home.as_deref());
    if let Some(home) = env.home.as_deref() {
        dirs.push(home.join(".local/share/pnpm"));
        if cfg!(target_os = "macos") {
            dirs.push(home.join("Library/pnpm"));
        }
        let nvm = home.join(".nvm/versions/node");
        let mut versions = probe.subfolders(&nvm);
        // The highest version first, compared by number and not by text, so
        // `v22.12.0` comes before `v9.0.0`.
        versions.sort_by_key(|name| std::cmp::Reverse(version_key(name)));
        dirs.extend(versions.into_iter().map(|v| nvm.join(v).join("bin")));
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// The numbers of a folder name such as `v22.12.0`, for ordering alone.
fn version_key(name: &str) -> Vec<u64> {
    name.trim_start_matches('v').split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

/// OSM-FR-EZKW: the first executable OpenSpec CLI in the search order.
pub fn resolve(env: &CliEnvironment, probe: &dyn CliProbe) -> Option<PathBuf> {
    search_directories(env, probe).into_iter().find_map(|dir| {
        executable_names().iter().map(|name| dir.join(name)).find(|p| probe.is_executable(p))
    })
}

/// OSM-FR-ZNVZ: the first `<major>.<minor>.<patch>` token of the output of
/// `openspec --version`, with its major number.
pub fn parse_version(text: &str) -> Option<(String, u64)> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let mut parts = token.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next()?;
        let patch = parts.next()?;
        let digits =
            |s: &str| !s.is_empty() && s.chars().take_while(char::is_ascii_digit).count() > 0;
        (digits(minor) && digits(patch)).then(|| (token.to_string(), major))
    })
}

// ---------------------------------------------------------------------------
// The child process, behind a trait (OSM-FR-HHUD, OSM-FR-ZIMO)
// ---------------------------------------------------------------------------

/// What a finished CLI process gave back. Standard error is not kept: no part
/// of it is read, and no part of it may reach a log record (OSM-FR-IFWM).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessOutput {
    /// `None` when the process ended on a signal.
    pub exit_status: Option<i32>,
    pub stdout: Vec<u8>,
}

/// Why a CLI process gave no output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// The executable could not be started.
    NotExecutable,
    /// OSM-FR-ZIMO: it ran too long and was stopped.
    TimedOut,
}

/// One bounded CLI process, run to its end.
pub trait OpenSpecRunner: Send + Sync {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, ProcessError>;
}

/// OSM-FR-HHUD: the `PATH` of a CLI process, with the directory of the
/// executable first, so that `#!/usr/bin/env node` finds the Node installed
/// beside it.
pub fn child_path(executable: &Path, inherited: Option<&OsStr>) -> OsString {
    let mut dirs: Vec<PathBuf> = executable.parent().map(Path::to_path_buf).into_iter().collect();
    if let Some(inherited) = inherited {
        dirs.extend(std::env::split_paths(inherited));
    }
    std::env::join_paths(dirs)
        .unwrap_or_else(|_| inherited.map(OsStr::to_os_string).unwrap_or_default())
}

/// OSM-FR-HHUD, OSM-FR-ITZI: every variable a CLI process gets that the
/// application environment does not already give it. Nothing else is added,
/// and no credential is among these.
pub fn child_variables(
    executable: &Path,
    inherited_path: Option<&OsStr>,
) -> Vec<(OsString, OsString)> {
    let mut vars: Vec<(OsString, OsString)> =
        CHILD_VARIABLES.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect();
    vars.push((OsString::from("PATH"), child_path(executable, inherited_path)));
    vars
}

/// The production runner.
///
/// The timeout is enforced by polling, as `crate::agentic::RealCliRunner`
/// does: a blocking wait would leave nothing to stop when the time is over.
/// Standard output is read on a thread, so a CLI that fills the pipe cannot
/// block against the wait. On Unix the child leads its own process group, and
/// a timeout stops the whole group: a process the CLI started would otherwise
/// keep the output pipe open after the CLI itself is gone.
pub struct RealRunner;

impl OpenSpecRunner for RealRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        use crate::embeddings::InheritedEnvironment;
        use std::io::Read;
        use std::process::{Command, Stdio};

        let mut command = Command::new(executable);
        command
            .with_inherited_hub_environment()
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        for (key, value) in child_variables(executable, std::env::var_os("PATH").as_deref()) {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(|_| ProcessError::NotExecutable)?;
        let mut stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(out) = stdout.as_mut() {
                let _ = out.read_to_end(&mut bytes);
            }
            bytes
        });
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= timeout => {
                    kill_group(&child);
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(ProcessError::TimedOut);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => {
                    let _ = child.kill();
                    let _ = reader.join();
                    return Err(ProcessError::NotExecutable);
                }
            }
        };
        // A process the CLI started can outlive it and keep the pipe open, so
        // the rest of the group stops before the reader is joined.
        kill_group(&child);
        let stdout = reader.join().unwrap_or_default();
        Ok(ProcessOutput { exit_status: status.code(), stdout })
    }
}

/// Stop every process of the child's group. The child was made the leader of
/// its group at spawn, so one signal reaches the group.
fn kill_group(child: &std::process::Child) {
    #[cfg(unix)]
    {
        // SAFETY: `killpg` touches no memory of this process. A failure means
        // the group is already gone, which is the result asked for.
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = child;
}

// ---------------------------------------------------------------------------
// Concurrency (OSM-FR-QHRV)
// ---------------------------------------------------------------------------

/// OSM-FR-QHRV: a counting gate over the CLI processes of the application.
pub struct Slots {
    max: usize,
    /// The slots held, and the holders that wait for one.
    state: Mutex<SlotCount>,
    freed: Condvar,
}

#[derive(Default)]
struct SlotCount {
    used: usize,
    waiting: usize,
}

impl Slots {
    pub fn new(max: usize) -> Self {
        Slots { max: max.max(1), state: Mutex::new(SlotCount::default()), freed: Condvar::new() }
    }

    /// Wait for a free slot and hold it until the guard drops.
    pub fn acquire(&self) -> SlotGuard<'_> {
        let mut count = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if count.used >= self.max {
            count.waiting += 1;
            while count.used >= self.max {
                count = self.freed.wait(count).unwrap_or_else(|e| e.into_inner());
            }
            count.waiting -= 1;
        }
        count.used += 1;
        SlotGuard { slots: self }
    }

    /// How many slots are held now.
    #[cfg(test)]
    pub fn in_use(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).used
    }

    /// How many holders wait for a slot now.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).waiting
    }
}

/// One held slot of [`Slots`].
pub struct SlotGuard<'a> {
    slots: &'a Slots,
}

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        let mut count = self.slots.state.lock().unwrap_or_else(|e| e.into_inner());
        count.used = count.used.saturating_sub(1);
        self.slots.freed.notify_one();
    }
}
