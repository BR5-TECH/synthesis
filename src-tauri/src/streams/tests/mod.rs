//! Tests for work streams (`WKS-work-streams.md`).

use std::path::{Path, PathBuf};

use tauri::Manager;
use tempfile::TempDir;

use super::*;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    project: TempDir,
    #[allow(dead_code)]
    store: TempDir,
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Fixture {
    fn new() -> Self {
        let project = TempDir::new().expect("project dir");
        let store = TempDir::new().expect("store dir");
        let root = canonical(project.path());
        init_repo(&root);

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        handle.manage(crate::fs::FsAccessState::default());
        handle.manage(crate::project::ProjectState::default());
        handle.manage(StreamState::rooted_at(canonical(store.path())));
        // The run order index a stream listing counts its queue from stands
        // under the same root, so a test never reads the machine's own store.
        handle.manage(crate::graduation::GraduationState::rooted_at(canonical(
            store.path(),
        )));
        // GRB-FR-XPLV: what a running merge attributes through.
        handle.manage(crate::progress::ProgressRegistry::default());
        // GRB-FR-KZUN: where a merge turn's activity is filed.
        handle.manage(std::sync::Arc::new(
            crate::agent_activity::ActivityStore::new(),
        ));

        handle
            .state::<crate::fs::FsAccessState>()
            .install_for_worktree_and(&root, &canonical(store.path()))
            .expect("two real directories");
        let access = handle
            .state::<crate::fs::FsAccessState>()
            .get()
            .expect("an instance");
        let state = handle.state::<crate::project::ProjectState>();
        state.set_root_with_access(root.clone(), Some(access));
        state.set_anchor(root.to_string_lossy().into_owned());

        Fixture {
            app: handle,
            project,
            store,
        }
    }

    fn root(&self) -> PathBuf {
        canonical(self.project.path())
    }

    fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.root()).expect("repo")
    }

    fn create(&self, name: &str, base: Option<&str>) -> Result<WorkStream, String> {
        create_work_stream(self.app.clone(), name.to_string(), base.map(str::to_string))
    }
}

fn init_repo(root: &Path) -> git2::Repository {
    let repo = git2::Repository::init(root).expect("init");
    {
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Test Author").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        // Set at repository level, which overrides the developer's own global
        // configuration. `core.excludesFile` feeds the ignore rules a merge
        // reads, and the other two change what a conflicted file holds — so a
        // machine with any of them set would otherwise run a different merge
        // from CI, in a suite whose whole subject is what lands on a branch.
        config.set_str("core.excludesFile", "").unwrap();
        config.set_str("core.autocrlf", "false").unwrap();
        config.set_str("merge.conflictStyle", "merge").unwrap();
    }
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    commit_all(&repo, "seed");
    repo
}

fn commit_all(repo: &git2::Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    index.write().expect("write");
    let tree = repo
        .find_tree(index.write_tree().expect("tree"))
        .expect("tree");
    let signature = repo.signature().expect("signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &refs)
        .expect("commit")
}

fn branch_names(repo: &git2::Repository) -> Vec<String> {
    repo.branches(Some(git2::BranchType::Local))
        .expect("branches")
        .flatten()
        .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
        .collect()
}

/// A merge is asynchronous because a semantic turn is, so every test drives it
/// on a runtime of its own.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

/// What one scripted turn does to the checkout it is given, and what it
/// reports.
enum Script {
    /// Write these `(relative path, content)` pairs, then report success.
    Settle(Vec<(String, String)>),
    /// Report success without touching anything, which leaves every marker.
    ClaimSuccessAndChangeNothing,
    /// Write these pairs, then run `side_effect` — a base branch that moves, a
    /// worktree the author dirties — before reporting success.
    SettleAnd(Vec<(String, String)>, Box<dyn Fn() + Send + Sync>),
    /// Report an escalation.
    Escalate,
    /// Fail before any container exists.
    Unlaunchable,
}

/// The dispatch seam with the execution agent removed.
///
/// What a test drives is the production merge with no container, no vendor CLI
/// and no credential anywhere near it.
struct ScriptedDispatch {
    scripts: std::sync::Mutex<std::collections::VecDeque<Script>>,
    /// Every request the merge made, so a test can assert on the mount and the
    /// task input rather than on what it expected them to be.
    seen: std::sync::Mutex<Vec<crate::tools::agent_exec::AgentExecutionRequest>>,
}

impl ScriptedDispatch {
    fn new(scripts: Vec<Script>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            scripts: std::sync::Mutex::new(scripts.into_iter().collect()),
            seen: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn turns(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

/// Every file under `root`, named relative to it.
fn files_under(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

impl crate::graduation::driver::GraduationDispatch<tauri::test::MockRuntime> for ScriptedDispatch {
    fn dispatch_graduation_turn<'a>(
        &'a self,
        _app: &'a tauri::AppHandle<tauri::test::MockRuntime>,
        _project_key: &'a str,
        request: crate::tools::agent_exec::AgentExecutionRequest,
    ) -> crate::graduation::driver::DispatchFuture<'a> {
        use crate::tools::agent_exec::protocol::{
            AgentEscalation, AgentEscalationQuestion, AgentOutcome, AgentResponseEnvelope,
            PROTOCOL_VERSION,
        };
        use crate::tools::agent_exec::runtime::CapturedStream;
        use crate::tools::agent_exec::{AgentExecution, ProcessOutcome};

        let directory = request.execution_directory.clone();
        self.seen.lock().unwrap().push(request);
        let script = self
            .scripts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Script::ClaimSuccessAndChangeNothing);
        Box::pin(async move {
            let (outcome, escalation) = match script {
                Script::Unlaunchable => {
                    return Err(crate::tools::agent_exec::AgentExecutionError::RuntimeUnavailable)
                }
                Script::Settle(writes) => {
                    for (path, content) in writes {
                        let target = directory.join(path);
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent).unwrap();
                        }
                        std::fs::write(target, content).unwrap();
                    }
                    (AgentOutcome::Success, None)
                }
                Script::SettleAnd(writes, side_effect) => {
                    for (path, content) in writes {
                        let target = directory.join(path);
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent).unwrap();
                        }
                        std::fs::write(target, content).unwrap();
                    }
                    side_effect();
                    (AgentOutcome::Success, None)
                }
                Script::ClaimSuccessAndChangeNothing => (AgentOutcome::Success, None),
                Script::Escalate => (
                    AgentOutcome::EscalationRequired,
                    Some(AgentEscalation {
                        reason: "Two requirements contradict each other.".to_string(),
                        questions: vec![AgentEscalationQuestion {
                            question: "Which limit stands?".to_string(),
                            options: Vec::new(),
                        }],
                    }),
                ),
            };
            Ok(AgentExecution {
                process_outcome: ProcessOutcome::Completed,
                exit_code: Some(0),
                response: Some(AgentResponseEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    outcome,
                    summary: "scripted".to_string(),
                    result: None,
                    failure: None,
                    escalation,
                    session: None,
                    metadata: None,
                }),
                stdout: CapturedStream::default(),
                stderr: CapturedStream::default(),
                duration_ms: 0,
                resume_unavailable: false,
                session: None,
                container_removed: true,
            })
        })
    }
}

/// The request of a merge on the production path.
///
/// A merge that Git settles needs no turn, and one it cannot settle is handed to
/// a merge run, so the request never dispatches an agent.
fn merge(
    fx: &Fixture,
    stream_id: &str,
    publication: StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    merge_work_stream_blocking(&fx.app, stream_id, publication)
}

fn worktree_names(repo: &git2::Repository) -> Vec<String> {
    repo.worktrees()
        .map(|list| {
            list.iter()
                .filter_map(|n| n.ok().flatten().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A stream and a base branch that changed the same region of one file, which
/// is the one thing a text merge cannot settle.
fn conflicting_stream(fx: &Fixture) -> WorkStream {
    let stream = fx.create("conflicting", None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(
        Path::new(&stream.worktree_path).join("README.md"),
        format!("{STREAM_SIDE}\n"),
    )
    .unwrap();
    commit_all(&worktree, "stream edit");
    std::fs::write(fx.root().join("README.md"), format!("{BASE_SIDE}\n")).unwrap();
    commit_all(&fx.repo(), "base edit");
    stream
}

/// Sentinels rather than the words "base" and "stream", so an assertion about
/// a side's content cannot be satisfied by a marker's own label.
const BASE_SIDE: &str = "BASE-SIDE-CONTENT";
const STREAM_SIDE: &str = "STREAM-SIDE-CONTENT";
const ANCESTOR_SIDE: &str = "seed";

/// One update on the production job, driven to rest.
///
/// The job is what a command starts, so a test that asserts on the update
/// record drives the same thread the application drives.
fn update_job_scripted(
    fx: &Fixture,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    base_revision: &str,
    decisions: Vec<StreamMergeDecision>,
    dispatch: std::sync::Arc<ScriptedDispatch>,
) -> Result<StreamUpdateRecord, String> {
    let (_, handle) = update_job::start_with(
        &fx.app,
        stream,
        strategy,
        base_revision.to_string(),
        Vec::new(),
        decisions,
        dispatch,
    )?;
    handle.join().expect("the update thread rested");
    Ok(update_of(fx, &stream.id).expect("an update record"))
}

/// The update record as it stands on disk right now.
fn update_of(fx: &Fixture, stream_id: &str) -> Option<StreamUpdateRecord> {
    get_work_stream_update(fx.app.clone(), stream_id.to_string()).expect("a known stream")
}

/// The revision a branch names right now.
fn tip_of(repo: &git2::Repository, branch: &str) -> git2::Oid {
    repo.find_branch(branch, git2::BranchType::Local)
        .expect("branch")
        .get()
        .peel_to_commit()
        .expect("commit")
        .id()
}

/// One summary out of the project's listing.
fn summary_of(fx: &Fixture, stream_id: &str) -> WorkStreamSummary {
    list_work_streams(fx.app.clone())
        .expect("a listing")
        .into_iter()
        .find(|summary| summary.stream.id == stream_id)
        .expect("the stream is listed")
}

/// A stream whose base branch has moved under it, without a conflict.
///
/// The base changes a file the stream never touched, so Git settles the whole
/// update on its own.
fn stream_behind_its_base(fx: &Fixture) -> WorkStream {
    let stream = fx.create("behind", None).expect("created");
    std::fs::write(fx.root().join("base-only.txt"), "from the base\n").unwrap();
    commit_all(&fx.repo(), "base moved on");
    stream
}

mod concurrency;
mod creation;
mod deletion;
mod deletion_events;
mod deletion_guards;
mod listing;
mod merge_apply_tests;
mod merge_clean;
mod merge_listing;
mod merge_shapes;
mod merge_refusals;
mod merge_second_request;
mod merge_snapshot_shapes;
mod merge_snapshot_tests;
mod merge_unsupported;
mod merge_support;
mod update_flow;
mod update_guards;
