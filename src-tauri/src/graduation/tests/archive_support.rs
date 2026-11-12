//! What the archive scenarios share (`OAR-openspec-archive.md`): a project in
//! OpenSpec mode whose fake CLI archives a change as `openspec archive` does,
//! streams that hold changes, and completed apply runs of them.
//!
//! OAR-FR-NIIP: no OpenSpec CLI, no container runtime, and no network. The
//! fake process seam of OSM moves the files a real archive would move.

use std::sync::Arc;

use super::*;
use crate::openspec::tests::{env, ok, FakeProbe, FakeRunner, CLI};
use crate::openspec::{OpenSpec, OpenSpecRunner, ProcessError, ProcessOutput};
use crate::streams::{StreamMergePublication, StreamMergeResult, WorkStream};

/// The date the fake CLI archives under.
pub(super) const DATE: &str = "2026-11-11";

/// `instructions apply` of a change with every task done.
pub(super) const ALL_DONE: &str =
    r#"{ "progress": { "total": 1, "complete": 1 }, "tasks": [], "state": "all_done" }"#;
/// The same with a task left.
pub(super) const READY: &str =
    r#"{ "progress": { "total": 2, "complete": 1 }, "tasks": [], "state": "ready" }"#;

/// What the fake `archive` of one change does.
#[derive(Clone)]
pub(super) enum Archive {
    /// Move the folder, update the live spec, and answer success.
    Succeed,
    /// Write `stray` files and change the live spec, then answer `stdout`
    /// with `status`.
    FailWriting { status: i32, stdout: String },
    /// Answer success and write nothing.
    SucceedWritingNothing,
    /// Move one file of the change folder to the archive folder, change the
    /// live spec, then answer `stdout` with `status`: a real half archive.
    MoveHalf { status: i32, stdout: String },
    /// Archive as `Succeed` does, and also change the tracked `src.txt` and
    /// write an untracked file below the archive folder of another change.
    SucceedWritingOutside,
    /// Answer `answer` and write nothing.
    Answer(Result<ProcessOutput, ProcessError>),
}

/// The process seam: answers by first argument as `FakeRunner` does, and
/// performs the writes of `archive` in the working copy it runs in.
pub(super) struct ArchivingRunner {
    pub(super) inner: FakeRunner,
    pub(super) archives: Mutex<std::collections::HashMap<String, Archive>>,
    /// What runs while an `archive` process runs.
    pub(super) during_archive: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// What runs after an `instructions` process answers.
    pub(super) after_instructions: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl ArchivingRunner {
    pub(super) fn set_archive(&self, change: &str, archive: Archive) {
        self.archives.lock().unwrap().insert(change.to_string(), archive);
    }

    /// The arguments of every `archive` call, in order.
    pub(super) fn archive_calls(&self) -> Vec<Vec<String>> {
        self.inner.calls().into_iter().filter(|c| c.args[0] == "archive").map(|c| c.args).collect()
    }
}

pub(super) fn success_json(change: &str) -> String {
    format!(r#"{{ "archive": {{ "change": "{change}", "archivedAs": "{DATE}-{change}" }} }}"#)
}

impl OpenSpecRunner for ArchivingRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: &Path,
        timeout: std::time::Duration,
    ) -> Result<ProcessOutput, ProcessError> {
        let answer = self.inner.run(executable, args, cwd, timeout);
        if args.first() == Some(&"instructions") {
            if let Some(hook) = self.after_instructions.lock().unwrap().as_ref() {
                hook();
            }
        }
        if args.first() != Some(&"archive") {
            return answer;
        }
        if let Some(hook) = self.during_archive.lock().unwrap().as_ref() {
            hook();
        }
        let change = args[1].to_string();
        let plan = self.archives.lock().unwrap().get(&change).cloned().unwrap_or(Archive::Succeed);
        let spec = cwd.join("openspec/specs/auth/spec.md");
        let target = cwd.join(format!("openspec/changes/archive/{DATE}-{change}"));
        let archive_all = || {
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::rename(cwd.join("openspec/changes").join(&change), &target).unwrap();
            let text = std::fs::read_to_string(&spec).unwrap_or_default();
            std::fs::write(&spec, format!("{text}### Requirement: From {change}\n")).unwrap();
        };
        match plan {
            Archive::Succeed => {
                archive_all();
                Ok(ok(&success_json(&change)))
            }
            Archive::SucceedWritingOutside => {
                archive_all();
                std::fs::write(cwd.join("src.txt"), "changed by the CLI\n").unwrap();
                let other = cwd.join("openspec/changes/archive/2020-01-01-other/x.md");
                std::fs::create_dir_all(other.parent().unwrap()).unwrap();
                std::fs::write(other, "x\n").unwrap();
                Ok(ok(&success_json(&change)))
            }
            Archive::MoveHalf { status, stdout } => {
                std::fs::create_dir_all(&target).unwrap();
                let folder = cwd.join("openspec/changes").join(&change);
                std::fs::rename(folder.join("proposal.md"), target.join("proposal.md")).unwrap();
                std::fs::write(&spec, "half merged\n").unwrap();
                Ok(ProcessOutput { exit_status: Some(status), stdout: stdout.into_bytes() })
            }
            Archive::Answer(answer) => answer,
            Archive::FailWriting { status, stdout } => {
                let stray = cwd.join(format!("openspec/changes/archive/{DATE}-{change}/stray.md"));
                std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
                std::fs::write(stray, "half\n").unwrap();
                std::fs::write(&spec, "broken by a half archive\n").unwrap();
                std::fs::write(cwd.join("openspec/specs/new-cap.md"), "new\n").unwrap();
                Ok(ProcessOutput { exit_status: Some(status), stdout: stdout.into_bytes() })
            }
            Archive::SucceedWritingNothing => Ok(ok(&success_json(&change))),
        }
    }
}

/// A project in OpenSpec mode with its config and one live spec committed,
/// and the repository machine store in a folder of its own, so the store is
/// no part of the base worktree.
pub(super) fn archive_fixture(progress: &str) -> (Fixture, Arc<ArchivingRunner>, Arc<FakeProbe>) {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec/specs/auth")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    std::fs::write(fx.root().join("openspec/specs/auth/spec.md"), "# Auth\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "Add OpenSpec");
    let machine = canonical(fx.store.path()).join("machine");
    std::fs::create_dir_all(&machine).unwrap();
    fx.app.state::<crate::project::ProjectState>().set_store(machine);
    let runner = Arc::new(ArchivingRunner {
        inner: FakeRunner::default().json("--version", "1.17.0\n").json("instructions", progress),
        archives: Mutex::new(std::collections::HashMap::new()),
        during_archive: Mutex::new(None),
        after_instructions: Mutex::new(None),
    });
    let probe = Arc::new(FakeProbe::default().with_executable(CLI));
    let seam = OpenSpec::new(runner.clone(), probe.clone(), env());
    seam.cli_state(&crate::openspec::tests::Silent, crate::openspec::tests::buffer());
    fx.app.manage(seam);
    (fx, runner, probe)
}

/// The repository machine store of the fixture.
pub(super) fn records_store(fx: &Fixture) -> crate::fs::RootFs {
    fx.app.state::<crate::project::ProjectState>().require_store().unwrap()
}

/// A stream whose branch holds one change folder for each name, with a task
/// file and a delta of `auth`.
pub(super) fn stream_with_changes(fx: &Fixture, name: &str, changes: &[&str]) -> WorkStream {
    let stream = fx.stream(name);
    for change in changes {
        let folder = stream.worktree().join("openspec/changes").join(change);
        std::fs::create_dir_all(folder.join("specs/auth")).unwrap();
        std::fs::write(folder.join("proposal.md"), format!("# Why {change}\n")).unwrap();
        std::fs::write(folder.join("tasks.md"), "- [x] Do it\n").unwrap();
        std::fs::write(folder.join("specs/auth/spec.md"), "## ADDED Requirements\n").unwrap();
    }
    std::fs::write(stream.worktree().join("src.txt"), format!("built by {name}\n")).unwrap();
    super::change_runs::commit_all(&stream.worktree(), "Build the changes");
    stream
}

/// A completed apply run of `change` on the stream, that completed at `at`.
pub(super) fn completed_apply(
    fx: &Fixture,
    stream: &WorkStream,
    change: &str,
    at: &str,
) -> GraduationRun {
    ended_run(fx, stream, crate::graduation::PHASE_APPLY, change, at, GraduationRunState::Completed)
}

/// A run of `phase` on `change` on the stream that ended in `state`, with a
/// transition to `done` at `at`.
pub(super) fn ended_run(
    fx: &Fixture,
    stream: &WorkStream,
    phase: &str,
    change: &str,
    at: &str,
    state: GraduationRunState,
) -> GraduationRun {
    let mut run = fx.enqueue(stream, &crate::graduation::apply_prompt(change));
    run.openspec = Some(RunOpenSpec::of_phase(phase, change));
    run.state = state;
    run.commits = vec![super::change_runs::head_of(&stream.worktree())];
    run.observability.stage_history.push(StageTransition {
        from: GraduationVisualStage::Review,
        to: GraduationVisualStage::Done,
        pass: 1,
        at: at.to_string(),
        reason: StageReason::Finished,
    });
    crate::graduation::save_run(&fx.app, &mut run).expect("a saved run");
    run
}

/// A merge of the stream, with the queue not offered a start.
pub(super) fn merge(
    fx: &Fixture,
    stream: &WorkStream,
    publication: StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let result = crate::streams::merge_work_stream_blocking(&fx.app, &stream.id, publication);
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    result
}

pub(super) fn commit_publication() -> StreamMergePublication {
    StreamMergePublication::Commit { message: "Land the stream".into() }
}

/// The tip of a branch of the project's repository.
pub(super) fn tip(fx: &Fixture, branch: &str) -> git2::Oid {
    let repo = fx.repo();
    let tip = repo
        .find_branch(branch, git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap()
        .id();
    tip
}

/// The paths one commit changed against its first parent, in text order.
pub(super) fn commit_paths(fx: &Fixture, commit: git2::Oid) -> Vec<String> {
    let repo = fx.repo();
    let commit = repo.find_commit(commit).unwrap();
    let tree = commit.tree().unwrap();
    let parent = commit.parent(0).unwrap().tree().unwrap();
    let diff = repo.diff_tree_to_tree(Some(&parent), Some(&tree), None).unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| {
            d.new_file().path().or_else(|| d.old_file().path()).map(|p| p.to_string_lossy().into())
        })
        .collect();
    paths.sort();
    paths
}

/// The messages of the commits a branch holds beyond `since`, oldest first.
pub(super) fn messages_since(fx: &Fixture, branch: &str, since: git2::Oid) -> Vec<String> {
    let repo = fx.repo();
    let mut walk = repo.revwalk().unwrap();
    walk.push(tip(fx, branch)).unwrap();
    walk.hide(since).unwrap();
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE).unwrap();
    walk.flatten()
        .map(|oid| {
            repo.find_commit(oid).unwrap().summary().ok().flatten().unwrap_or_default().to_string()
        })
        .collect()
}

/// The uncommitted paths of a working copy, untracked files included.
pub(super) fn dirty(worktree: &Path) -> Vec<String> {
    let repo = git2::Repository::open(worktree).unwrap();
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    statuses.iter().filter_map(|e| e.path().ok().map(str::to_string)).collect()
}

/// Write one record file below the store folder of a change.
pub(super) fn write_record(fx: &Fixture, stream: &WorkStream, change: &str, rel: &str, body: &str) {
    let path = records_store(fx)
        .path()
        .join(crate::repository_store::CHANGES_DIRNAME)
        .join(&stream.id)
        .join(change)
        .join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

const AGENT: &str = "[agent]\nkind = \"agent\"\nagentId = \"a1\"\nhandle = \"ada\"\n";

/// A change edit proposal of `change` on the stream in `state`, owing its
/// commit where `owed`.
pub(super) fn edit_proposal(stream: &WorkStream, change: &str, state: &str, owed: bool) -> String {
    format!(
        "id = \"p1\"\nstreamId = \"{}\"\nchange = \"{change}\"\nrationale = \"r\"\n\
         threadId = \"t1\"\ncommentId = \"m1\"\nstate = \"{state}\"\ncommitOwed = {owed}\n\
         createdAt = \"2026-11-10T10:00:00Z\"\n{AGENT}",
        stream.id
    )
}

/// A new-change proposal of `change` on the stream in `state`, owing its
/// commit where `owed`.
pub(super) fn new_change_proposal(
    stream: &WorkStream,
    change: &str,
    state: &str,
    owed: bool,
) -> String {
    format!(
        "id = \"n1\"\nsetId = \"s1\"\nstreamId = \"{}\"\nchange = \"{change}\"\n\
         name = \"split-off\"\npaths = [\"tasks.md\"]\nposition = 1\ntotal = 1\nrationale = \"r\"\n\
         threadId = \"t1\"\ncommentId = \"m1\"\nstate = \"{state}\"\nrevision = 1\n\
         commitOwed = {owed}\ncreatedAt = \"2026-11-10T10:00:00Z\"\n{AGENT}",
        stream.id
    )
}

/// A change action request of `change` on the stream in `state`.
pub(super) fn action_request(stream: &WorkStream, change: &str, state: &str) -> String {
    format!(
        "id = \"r1\"\nstreamId = \"{}\"\nchange = \"{change}\"\naction = \"apply\"\n\
         rationale = \"r\"\nthreadId = \"t1\"\ncommentId = \"m1\"\nstate = \"{state}\"\n\
         createdAt = \"2026-11-10T10:00:00Z\"\n{AGENT}",
        stream.id
    )
}

/// The records of the application's diagnostic buffer that name the stream.
/// nextest runs each test in its own process, so no other test adds to them.
pub(super) fn stream_records(stream: &WorkStream) -> Vec<crate::logging::LogRecord> {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..crate::logging::LogFilter::default()
    };
    crate::logging::BUFFER
        .query(&filter, None, 5000)
        .expect("a page")
        .records
        .into_iter()
        .filter(|r| r.fields.get("stream_id") == Some(&serde_json::json!(stream.id)))
        .collect()
}

/// The one record of `message` that names the stream.
pub(super) fn one_record(stream: &WorkStream, message: &str) -> crate::logging::LogRecord {
    let found: Vec<_> =
        stream_records(stream).into_iter().filter(|r| r.message == message).collect();
    assert_eq!(found.len(), 1, "one {message:?} record: {found:?}");
    found.into_iter().next().unwrap()
}
