//! The tests of OpenSpec mode (`OSM-openspec-mode.md`).
//!
//! Every test runs against a fake runner and a fake probe, so no test needs
//! Node or the OpenSpec CLI. The real runner has its own tests in `runner.rs`,
//! with a shell script in place of the CLI.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::*;
use crate::logging::{BufferState, LogBuffer, LogFilter, LogLevel, LogRecord};

mod apply;
mod archive;
mod change;
mod cli;
mod mode;
mod project_validation;
mod reads;
#[cfg(unix)]
mod runner;

#[derive(Clone, Default)]
pub(crate) struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

pub(crate) fn buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

pub(crate) fn records(buffer: &'static LogBuffer) -> Vec<LogRecord> {
    buffer
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 500)
        .unwrap()
        .records
}

/// One call the fake runner received.
#[derive(Clone, Debug)]
pub(crate) struct Call {
    pub(crate) executable: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) timeout: Duration,
}

/// What the fake runner gives back for one call.
pub(crate) type Answer = Result<ProcessOutput, ProcessError>;

/// A runner that answers by the first argument and records every call.
#[derive(Default)]
pub(crate) struct FakeRunner {
    answers: Mutex<HashMap<String, Result<ProcessOutput, ProcessError>>>,
    /// Answers chosen by the leading arguments, tried before `answers`.
    by_prefix: Mutex<Vec<(Vec<String>, Answer)>>,
    pub(crate) calls: Mutex<Vec<Call>>,
}

impl FakeRunner {
    pub(crate) fn answering(
        self,
        first_arg: &str,
        answer: Result<ProcessOutput, ProcessError>,
    ) -> Self {
        self.answers.lock().unwrap().insert(first_arg.to_string(), answer);
        self
    }

    /// Change the answer to `first_arg` from now on.
    pub(crate) fn set(&self, first_arg: &str, answer: Result<ProcessOutput, ProcessError>) {
        self.answers.lock().unwrap().insert(first_arg.to_string(), answer);
    }

    pub(crate) fn json(self, first_arg: &str, json: &str) -> Self {
        self.answering(first_arg, Ok(ok(json)))
    }

    /// An answer for every call whose arguments start with `prefix`.
    pub(crate) fn answering_prefix(
        self,
        prefix: &[&str],
        answer: Result<ProcessOutput, ProcessError>,
    ) -> Self {
        self.by_prefix
            .lock()
            .unwrap()
            .push((prefix.iter().map(|s| s.to_string()).collect(), answer));
        self
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    pub(crate) fn calls_of(&self, first_arg: &str) -> usize {
        self.calls()
            .iter()
            .filter(|c| c.args.first().map(String::as_str) == Some(first_arg))
            .count()
    }
}

impl OpenSpecRunner for FakeRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        self.calls.lock().unwrap().push(Call {
            executable: executable.to_path_buf(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            timeout,
        });
        if let Some((_, answer)) = self.by_prefix.lock().unwrap().iter().find(|(prefix, _)| {
            prefix.iter().zip(args.iter()).all(|(p, a)| p == a) && prefix.len() <= args.len()
        }) {
            return answer.clone();
        }
        let key = args.first().copied().unwrap_or_default();
        self.answers
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .unwrap_or(Ok(ProcessOutput { exit_status: Some(1), stdout: Vec::new() }))
    }
}

pub(crate) fn ok(stdout: &str) -> ProcessOutput {
    ProcessOutput { exit_status: Some(0), stdout: stdout.as_bytes().to_vec() }
}

/// A probe over a fixed set of executables, modification times, and folders.
#[derive(Default)]
pub(crate) struct FakeProbe {
    pub(crate) executables: Mutex<HashSet<PathBuf>>,
    pub(crate) modified: Mutex<HashMap<PathBuf, SystemTime>>,
    pub(crate) folders: Mutex<HashMap<PathBuf, Vec<String>>>,
}

impl FakeProbe {
    pub(crate) fn with_executable(self, path: &str) -> Self {
        self.executables.lock().unwrap().insert(PathBuf::from(path));
        self
    }

    pub(crate) fn touch(&self, path: &str, at: SystemTime) {
        self.modified.lock().unwrap().insert(PathBuf::from(path), at);
    }
}

impl CliProbe for FakeProbe {
    fn is_executable(&self, path: &Path) -> bool {
        self.executables.lock().unwrap().contains(path)
    }

    fn modified(&self, path: &Path) -> Option<SystemTime> {
        self.modified.lock().unwrap().get(path).copied()
    }

    fn subfolders(&self, dir: &Path) -> Vec<String> {
        self.folders.lock().unwrap().get(dir).cloned().unwrap_or_default()
    }
}

/// The one executable most tests find, on the `PATH` of the fake environment.
pub(crate) const CLI: &str = "/fake/bin/openspec";

pub(crate) fn env() -> CliEnvironment {
    CliEnvironment {
        path_var: Some("/fake/bin".to_string()),
        home: Some(PathBuf::from("/fake/home")),
    }
}

/// A seam whose CLI is found and reports version 1.17.0, plus the answers
/// the test adds.
pub(crate) fn seam_with(runner: FakeRunner) -> (OpenSpec, Arc<FakeRunner>, Arc<FakeProbe>) {
    let runner = Arc::new(runner.json("--version", "1.17.0\n"));
    let probe = Arc::new(FakeProbe::default().with_executable(CLI));
    let seam = OpenSpec::new(runner.clone(), probe.clone(), env());
    (seam, runner, probe)
}

/// A project that uses OpenSpec through its config file.
pub(crate) fn openspec_project() -> (TempDir, RootFs) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("openspec")).unwrap();
    std::fs::write(dir.path().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    let root = RootFs::for_root(dir.path());
    (dir, root)
}

/// A project with no OpenSpec folder.
pub(crate) fn plain_project() -> (TempDir, RootFs) {
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    (dir, root)
}

pub(crate) fn write(dir: &TempDir, rel: &str, body: &str) {
    let path = dir.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
