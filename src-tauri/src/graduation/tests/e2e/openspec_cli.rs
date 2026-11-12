//! The fake OpenSpec CLI of the OpenSpec scenarios (GTE-FR-QPMA,
//! GTE-FR-ZDCH).
//!
//! It stands behind the process seam of `OSM-openspec-mode.md`, the one
//! substitution of an OpenSpec scenario beside the dispatch seam. It answers
//! each subcommand from what the working copy it runs in holds, as the real
//! CLI does: `validate` reads the files of the change folder, `instructions
//! apply` reads the task marks of `tasks.md`, and `archive` moves the change
//! folder and writes its deltas into the live specs. It records every process,
//! and it answers at once: a timeout is an answer it gives, not a wait.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use super::settings::guard;
use crate::openspec::tests::{ok, Answer, Call};
use crate::openspec::{OpenSpecRunner, ProcessError, ProcessOutput};

/// The date the fake CLI archives a change under (OAR-FR-XTLB).
pub(crate) const ARCHIVE_DATE: &str = "2026-11-11";

/// What one process of the fake CLI wrote to standard error before it timed
/// out (OFP-FR-WNVO).
pub(crate) const STALLED: &str = "openspec: the schema read stalled";

/// The fake CLI. Each subcommand answers from the working copy, unless the
/// scenario queued an answer for its next processes.
#[derive(Default)]
pub(crate) struct WorkingCopyCli {
    calls: Mutex<Vec<Call>>,
    /// Answers given once each, in order, by subcommand, before the working
    /// copy is read.
    queued: Mutex<HashMap<String, VecDeque<Answer>>>,
    /// The argument vectors that no journey reads, so the fake answered them
    /// as a failed process. A scenario fails at its end where one stands.
    unknown: Mutex<Vec<Vec<String>>>,
}

impl WorkingCopyCli {
    /// The next `count` processes of `subcommand` time out, each with the
    /// standard error of [`STALLED`].
    pub(crate) fn time_out(&self, subcommand: &str, count: usize) {
        let stalled = || Err(ProcessError::TimedOut { stderr: STALLED.as_bytes().to_vec() });
        self.queue(subcommand, (0..count).map(|_| stalled()).collect());
    }

    /// The next `archive` process refuses the change with a `status` entry of
    /// `cause`, as the CLI refuses deltas that do not apply, and writes nothing.
    pub(crate) fn refuse_archive(&self, cause: &str) {
        let stdout = format!(
            r#"{{ "archive": null, "status": [ {{ "severity": "error", "code": "{cause}",
                "message": "MODIFIED \"Session Expiration\" not found in auth",
                "fix": "Use ADDED for a new requirement" }} ] }}"#
        );
        self.queue(
            "archive",
            vec![Ok(ProcessOutput {
                exit_status: Some(1),
                stdout: stdout.into_bytes(),
                stderr: Vec::new(),
            })],
        );
    }

    fn queue(&self, subcommand: &str, answers: Vec<Answer>) {
        self.queued.lock().unwrap().entry(subcommand.to_string()).or_default().extend(answers);
    }

    /// Every process the fake ran, in order.
    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// The processes of one subcommand, in order.
    pub(crate) fn calls_of(&self, subcommand: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.args.first().map(String::as_str) == Some(subcommand))
            .collect()
    }

    /// The argument vectors that fell into the arm of a read no journey makes.
    pub(crate) fn unknown_calls(&self) -> Vec<Vec<String>> {
        self.unknown.lock().unwrap().clone()
    }

    /// A read no journey makes: recorded, and answered as a CLI that exits
    /// with status 2.
    fn unknown(&self, args: &[&str]) -> Answer {
        self.unknown.lock().unwrap().push(args.iter().map(|arg| arg.to_string()).collect());
        Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new(), stderr: Vec::new() })
    }

    fn answer(&self, args: &[&str], cwd: &Path) -> Answer {
        match args {
            ["--version"] => Ok(ok("1.17.0\n")),
            // A project-wide validation (`--all`, `--specs`) is no change
            // name: no journey reads it, so it fails as an unknown read does.
            ["validate", flag, ..] if flag.starts_with("--") => self.unknown(args),
            ["validate", change, ..] => Ok(validate(cwd, change)),
            ["instructions", "apply", "--change", change, ..] => Ok(apply_progress(cwd, change)),
            ["archive", change, ..] => Ok(archive(cwd, change)),
            // No journey reads anything else, so any other read fails as a
            // CLI that exits with status 2 does.
            _ => self.unknown(args),
        }
    }
}

impl OpenSpecRunner for WorkingCopyCli {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        self.calls.lock().unwrap().push(Call {
            executable: executable.to_path_buf(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            timeout,
        });
        let subcommand = args.first().copied().unwrap_or_default();
        let queued = self.queued.lock().unwrap().get_mut(subcommand).and_then(VecDeque::pop_front);
        queued.unwrap_or_else(|| self.answer(args, cwd))
    }
}

/// The text of one file of the working copy, or `None` where none stands.
fn read(cwd: &Path, rel: &str) -> Option<String> {
    guard(cwd).read_text(cwd.join(rel)).ok()
}

/// `validate <change> --strict --json`: a change is valid where its folder
/// holds `proposal.md`, `tasks.md`, and the delta `specs/auth/spec.md`. Each
/// file that is missing is one issue, and an invalid change exits with 1.
fn validate(cwd: &Path, change: &str) -> ProcessOutput {
    let issues: Vec<serde_json::Value> = ["proposal.md", "tasks.md", "specs/auth/spec.md"]
        .iter()
        .filter(|file| read(cwd, &format!("openspec/changes/{change}/{file}")).is_none())
        .map(|file| {
            serde_json::json!({ "level": "ERROR", "path": file, "message": format!("{file} is missing") })
        })
        .collect();
    let valid = issues.is_empty();
    let body = serde_json::json!({
        "items": [ { "id": change, "type": "change", "valid": valid, "issues": issues } ]
    });
    ProcessOutput {
        exit_status: Some(if valid { 0 } else { 1 }),
        stdout: body.to_string().into_bytes(),
        stderr: Vec::new(),
    }
}

/// `instructions apply --change <change> --json`: the task marks of
/// `tasks.md`, one task for each line `- [ ]` or `- [x]`. A change with no
/// `tasks.md` is `blocked`.
fn apply_progress(cwd: &Path, change: &str) -> ProcessOutput {
    let Some(text) = read(cwd, &format!("openspec/changes/{change}/tasks.md")) else {
        let body = serde_json::json!({
            "changeName": change, "state": "blocked",
            "progress": { "total": 0, "complete": 0, "remaining": 0 },
            "tasks": [], "missingArtifacts": ["tasks"],
        });
        return ok(&body.to_string());
    };
    let tasks: Vec<(String, bool)> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("- [ ] ") {
                Some((rest.to_string(), false))
            } else {
                line.strip_prefix("- [x] ")
                    .or_else(|| line.strip_prefix("- [X] "))
                    .map(|rest| (rest.to_string(), true))
            }
        })
        .collect();
    let complete = tasks.iter().filter(|(_, done)| *done).count();
    let state = if complete == tasks.len() && !tasks.is_empty() { "all_done" } else { "ready" };
    let body = serde_json::json!({
        "changeName": change,
        "state": state,
        "progress": { "total": tasks.len(), "complete": complete, "remaining": tasks.len() - complete },
        "tasks": tasks
            .iter()
            .enumerate()
            .map(|(index, (description, done))| serde_json::json!({
                "id": (index + 1).to_string(), "description": description, "done": done,
            }))
            .collect::<Vec<_>>(),
    });
    ok(&body.to_string())
}

/// `archive <change> --json --yes`: the change folder moves to
/// `openspec/changes/archive/<date>-<change>/`, and the delta of `auth` is
/// added to the live spec of `auth`.
fn archive(cwd: &Path, change: &str) -> ProcessOutput {
    let fs = guard(cwd);
    let folder = cwd.join("openspec/changes").join(change);
    let archived_as = format!("{ARCHIVE_DATE}-{change}");
    let target = cwd.join("openspec/changes/archive").join(&archived_as);
    let delta = read(cwd, &format!("openspec/changes/{change}/specs/auth/spec.md"));
    // The move makes the parents of the archive folder.
    fs.move_path(&folder, &target)
        .unwrap_or_else(|_| panic!("the fake CLI could not move the folder of {change}"));
    let live = read(cwd, "openspec/specs/auth/spec.md").unwrap_or_default();
    fs.write_text_atomic(
        cwd.join("openspec/specs/auth/spec.md"),
        &format!("{live}{}", delta.unwrap_or_default()),
    )
    .expect("the live spec is writable");
    ok(&format!(r#"{{ "archive": {{ "change": "{change}", "archivedAs": "{archived_as}" }} }}"#))
}
