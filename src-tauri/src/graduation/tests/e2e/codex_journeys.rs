//! Journey 55 of the coverage matrix: an implementation run whose project
//! executes Codex, driven through the **production** executor
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Every other journey removes the execution agent at the dispatch seam, so it
//! proves nothing about a vendor. This one binds a seam that hands each turn to
//! the shipped `AgentCliExecutor`, with only its container runtime replaced by
//! a double that answers in the Codex 0.172.0 event stream
//! (`../../../../../specifications/infra/CDX-codex-cli-protocol.md`). So the
//! vendor argument vector, the launch files, the credential, the session and
//! the milestone markers are all the production ones, and the loop reads back
//! what the production extractor made of the stream.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use super::super::{canonical, Fixture, Runtime};
use super::hooks::without_loop;
use crate::agentic::{AgenticRecord, CodexAuthMode, PathOrigin};
use crate::graduation::{
    GraduationEscalationAnswer, GraduationEscalationOrigin, GraduationRun, GraduationRunState,
};
use crate::tools::agent_exec::descriptor::{codex, ContainerSpec};
use crate::tools::agent_exec::protocol::ResultContract;
use crate::tools::agent_exec::runtime::{
    BoxFuture, CapturedStream, DockerRuntime, RunEnd, RunRequest, RuntimeError, RuntimeOutcome,
    StreamChannel,
};
use crate::tools::agent_exec::{
    AgentCliExecutor, AgentExecutionError, AgentExecutionRequest, LaunchContext, TurnKind,
};

/// The thread every scripted Codex turn reports, so a resume can be seen to
/// name it.
const THREAD: &str = "019a0c3e-codex-thread-0001";

/// The milestone markers a turn writes in one agent message (GRL-FR-KWPT).
const MILESTONES: &str =
    "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"Write the panel\",\"Check it\"]}\n\
SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1}\n\
SYNTHESIS_MILESTONE {\"event\":\"done\",\"index\":1}";

// ---------------------------------------------------------------------------
// The runtime double: the container is the only thing that is not production
// ---------------------------------------------------------------------------

/// One scripted container run: what it writes into the turn's directory, and
/// the stdout lines the CLI prints.
struct CodexTurn {
    writes: Vec<(&'static str, &'static str)>,
    lines: Vec<String>,
}

/// What one launch handed the runtime.
#[derive(Clone)]
struct Launched {
    container: ContainerSpec,
    /// The client environment's names, values left out: a secret is held
    /// there, and it is enough to know which variables were set.
    env_names: Vec<String>,
    final_event: bool,
}

#[derive(Default)]
struct CodexRuntime {
    turns: Mutex<VecDeque<CodexTurn>>,
    directory: Mutex<Option<PathBuf>>,
    launched: Mutex<Vec<Launched>>,
}

impl DockerRuntime for CodexRuntime {
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async { Ok(()) })
    }

    fn ensure_image<'a>(&'a self, _image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async { Ok(()) })
    }

    fn run<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            let container = request.container.cloned().expect("an agent turn has a container");
            self.launched.lock().unwrap().push(Launched {
                container,
                env_names: request.env.keys().cloned().collect(),
                final_event: request.final_event.is_some(),
            });
            let turn = self
                .turns
                .lock()
                .unwrap()
                .pop_front()
                .expect("the loop launched a Codex turn nobody scripted");
            let directory = self.directory.lock().unwrap().clone().expect("a turn directory");
            // Through the guarded handle rather than `std::fs`, on the terms
            // FSA-FR-19 binds every other write in the backend: the turn
            // directory is exactly what a root is for.
            let access = crate::fs::FsAccess::builder()
                .allow_root(&directory)
                .build()
                .expect("a real turn directory");
            for (path, content) in &turn.writes {
                access
                    .write_bytes_atomic(directory.join(path), content.as_ref())
                    .expect("the turn's file is writable");
            }
            // EAC-FR-32: the executor watches the stream as the CLI writes it.
            if let Some(observer) = request.observer {
                for line in &turn.lines {
                    observer.line(StreamChannel::Stdout, line.as_bytes(), true);
                }
            }
            let mut stdout = turn.lines.join("\n");
            stdout.push('\n');
            Ok(RuntimeOutcome {
                end: RunEnd::Exited,
                exit_code: Some(0),
                stdout: CapturedStream { bytes: stdout.into_bytes(), truncated: false },
                stderr: CapturedStream::default(),
                stop_unconfirmed: false,
            })
        })
    }

    fn remove_container<'a>(&'a self, _name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async { Ok(()) })
    }
}

/// What one dispatch was asked, as the seam saw it.
#[derive(Clone)]
struct Dispatched {
    turn_kind: TurnKind,
    instruction: String,
    result_contract: Option<ResultContract>,
    resume: Option<(Option<String>, Option<String>)>,
    /// The task input's `purpose` and its escalation answers, as text.
    purpose: String,
    answers: String,
}

/// The dispatch seam bound to the production executor (GXD-FR-KXXB), with the
/// launch context composed from the application exactly as production
/// composes it.
struct ExecutorDispatch {
    executor: AgentCliExecutor,
    runtime: Arc<CodexRuntime>,
    project_root: PathBuf,
    dispatched: Mutex<Vec<Dispatched>>,
}

impl crate::graduation::driver::GraduationDispatch<Runtime> for ExecutorDispatch {
    fn dispatch_graduation_turn<'a>(
        &'a self,
        app: &'a tauri::AppHandle<Runtime>,
        project_key: &'a str,
        request: AgentExecutionRequest,
    ) -> crate::graduation::driver::DispatchFuture<'a> {
        self.dispatched.lock().unwrap().push(Dispatched {
            turn_kind: request.turn_kind,
            purpose: input_text(&request, "purpose"),
            answers: input_text(&request, "escalation_answers"),
            instruction: request.task.instruction.clone(),
            result_contract: request.task.result_contract,
            resume: request
                .task
                .resume
                .as_ref()
                .map(|r| (r.session_id.clone(), r.continuation_token.clone())),
        });
        *self.runtime.directory.lock().unwrap() = Some(request.execution_directory.clone());
        Box::pin(async move {
            let store = app.state::<crate::global_settings::GlobalSettingsStore>();
            let integrations = app.state::<crate::agentic::AgenticIntegrations>();
            let fs = crate::graduation::store_fs(app).expect("the guarded handle");
            let access = app.state::<crate::fs::FsAccessState>().get().expect("an instance");
            let (_, images) = crate::tools::agent_exec::project_launch_inputs(
                crate::fs::RootFs::new(self.project_root.clone(), access),
            );
            let context = LaunchContext {
                store: &store,
                integrations: &integrations,
                project_key,
                fs: &fs,
                images: &images,
                docker_in_docker: Default::default(),
                devcontainer: None,
            };
            self.executor.execute_agent_cli(app, &context, request).await
        })
    }

    fn probe_graduation_image<'a>(
        &'a self,
        _app: &'a tauri::AppHandle<Runtime>,
        _project_key: &'a str,
        _request: crate::tools::agent_exec::image_probe::ImageProbeRequest,
    ) -> crate::graduation::driver::ProbeFuture<'a> {
        Box::pin(async { Err(AgentExecutionError::LaunchFailed("no probe here".into())) })
    }
}

/// One field of the task input, as JSON text, empty where it is absent.
fn input_text(request: &AgentExecutionRequest, key: &str) -> String {
    request
        .task
        .input
        .as_ref()
        .and_then(|input| input.get(key))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

impl ExecutorDispatch {
    fn new(fx: &Fixture, turns: Vec<CodexTurn>) -> Arc<Self> {
        let runtime = Arc::new(CodexRuntime::default());
        runtime.turns.lock().unwrap().extend(turns);
        // EAC-FR-31 / EAC-FR-BOXF: the host root of session state and launch
        // files, inside what the guarded handle allows, as production keeps
        // both under the application data directory.
        let executor = AgentCliExecutor::new(runtime.clone())
            .with_session_state_root(canonical(fx.store.path()).join("agent-exec"));
        Arc::new(Self {
            executor,
            runtime,
            project_root: fx.root(),
            dispatched: Mutex::new(Vec::new()),
        })
    }

    fn launched(&self) -> Vec<Launched> {
        self.runtime.launched.lock().unwrap().clone()
    }

    fn dispatched(&self) -> Vec<Dispatched> {
        self.dispatched.lock().unwrap().clone()
    }

    fn turns_left(&self) -> usize {
        self.runtime.turns.lock().unwrap().len()
    }
}

// ---------------------------------------------------------------------------
// The Codex event stream (CDX-FR-12, CDX-FR-28)
// ---------------------------------------------------------------------------

fn event(value: serde_json::Value) -> String {
    value.to_string()
}

/// One turn's stdout: the thread, optional milestone markers in a first agent
/// message, the envelope as the last agent message, and the final event.
fn codex_stream(milestones: bool, envelope: serde_json::Value) -> Vec<String> {
    let mut lines = vec![
        event(serde_json::json!({"type": "thread.started", "thread_id": THREAD})),
        event(serde_json::json!({"type": "turn.started"})),
        // CDX-FR-15: the notice the bypass flags print is not a failure.
        event(serde_json::json!({"type": "item.completed", "item": {"id": "item_0",
            "type": "error", "message": "`--dangerously-bypass-hook-trust` is enabled."}})),
    ];
    if milestones {
        lines.push(event(serde_json::json!({"type": "item.completed", "item": {"id": "item_1",
            "type": "agent_message", "text": MILESTONES}})));
    }
    lines.push(event(serde_json::json!({"type": "item.completed", "item": {"id": "item_2",
        "type": "agent_message", "text": envelope.to_string()}})));
    lines.push(event(serde_json::json!({"type": "turn.completed", "usage": {
        "input_tokens": 10, "cached_input_tokens": 0, "output_tokens": 5}})));
    lines
}

/// CDX-FR-TPFM: the strict schema makes every optional position present, so
/// the answer carries `null` where an envelope would leave a field out.
fn work_success() -> serde_json::Value {
    serde_json::json!({
        "protocol_version": 1, "outcome": "success", "summary": "The panel has its empty state.",
        "result": {"checks": [{"name": "tests", "command": "run the project's tests",
            "finished": true, "exit_status": 0, "waived_by": null}], "no_checks_reason": null},
        "failure": null, "escalation": null, "session": null, "metadata": null
    })
}

fn escalation(question: &str) -> serde_json::Value {
    serde_json::json!({
        "protocol_version": 1, "outcome": "escalation_required",
        "summary": "The prompt names no panel.", "result": null, "failure": null,
        "escalation": {"kind": null, "reason": "Two panels match the prompt.",
            "questions": [{"question": question, "options": null}]},
        "session": null, "metadata": null
    })
}

fn review_ready() -> serde_json::Value {
    serde_json::json!({
        "protocol_version": 1, "outcome": "success", "summary": "The review ran.",
        "result": {"verdict": "ready", "rationale": "The work answers the prompt.",
            "findings": [], "summary": null},
        "failure": null, "escalation": null, "session": null, "metadata": null
    })
}

// ---------------------------------------------------------------------------
// The journey's arrangement and acts
// ---------------------------------------------------------------------------

/// A machine whose one verified vendor is Codex, in `mode`.
fn codex_fixture(mode: CodexAuthMode) -> Fixture {
    let fx = Fixture::new();
    fx.allow_execution_for(AgenticRecord {
        vendor: "codex".into(),
        binary_path: Some("/opt/agents/codex".into()),
        path_origin: PathOrigin::Detected,
        codex_auth_mode: mode,
        ..AgenticRecord::default()
    });
    fx
}

fn drive(fx: &Fixture, run: &GraduationRun, dispatch: &Arc<ExecutorDispatch>) -> GraduationRun {
    crate::graduation::driver::drive_for_test(
        &fx.app,
        &run.id,
        &run.queue_key(),
        dispatch.clone() as Arc<dyn crate::graduation::driver::GraduationDispatch<Runtime>>,
    );
    fx.reload(&run.id)
}

fn answer(fx: &Fixture, run: &GraduationRun, text: &str) -> GraduationRun {
    let position = run.escalation.as_ref().expect("an escalation").questions[0].position;
    without_loop(&fx.app, || {
        crate::graduation::answer_graduation_escalation(
            fx.app.clone(),
            run.id.clone(),
            vec![GraduationEscalationAnswer {
                position,
                answer: text.to_string(),
                summary: text.to_string(),
                note: None,
            }],
            Default::default(),
        )
    })
    .expect("the answer is accepted")
}

/// The vendor part of one launch: what the image entrypoint receives.
fn vendor_args(launched: &Launched) -> &[String] {
    &launched.container.vendor_args
}

fn literal<'a>(launched: &'a Launched, name: &str) -> Option<&'a str> {
    launched
        .container
        .env_literals
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// CDX-FR-02 / CDX-FR-09 / CDX-FR-04 / CDX-FR-08 / CDX-FR-14 / CDX-FR-OQVA: the
/// 0.172.0 grammar of one launch, given what the dispatch asked for.
fn assert_codex_grammar(launched: &Launched, asked: &Dispatched, resumes: Option<&str>) {
    let argv = vendor_args(launched);
    match resumes {
        Some(thread) => {
            assert_eq!(&argv[..3], ["exec", "resume", "--json"]);
            assert_eq!(argv[argv.len() - 2], thread, "the resume names the thread: {argv:?}");
        }
        None => assert_eq!(&argv[..2], ["exec", "--json"]),
    }
    assert!(argv.iter().any(|a| a == "--dangerously-bypass-approvals-and-sandbox"));
    assert!(argv.iter().any(|a| a == "--skip-git-repo-check"));
    assert_eq!(argv.last().map(String::as_str), Some("-"), "the prompt is stdin: {argv:?}");
    // CDX-FR-27 / CDX-FR-LJBK: never a sandbox flag, a trust flag or a key.
    for never in ["--sandbox", "--full-auto", "--trust", "--yolo"] {
        assert!(!argv.iter().any(|a| a == never), "{never} is never generated: {argv:?}");
    }
    assert!(!argv.iter().any(|a| a.contains("projects.")), "no trust override: {argv:?}");

    let schema = argv.iter().position(|a| a == "--output-schema").map(|at| argv[at + 1].clone());
    let expected_schema = asked
        .result_contract
        .map(|contract| format!("/opt/synthesis/vendor/{}", codex::output_schema_file(contract)));
    assert_eq!(schema, expected_schema, "the contract's own strict schema file");

    let reminder =
        asked.turn_kind.announces_milestones() && asked.instruction.contains("SYNTHESIS_MILESTONE");
    assert_eq!(
        argv.iter().any(|a| a == "--dangerously-bypass-hook-trust"),
        reminder,
        "the reminder rides on a turn that announces milestones: {argv:?}"
    );
    if reminder {
        assert!(argv.iter().any(|a| a == &codex::reminder_hook_override()));
    }
    // CDX-FR-RJPF / EAC-FR-HZIE: the run is ended at the vendor's final event.
    assert!(launched.final_event, "a Codex turn carries its final event rule");
}

// ---------------------------------------------------------------------------
// 55 — a Codex run works, escalates, resumes on the answer and graduates
// ---------------------------------------------------------------------------

// GTE-FR-QVHM, GTE-FR-DKMF / EAC-FR-12, EAC-FR-BOXF, EAC-FR-QHIU, EAC-FR-IGXZ, EAC-FR-RBLI,
// EAC-FR-WBIF, EAC-FR-HZIE, CDX-FR-02, CDX-FR-04, CDX-FR-08, CDX-FR-09, CDX-FR-14,
// CDX-FR-15, CDX-FR-OQVA, CDX-FR-RJPF, CDX-FR-TPFM, CDX-FR-EUBZ, AIC-FR-30, AIC-FR-KAVY,
// GRL-FR-VYNX, GRL-FR-VBCL, GRL-FR-KWPT, GXD-FR-XPUR, GXD-FR-BJYT, GXD-FR-IOZU:
// a run whose project executes Codex in `access_token` mode goes through the
// production executor end to end. The work turn writes milestone markers and
// escalates; the author's answer resumes the same Codex thread under the same
// auth mode; the resumed work turn reports its checks; a review answers ready;
// and the run graduates with exactly what the work turn wrote.
#[test]
fn a_codex_run_escalates_resumes_its_thread_on_the_answer_and_graduates() {
    let fx = codex_fixture(CodexAuthMode::AccessToken);
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ExecutorDispatch::new(
        &fx,
        vec![CodexTurn {
            writes: Vec::new(),
            lines: codex_stream(true, escalation("Which panel is meant?")),
        }],
    );
    let rested = drive(&fx, &run, &dispatch);

    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor, "the work turn asked");
    let asked = rested.escalation.as_ref().expect("the escalation reached the author");
    assert_eq!(asked.origin, GraduationEscalationOrigin::Work);
    assert_eq!(asked.questions[0].question, "Which panel is meant?");

    let launched = dispatch.launched();
    let seen = dispatch.dispatched();
    assert_eq!(launched.len(), 1);
    assert_eq!(seen[0].turn_kind, TurnKind::Work);
    assert_eq!(seen[0].result_contract, Some(ResultContract::WorkResult));
    assert_codex_grammar(&launched[0], &seen[0], None);
    assert!(
        seen[0].instruction.contains("SYNTHESIS_MILESTONE"),
        "GRL-FR-KWPT: a work turn's instruction holds the milestone block"
    );

    // EAC-FR-QHIU / CDX-FR-EUBZ: the token arrives in the environment, by name
    // alone on the vector, and the session state is a dedicated directory.
    let first = &launched[0];
    assert!(first.env_names.iter().any(|n| n == "CODEX_ACCESS_TOKEN"));
    assert!(first.container.env_names.iter().any(|n| n == "CODEX_ACCESS_TOKEN"));
    assert!(!vendor_args(first).iter().any(|a| a.contains("a credential this test never reads")));
    assert_eq!(literal(first, "CODEX_HOME"), Some("/home/agent/.codex"));
    let mounts = &first.container.mounts;
    let home = mounts
        .iter()
        .find(|m| m.target == "/home/agent/.codex")
        .expect("the dedicated session-state mount");
    assert!(!home.read_only);
    assert!(home.source.contains("agent-exec"), "a directory this application owns");
    let vendor = mounts
        .iter()
        .find(|m| m.target == "/opt/synthesis/vendor")
        .expect("the launch files are mounted");
    assert!(vendor.read_only);

    // GXD-FR-IOZU / EAC-FR-HRTK: the markers reached the run as milestones.
    let activity = fx.app.state::<Arc<crate::agent_activity::ActivityStore>>();
    let milestones: Vec<String> = activity
        .read(&run.id, None, None)
        .records
        .into_iter()
        .filter(|r| r.kind == "milestone")
        .map(|r| r.summary)
        .collect();
    assert_eq!(
        milestones,
        [
            r#"{"event":"plan","milestones":["Write the panel","Check it"]}"#,
            r#"{"event":"start","index":1}"#,
            r#"{"event":"done","index":1}"#,
        ]
    );

    // The answer resumes the same thread, and the review graduates the run.
    let answered = answer(&fx, &rested, "The editor's.");
    let dispatch = ExecutorDispatch::new(
        &fx,
        vec![
            CodexTurn {
                writes: vec![("src/panel.ts", "export const panel = 1;\n")],
                lines: codex_stream(false, work_success()),
            },
            CodexTurn { writes: Vec::new(), lines: codex_stream(false, review_ready()) },
        ],
    );
    let finished = drive(&fx, &answered, &dispatch);

    assert_eq!(finished.state, GraduationRunState::Completed, "the run graduated");
    assert_eq!(dispatch.turns_left(), 0, "every scripted turn ran");
    let launched = dispatch.launched();
    let seen = dispatch.dispatched();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].turn_kind, TurnKind::Work);
    // EAC-FR-IGXZ: the recorded session carries the auth mode it ran under.
    assert_eq!(
        seen[0].resume,
        Some((Some(THREAD.to_string()), Some("access_token".to_string()))),
        "the answer continues the turn that asked"
    );
    assert_codex_grammar(&launched[0], &seen[0], Some(THREAD));
    assert_eq!(seen[0].purpose, "\"escalation_answer\"");
    assert!(seen[0].answers.contains("The editor's."), "the answers: {}", seen[0].answers);
    assert_eq!(seen[1].turn_kind, TurnKind::Review);
    assert_eq!(seen[1].result_contract, Some(ResultContract::ReviewVerdict));
    assert_codex_grammar(&launched[1], &seen[1], None);
    // EAC-FR-BOXF: one directory of launch files for every turn of the build.
    let sources: Vec<&str> = launched
        .iter()
        .chain(std::iter::once(first))
        .map(|l| {
            l.container
                .mounts
                .iter()
                .find(|m| m.target == "/opt/synthesis/vendor")
                .map(|m| m.source.as_str())
                .expect("mounted")
        })
        .collect();
    assert!(sources.iter().all(|s| *s == sources[0]), "{sources:?}");

    let committed = super::super::tree_under(&stream.worktree());
    assert_eq!(
        committed.get("src/panel.ts").map(String::as_str),
        Some("export const panel = 1;\n"),
        "the work turn's file is in the stream"
    );
    let repo = git2::Repository::open(stream.worktree()).expect("the stream's repository");
    let head = repo.head().and_then(|h| h.peel_to_commit()).expect("a commit");
    let parent = head.parent(0).expect("the base");
    let diff = repo
        .diff_tree_to_tree(Some(&parent.tree().unwrap()), Some(&head.tree().unwrap()), None)
        .expect("a diff");
    let paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(paths, ["src/panel.ts"], "the run committed exactly what the work wrote");
}

// GTE-FR-QVHM / EAC-FR-IGXZ, EAC-FR-30, AIC-FR-UFYH, GXD-FR-BJYT: when the author
// switches Codex from `login_folder` to `access_token` while a run waits on an
// answer, the turn that takes the answer cannot reach the thread the other
// mode recorded. The executor reports the resume unavailable before any
// container is created, and the loop continues in a fresh thread, with the
// answer still in the task.
#[test]
fn a_codex_auth_mode_switch_between_turns_starts_a_fresh_thread() {
    let fx = codex_fixture(CodexAuthMode::LoginFolder);
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ExecutorDispatch::new(
        &fx,
        vec![CodexTurn {
            writes: Vec::new(),
            lines: codex_stream(false, escalation("Which panel is meant?")),
        }],
    );
    let rested = drive(&fx, &run, &dispatch);
    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
    // CDX-FR-23 / EAC-FR-16: in `login_folder` mode the login directory is
    // the session state, and no token is in the environment.
    let first = &dispatch.launched()[0];
    assert!(!first.env_names.iter().any(|n| n == "CODEX_ACCESS_TOKEN"));
    assert!(first
        .container
        .mounts
        .iter()
        .any(|m| m.target == "/home/agent/.codex" && m.source.ends_with(".codex")));

    // The author switches the machine to an access token.
    fx.allow_execution_for(AgenticRecord {
        vendor: "codex".into(),
        binary_path: Some("/opt/agents/codex".into()),
        path_origin: PathOrigin::Detected,
        codex_auth_mode: CodexAuthMode::AccessToken,
        ..AgenticRecord::default()
    });
    let answered = answer(&fx, &rested, "The editor's.");
    let dispatch = ExecutorDispatch::new(
        &fx,
        vec![
            CodexTurn {
                writes: vec![("src/panel.ts", "export const panel = 1;\n")],
                lines: codex_stream(false, work_success()),
            },
            CodexTurn { writes: Vec::new(), lines: codex_stream(false, review_ready()) },
        ],
    );
    let finished = drive(&fx, &answered, &dispatch);

    let seen = dispatch.dispatched();
    let launched = dispatch.launched();
    assert_eq!(finished.state, GraduationRunState::Completed, "the run still graduated");
    assert_eq!(dispatch.turns_left(), 0, "every scripted turn ran");
    assert_eq!(seen.len(), 3, "the refused resume, the fresh work turn, the review");
    assert_eq!(
        seen[0].resume,
        Some((Some(THREAD.to_string()), Some("login_folder".to_string()))),
        "the answer asked to continue the thread the other mode recorded"
    );
    // GXD-FR-XPUR: the answers go into a fresh work turn, which still carries
    // them, and no container was created for the refused resume.
    assert_eq!(seen[1].turn_kind, TurnKind::Work);
    assert_eq!(seen[1].resume, None, "the fresh turn resumes nothing");
    assert_eq!(seen[1].purpose, "\"escalation_answer\"");
    assert!(seen[1].answers.contains("The editor's."), "the answers: {}", seen[1].answers);
    assert_eq!(launched.len(), 2, "only the fresh work turn and the review ran");
    assert_codex_grammar(&launched[0], &seen[1], None);
    assert_codex_grammar(&launched[1], &seen[2], None);
    for launch in &launched {
        assert!(launch.env_names.iter().any(|n| n == "CODEX_ACCESS_TOKEN"), "the new mode");
        assert_eq!(literal(launch, "CODEX_HOME"), Some("/home/agent/.codex"));
    }
}
