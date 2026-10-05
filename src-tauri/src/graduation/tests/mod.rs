//! The graduation run's own tests, and the fixture they share.
//!
//! Every scenario here drives the **production loop** with the execution agent
//! removed at the one seam `GXD-graduation-execution.md` GXD-FR-DRFF names. No
//! test reaches a container runtime, an agentic CLI, a provider credential or
//! the network, which is what `../ai/GRL-graduation-loop.md` GRL-FR-GIMN and
//! `GRD-graduation.md` GRD-FR-KVNZ each ask of every behaviour of their module.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::Manager;
use tempfile::TempDir;

use crate::graduation::*;
use crate::tools::agent_exec::protocol::{
    AgentEscalation, AgentEscalationQuestion, AgentOutcome, AgentResponseEnvelope, SessionRef,
    PROTOCOL_VERSION,
};
use crate::tools::agent_exec::runtime::CapturedStream;
use crate::tools::agent_exec::{
    AgentExecution, AgentExecutionError, AgentExecutionRequest, ProcessOutcome, TurnKind,
};

mod blocking;
mod commit_retitle;
mod direct;
mod e2e;
mod github_shadow;
mod live_index;
mod log_read_command;
mod log_reads;
mod logs;
mod loop_driven;
mod merge_handoff;
mod merge_input;
mod merge_logs;
mod merge_passes;
mod merge_prompts;
mod merge_records;
mod merge_run;
mod prompts;
mod review_checkout;
mod run_progress;
mod scheduler;
mod standing_work;
mod state_machine;
mod stream_hold;

type Runtime = tauri::test::MockRuntime;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Every line of one durable log stream of a run, decoded.
fn lines_of(
    fx: &Fixture,
    run_id: &str,
    stream: crate::graduation::logs::GraduationLogStream,
) -> Vec<serde_json::Value> {
    let path = crate::graduation::logs::paths_for(&fx.app, run_id)
        .expect("the run's log paths")
        .stream(stream);
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A project in a real repository, with the run store and the stream store
/// rooted at temporary directories of their own.
struct Fixture {
    app: tauri::AppHandle<Runtime>,
    project: TempDir,
    #[allow(dead_code)]
    store: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let project = TempDir::new().expect("project dir");
        let store = TempDir::new().expect("store dir");
        let root = canonical(project.path());
        let store_root = canonical(store.path());
        init_repo(&root);

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        handle.manage(crate::fs::FsAccessState::default());
        handle.manage(crate::project::ProjectState::default());
        handle.manage(crate::streams::StreamState::rooted_at(store_root.clone()));
        handle.manage(std::sync::Arc::new(crate::agent_activity::ActivityStore::new()));
        handle.manage(GraduationState::rooted_at(store_root.clone()));
        handle.manage(crate::graduation::logs::LogRegistry::default());
        // What a push composes (GTC-FR-YHDU). Managed here rather than in the
        // one test that pushes, because a handle holding none of it reports
        // `push_unavailable` from the guard and never reaches the transfer.
        handle.manage(crate::global_settings::GlobalSettingsStore::in_memory());
        handle.manage(crate::github_tokens::GithubTokens::default());
        handle.manage(crate::progress::ProgressRegistry::default());
        // GSU-FR-HIPF: the start's own checks read configuration and
        // verification state rather than a daemon or a registry, so they are
        // exercisable here — with collaborators that reach neither the
        // machine's real filesystem nor its keychain.
        handle.manage(crate::agentic::AgenticIntegrations::new(
            Box::new(EverythingIsThere),
            Box::new(NoCli),
            Box::new(NoEndpoint),
            Box::new(EveryKeyIsThere),
        ));

        handle
            .state::<crate::fs::FsAccessState>()
            .install_for_worktree_and(&root, &store_root)
            .expect("two real directories");
        let access = handle
            .state::<crate::fs::FsAccessState>()
            .get()
            .expect("an instance");
        let state = handle.state::<crate::project::ProjectState>();
        state.set_root_with_access(root.clone(), Some(access));
        state.set_anchor(root.to_string_lossy().into_owned());
        // The repository machine store, named rather than resolved, so the
        // suite writes into its own tempdir (RMS-FR-ZXHM).
        state.set_store(root.clone());

        Fixture {
            app: handle,
            project,
            store,
        }
    }

    fn root(&self) -> PathBuf {
        canonical(self.project.path())
    }

    /// Hold a stream, so a run enqueued on it stays in its queue.
    ///
    /// `start_graduation` ends by offering the stream's queue a chance to
    /// dispatch (GSU-FR-NCJN). A test about what a start **records** is not a
    /// test about what a turn then does, so it takes the stream first and the
    /// enqueued run waits exactly as one behind another run does.
    fn hold_stream(&self, stream_id: &str) {
        assert!(
            self.app.state::<GraduationState>().claim(
                stream_id,
                "a run this test holds the stream with",
                false,
                crate::project_settings::GraduationConcurrency::limited(1),
                crate::tools::agent_exec::runtime::CancellationToken::new(),
            ),
            "the stream was free to hold"
        );
    }

    /// GSU-FR-GLVQ: make the image preflight pass, so `start_graduation` and
    /// `restart_graduation_run` are reachable.
    ///
    /// Three pieces of configuration and nothing else: a verified CLI vendor,
    /// the image this project commits for it, and a Docker backend the machine
    /// has verified. Nothing is probed, launched or contacted.
    fn allow_execution(&self) {
        use crate::agentic::{AgenticRecord, PathOrigin};
        let store = self.app.state::<crate::global_settings::GlobalSettingsStore>();
        store
            .save_agentic_registry(
                vec![AgenticRecord {
                    vendor: "claude_code".into(),
                    binary_path: Some("/opt/agents/claude".into()),
                    path_origin: PathOrigin::Detected,
                    ..AgenticRecord::default()
                }],
                Some("claude_code".into()),
            )
            .expect("a registry");

        let access = self
            .app
            .state::<crate::fs::FsAccessState>()
            .get()
            .expect("an instance");
        let root = crate::fs::RootFs::new(self.root(), access);
        crate::project_settings::save_project_vendor_image_to(
            &root,
            "claude_code",
            &crate::project_settings::images::ProjectVendorImage {
                image_name: "acme/agent".into(),
                tag: Some("latest".into()),
                dockerfile: None,
            },
        )
        .expect("an image");

        let docker = crate::docker::DockerBackendConfig {
            mode: crate::docker::DockerBackendMode::Bollard,
            endpoint: crate::docker::DockerEndpoint::Automatic,
            cli_path: None,
        };
        store.save_docker_backend(&docker).expect("a backend");
        store
            .record_docker_verification(&docker, "27.0.0", &crate::notes::now_rfc3339())
            .expect("a verification");
    }

    /// The repository every stream and every review checkout is registered in.
    fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.root()).expect("repo")
    }

    /// A work stream on the production path.
    fn stream(&self, name: &str) -> crate::streams::WorkStream {
        crate::streams::create_work_stream(self.app.clone(), name.to_string(), None)
            .expect("a stream")
    }

    /// A real draft in the project, so what a run is attributed to exists.
    ///
    /// A draft that is not there takes its statistics log with it
    /// (`DRS-draft-storage.md` DRS-FR-ZLBK), so a run whose source draft the
    /// fixture never made is counted nowhere.
    fn draft(&self, name: &str) -> String {
        let access = self
            .app
            .state::<crate::fs::FsAccessState>()
            .get()
            .expect("an instance");
        let root = crate::fs::RootFs::new(self.root(), access);
        crate::drafts::create_draft_at_root(&root, Some(name))
            .expect("a draft")
            .draft
            .id
    }

    /// A queued run over a captured prompt, on the production record shape.
    ///
    /// `start_graduation` is not the route, because its image preflight refuses
    /// on a machine with no container runtime (GSU-FR-GLVQ) and every scenario
    /// here is about what the loop does after a run exists.
    fn enqueue(&self, stream: &crate::streams::WorkStream, prompt: &str) -> GraduationRun {
        self.enqueue_for(stream, prompt, "d1")
    }

    /// The same, over a draft the caller named.
    fn enqueue_for(
        &self,
        stream: &crate::streams::WorkStream,
        prompt: &str,
        draft_id: &str,
    ) -> GraduationRun {
        self.enqueue_with(stream, prompt, draft_id, StandingWork::default())
    }

    /// The same, under a standing-work choice the caller named (GRD-FR-HQPD).
    fn enqueue_with(
        &self,
        stream: &crate::streams::WorkStream,
        prompt: &str,
        draft_id: &str,
        standing_work: StandingWork,
    ) -> GraduationRun {
        self.enqueue_under(stream, prompt, draft_id, standing_work, None)
    }

    /// The same, under a commit message the caller named (GRD-FR-RJFC).
    fn enqueue_under(
        &self,
        stream: &crate::streams::WorkStream,
        prompt: &str,
        draft_id: &str,
        standing_work: StandingWork,
        standing_work_message: Option<String>,
    ) -> GraduationRun {
        let input = CapturedGraduationInput {
            draft_id: draft_id.to_string(),
            draft_name: "editor draft".to_string(),
            prompt: prompt.to_string(),
            prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
            captured_at: crate::notes::now_rfc3339(),
        };
        let mut run = crate::graduation::commands::new_run(
            &self.project_key(),
            stream,
            input,
            standing_work,
            standing_work_message,
        );
        crate::graduation::save_run(&self.app, &mut run).expect("a saved run");
        run
    }

    fn project_key(&self) -> String {
        self.app
            .state::<crate::project::ProjectState>()
            .slot_key()
    }

    /// Drive one run to rest, with the agent scripted.
    fn drive(&self, run: &GraduationRun, dispatch: Arc<ScriptedDispatch>) -> GraduationRun {
        crate::graduation::driver::drive_for_test(
            &self.app,
            &run.id,
            &run.queue_key(),
            dispatch as Arc<dyn crate::graduation::driver::GraduationDispatch<Runtime>>,
        );
        self.reload(&run.id)
    }

    /// The same, reporting whether the stream was free to be claimed.
    fn try_drive(&self, run: &GraduationRun, dispatch: Arc<ScriptedDispatch>) -> bool {
        crate::graduation::driver::drive_for_test(
            &self.app,
            &run.id,
            &run.queue_key(),
            dispatch as Arc<dyn crate::graduation::driver::GraduationDispatch<Runtime>>,
        )
    }

    fn reload(&self, run_id: &str) -> GraduationRun {
        crate::graduation::load_run(&self.app, run_id).expect("the run record")
    }

    /// The author's Continue, without the loop starting behind it.
    ///
    /// The scenarios that use this drive the run themselves afterwards, so the
    /// dispatch each one wants is the one it scripts rather than whatever the
    /// queue would have picked up.
    fn continue_run(&self, run: &GraduationRun) -> GraduationRun {
        use tauri::Manager;
        self.app.state::<GraduationState>().set_loop_enabled(false);
        let continued =
            crate::graduation::continue_graduation_run(self.app.clone(), run.id.clone())
                .expect("continued");
        self.app.state::<GraduationState>().set_loop_enabled(true);
        continued
    }
}

fn init_repo(root: &Path) -> git2::Repository {
    let repo = git2::Repository::init(root).expect("init");
    {
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Test Author").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        // Set at repository level, which overrides the developer's own global
        // configuration, so one machine cannot run a different check from CI.
        config.set_str("core.excludesFile", "").unwrap();
        config.set_str("core.autocrlf", "false").unwrap();
    }
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    index.write().expect("write");
    let tree = repo
        .find_tree(index.write_tree().expect("tree"))
        .expect("tree");
    let signature = repo.signature().expect("signature");
    repo.commit(Some("HEAD"), &signature, &signature, "seed", &tree, &[])
        .expect("commit");
    drop(index);
    drop(tree);
    repo
}

// ---------------------------------------------------------------------------
// The scripted agent
// ---------------------------------------------------------------------------

/// What one scripted turn answers with.
enum Answer {
    /// The agent reported the work finished.
    Success,
    /// A review turn's verdict.
    Verdict(ReviewVerdict),
    /// A `result` object of any shape, for the verdicts the loop refuses.
    RawResult(serde_json::Value),
    /// The agent stopped to ask the author something.
    Escalate(Vec<String>),
    /// The same, under a reason the test names, so an escalation outside the
    /// rules of ESU-FR-19 can be composed.
    EscalateBecause { reason: String, questions: Vec<String> },
    /// GXD-FR-LBYG: an envelope that claims a change set of its own, so a loop
    /// that believed one would be caught.
    ClaimingPaths(Vec<String>),
    /// GRL-FR-DNKA: the agent reported it could not do the work, and said what
    /// stopped it.
    ReportedFailure(String),
    /// GRL-FR-DNKA: the agent reported a failure whose message says nothing, so
    /// its summary is the only account there is.
    ReportedFailureWithoutMessage(String),
    /// GRL-FR-DNKA: the agent reported the work finished and said something
    /// about it in its summary — the case that carries an account the review
    /// most needs, because a turn that left part of the work undone reports
    /// success and says the rest here.
    SucceedingWith(String),
    /// The process did not complete.
    Process(ProcessOutcome),
    /// Nothing was launched at all.
    Unlaunchable(AgentExecutionError),
}

/// One scripted turn: what it writes, what it then answers.
struct Turn {
    writes: Vec<(String, String)>,
    /// Symbolic links the turn installs, as a dependency install does.
    links: Vec<(String, String)>,
    deletes: Vec<String>,
    removed_dirs: Vec<String>,
    answer: Answer,
    /// Something to do inside the turn — cancel the run, move a branch.
    side_effect: Option<Box<dyn Fn() + Send + Sync>>,
    /// Whether the answer carries a resumable vendor session.
    session: bool,
    /// Whether the turn answers as scripted even where the run was stopped
    /// while it ran, as an agent that finished at the same moment does.
    ignores_cancellation: bool,
}

impl Turn {
    fn answering(answer: Answer) -> Self {
        Self {
            writes: Vec::new(),
            links: Vec::new(),
            deletes: Vec::new(),
            removed_dirs: Vec::new(),
            answer,
            side_effect: None,
            session: false,
            ignores_cancellation: false,
        }
    }

    fn ignoring_cancellation(mut self) -> Self {
        self.ignores_cancellation = true;
        self
    }

    /// A work turn that reports the work finished.
    fn work() -> Self {
        Self::answering(Answer::Success)
    }

    /// A work turn that reports the work finished and says something about it.
    fn work_saying(summary: &str) -> Self {
        Self::answering(Answer::SucceedingWith(summary.to_string()))
    }

    /// A review turn that lets the work through.
    fn ready() -> Self {
        Self::answering(Answer::Verdict(ReviewVerdict {
            verdict: ReviewOutcome::Ready,
            rationale: "The work answers the prompt.".to_string(),
            findings: Vec::new(),
        }))
    }

    /// A review turn that asks for one change of the named severity.
    fn revise(severity: ReviewSeverity, description: &str) -> Self {
        Self::answering(Answer::Verdict(ReviewVerdict {
            verdict: ReviewOutcome::Revise,
            rationale: "One thing is not delivered.".to_string(),
            findings: vec![ReviewFinding {
                severity,
                description: description.to_string(),
                affected_files: vec!["src/panel.ts".to_string()],
                correction: "Write the empty state.".to_string(),
            }],
        }))
    }

    fn writing(mut self, path: &str, content: &str) -> Self {
        self.writes.push((path.to_string(), content.to_string()));
        self
    }

    fn deleting(mut self, path: &str) -> Self {
        self.deletes.push(path.to_string());
        self
    }

    /// A symbolic link the turn installs, relative to the directory it runs in.
    ///
    /// This is what a project's own dependency install writes, and what the
    /// application must still be able to reclaim afterwards (FSA-FR-ZUCF).
    #[cfg(unix)]
    fn linking(mut self, path: &str, target: &str) -> Self {
        self.links.push((path.to_string(), target.to_string()));
        self
    }

    /// Remove a whole directory, as a turn that reorganizes a module does.
    fn removing_dir(mut self, path: &str) -> Self {
        self.removed_dirs.push(path.to_string());
        self
    }

    fn with_session(mut self) -> Self {
        self.session = true;
        self
    }

    fn doing(mut self, side_effect: impl Fn() + Send + Sync + 'static) -> Self {
        self.side_effect = Some(Box::new(side_effect));
        self
    }
}

/// What the loop asked of one turn, and what that turn found in front of it.
#[derive(Clone, Debug)]
struct Seen {
    /// `work`, `review` or `semantic_merge`.
    part: String,
    purpose: String,
    pass: u32,
    turn_kind: TurnKind,
    directory: PathBuf,
    /// GXD-FR-IOZU: whether the dispatch carried an activity sink.
    had_activity: bool,
    /// GXD-FR-MTVR: the execution timeout this dispatch was composed under.
    timeout_ms: u64,
    /// GRL-FR-DXLU: the instruction the turn was handed, whole.
    instruction: String,
    input: serde_json::Map<String, serde_json::Value>,
    /// Every file of the execution directory when the turn started, outside
    /// the repository's own admin directory.
    tree: BTreeMap<String, String>,
    /// The revision the execution directory stood at.
    head: Option<String>,
    /// What `git diff HEAD` would name there, untracked files included.
    diff_paths: Vec<String>,
    /// The tree the execution directory's working copy holds.
    worktree_tree: Option<String>,
    /// GXD-FR-XPUR: the vendor session the dispatch asked to resume.
    resume_session: Option<String>,
    /// GRB-FR-TOOU: whether the dispatch carried the semantic-rebase mount.
    semantic_mount: bool,
}

impl Seen {
    fn instruction_field(&self, key: &str) -> Option<String> {
        self.input
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// How many entries a list field holds, whatever their shape.
    fn list_len(&self, key: &str) -> usize {
        self.input
            .get(key)
            .and_then(|v| v.as_array())
            .map(|list| list.len())
            .unwrap_or(0)
    }

    fn path_list(&self, key: &str) -> Vec<String> {
        self.input
            .get(key)
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The dispatch seam with the execution agent removed (GXD-FR-DRFF).
///
/// No command, project setting or provider record selects this over the
/// production one: a substitution is a test's act rather than a configuration.
struct ScriptedDispatch {
    turns: Mutex<std::collections::VecDeque<Turn>>,
    seen: Mutex<Vec<Seen>>,
}

impl ScriptedDispatch {
    fn new(turns: Vec<Turn>) -> Arc<Self> {
        Arc::new(Self {
            turns: Mutex::new(turns.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn turns_taken(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    /// The turns of one part, in the order the loop dispatched them.
    fn of_part(&self, part: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.part == part)
            .collect()
    }
}

/// Every file under `root`, named relative to it, with its content.
///
/// The repository's own admin entry is left out: it is Git's bookkeeping rather
/// than anything a turn was given to read.
pub(super) fn tree_under(root: &Path) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == ".git" || rel.starts_with(".git/") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(rel, std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    out
}

/// What the execution directory held when a turn started, read through Git.
fn read_git_state(directory: &Path) -> (Option<String>, Vec<String>, Option<String>) {
    let Ok(repo) = git2::Repository::open(directory) else {
        return (None, Vec::new(), None);
    };
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .map(|c| c.id().to_string());
    let head_tree = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .and_then(|c| c.tree().ok());
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let mut diff_paths: Vec<String> = Vec::new();
    if let Ok(diff) = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts)) {
        for delta in diff.deltas() {
            for file in [delta.new_file(), delta.old_file()] {
                if let Some(path) = file.path() {
                    diff_paths.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }
    diff_paths.sort();
    diff_paths.dedup();

    // The checkout is thrown away when the turn ends, so staging it here costs
    // nothing and is the cheapest way to name the tree it holds.
    let worktree_tree = repo.index().ok().and_then(|mut index| {
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .ok()?;
        index.write_tree().ok().map(|oid| oid.to_string())
    });
    (head, diff_paths, worktree_tree)
}

impl crate::graduation::driver::GraduationDispatch<Runtime> for ScriptedDispatch {
    fn dispatch_graduation_turn<'a>(
        &'a self,
        _app: &'a tauri::AppHandle<Runtime>,
        _project_key: &'a str,
        request: AgentExecutionRequest,
    ) -> crate::graduation::driver::DispatchFuture<'a> {
        let directory = request.execution_directory.clone();
        let cancellation = request.cancellation.clone();
        let input = request.task.input.clone().unwrap_or_default();
        let request = std::sync::Arc::new(request);
        let (head, diff_paths, worktree_tree) = read_git_state(&directory);
        self.seen.lock().unwrap().push(Seen {
            part: input
                .get("part")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            purpose: input
                .get("purpose")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            pass: input
                .get("pass")
                .and_then(|v| v.as_u64())
                .unwrap_or_default() as u32,
            turn_kind: request.turn_kind,
            directory: directory.clone(),
            had_activity: request.activity.is_some(),
            timeout_ms: request.task.execution.timeout_ms,
            instruction: request.task.instruction.clone(),
            input,
            tree: tree_under(&directory),
            head,
            diff_paths,
            worktree_tree,
            resume_session: request
                .task
                .resume
                .as_ref()
                .and_then(|resume| resume.session_id.clone()),
            semantic_mount: request.supplementary_mount.is_some(),
        });

        let turn = self.turns.lock().unwrap().pop_front().unwrap_or_else(|| {
            panic!(
                "the loop dispatched a turn nobody scripted: part {:?}, purpose {:?}",
                self.seen.lock().unwrap().last().map(|s| s.part.clone()),
                self.seen.lock().unwrap().last().map(|s| s.purpose.clone()),
            )
        });
        Box::pin(async move {
            for (path, content) in &turn.writes {
                let target = directory.join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(target, content).unwrap();
            }
            #[cfg(unix)]
            for (path, target) in &turn.links {
                let link = directory.join(path);
                if let Some(parent) = link.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                let _ = std::fs::remove_file(&link);
                std::os::unix::fs::symlink(target, &link).unwrap();
            }
            for path in &turn.deletes {
                let _ = std::fs::remove_file(directory.join(path));
            }
            for path in &turn.removed_dirs {
                let _ = std::fs::remove_dir_all(directory.join(path));
            }
            if let Some(side_effect) = turn.side_effect.as_ref() {
                side_effect();
            }
            // EAC-FR-34: the executor reports what it did through the sink it
            // was given, so a test can assert the caller is actually watching.
            if let Some(sink) = request.activity.as_ref() {
                sink.activity(crate::tools::agent_exec::AgentActivityEvent {
                    at: crate::notes::now_rfc3339(),
                    channel: "scripted",
                    kind: "turn",
                    summary: "the scripted turn ran".to_string(),
                    payload: String::new(),
                    payload_truncated: false,
                });
            }
            // EAC-FR-25: the executor honours the cancellation token, so a turn
            // stopped while it ran reports that rather than an answer.
            if cancellation.is_cancelled() && !turn.ignores_cancellation {
                return Ok(execution(ProcessOutcome::Cancelled, None, false));
            }

            let mut summary = "scripted".to_string();
            let mut reported: Option<crate::tools::agent_exec::protocol::AgentFailure> = None;
            if let Answer::ClaimingPaths(claimed) = &turn.answer {
                summary = format!("I wrote {}", claimed.join(", "));
            }
            if let Answer::SucceedingWith(said) = &turn.answer {
                summary = said.clone();
            }
            if let Answer::ReportedFailureWithoutMessage(said) = &turn.answer {
                summary = said.clone();
            }
            let (outcome, result, escalation) = match turn.answer {
                Answer::Unlaunchable(error) => return Err(error),
                Answer::Process(process_outcome) => {
                    return Ok(execution(process_outcome, None, turn.session))
                }
                Answer::Success => (AgentOutcome::Success, None, None),
                Answer::SucceedingWith(_) => (AgentOutcome::Success, None, None),
                Answer::ClaimingPaths(claimed) => (
                    AgentOutcome::Success,
                    object_of(serde_json::json!({ "changed_paths": claimed.clone() })),
                    None,
                ),
                Answer::ReportedFailureWithoutMessage(_) => {
                    reported = Some(crate::tools::agent_exec::protocol::AgentFailure {
                        code: "could_not_finish".to_string(),
                        message: String::new(),
                        retryable: false,
                    });
                    (AgentOutcome::Failure, None, None)
                }
                Answer::ReportedFailure(message) => {
                    reported = Some(crate::tools::agent_exec::protocol::AgentFailure {
                        code: "could_not_finish".to_string(),
                        message,
                        retryable: false,
                    });
                    (AgentOutcome::Failure, None, None)
                }
                Answer::Verdict(verdict) => {
                    let value = serde_json::to_value(&verdict).expect("a verdict");
                    (AgentOutcome::Success, object_of(value), None)
                }
                Answer::RawResult(value) => (AgentOutcome::Success, object_of(value), None),
                Answer::EscalateBecause { reason, questions } => (
                    AgentOutcome::EscalationRequired,
                    None,
                    Some(AgentEscalation {
                        reason,
                        questions: questions
                            .into_iter()
                            .map(|question| AgentEscalationQuestion {
                                question,
                                options: Vec::new(),
                            })
                            .collect(),
                    }),
                ),
                Answer::Escalate(questions) => (
                    AgentOutcome::EscalationRequired,
                    None,
                    Some(AgentEscalation {
                        reason: "Only the author can settle this.".to_string(),
                        questions: questions
                            .into_iter()
                            .map(|question| AgentEscalationQuestion {
                                question,
                                options: Vec::new(),
                            })
                            .collect(),
                    }),
                ),
            };
            Ok(execution(
                ProcessOutcome::Completed,
                Some(AgentResponseEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    outcome,
                    summary: summary.clone(),
                    result,
                    failure: reported,
                    escalation,
                    session: turn.session.then(|| SessionRef {
                        session_id: Some("s1".to_string()),
                        continuation_token: None,
                    }),
                    metadata: None,
                }),
                turn.session,
            ))
        })
    }
}

fn object_of(value: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    match value {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

fn execution(
    process_outcome: ProcessOutcome,
    response: Option<AgentResponseEnvelope>,
    session: bool,
) -> AgentExecution {
    // A process that was stopped has no exit code, and one that exited with an
    // error has a code other than zero, as the real executor reports them.
    let exit_code = match process_outcome {
        ProcessOutcome::NonZeroExit => Some(1),
        ProcessOutcome::Timeout | ProcessOutcome::Cancelled | ProcessOutcome::Terminated => None,
        _ => Some(0),
    };
    AgentExecution {
        process_outcome,
        exit_code,
        response,
        stdout: CapturedStream::default(),
        stderr: CapturedStream::default(),
        duration_ms: 0,
        resume_unavailable: false,
        session: session.then(|| SessionRef {
            session_id: Some("s1".to_string()),
            continuation_token: None,
        }),
        container_removed: true,
    }
}

// ---------------------------------------------------------------------------
// The collaborators the image preflight reads through (GSU-FR-HIPF)
// ---------------------------------------------------------------------------

/// Every binary the registry names is there and runnable. The preflight reads
/// this rather than the machine, so a fixture that says yes is what makes a
/// configured vendor resolve.
struct EverythingIsThere;

impl crate::agentic::FileProbe for EverythingIsThere {
    fn exists(&self, _path: &Path) -> bool {
        true
    }
    fn is_executable(&self, _path: &Path) -> bool {
        true
    }
    fn dir_exists(&self, _path: &Path) -> bool {
        true
    }
}

/// Nothing here runs a child process: the preflight probes nothing, and a test
/// that reached this would be launching something.
struct NoCli;

impl crate::agentic::CliRunner for NoCli {
    fn run(
        &self,
        _path: &Path,
        _args: &[&str],
        _timeout: std::time::Duration,
    ) -> Result<crate::agentic::CliOutput, crate::agentic::RunError> {
        panic!("no turn of these tests runs a binary")
    }
}

/// The same for the network.
struct NoEndpoint;

impl crate::ai_shared::EndpointProber for NoEndpoint {
    fn probe(
        &self,
        _request: &crate::ai_shared::ProbeRequest,
    ) -> Result<Vec<crate::ai_shared::ModelOption>, crate::ai_shared::ProbeError> {
        panic!("no turn of these tests contacts an endpoint")
    }
}

/// A vault that holds a credential for every vendor, and hands none of it to
/// anything: the preflight asks whether one is present and never what it is.
struct EveryKeyIsThere;

impl crate::ai_shared::SecretStore for EveryKeyIsThere {
    fn set(&self, _id: &str, _secret: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
    fn get(&self, _id: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        Ok(Some("a credential this test never reads".into()))
    }
    fn delete(&self, _id: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
}
