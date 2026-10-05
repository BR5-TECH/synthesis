//! The shared doubles, fixtures, scripted stdout, and run helpers every
//! topic file in this suite uses.

use super::*;

/// The placeholder a scripted stdout uses where the real CLI would echo back the
/// session it ran in.
pub(super) const SESSION_PLACEHOLDER: &str = "{{SESSION_ID}}";

/// Stand in for the pinned CLI reporting its own session identity.
///
/// Claude Code is told which session to use with `--session-id` and reports that
/// same value back in its result document (CCP-FR-16), so a double that replayed
/// a fixed string would be asserting against a session no real run could
/// produce. Substituting the value out of the argv is what makes the double
/// behave like the CLI rather than like a recording of one.
pub(super) fn substitute_session(stdout: &str, argv: &[String]) -> String {
    if !stdout.contains(SESSION_PLACEHOLDER) {
        return stdout.to_string();
    }
    let assigned = assigned_session_id(argv).unwrap_or_default();
    stdout.replace(SESSION_PLACEHOLDER, &assigned)
}

/// The value of `--session-id` in a generated vector, if it carries one.
pub(super) fn assigned_session_id(argv: &[String]) -> Option<String> {
    argv.iter()
        .position(|a| a == "--session-id")
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

/// The `did not complete` record this launch emitted, found by the container it
/// names (EAC-FR-29).
///
/// The log buffer is process-global and every other test in this file writes to
/// it, so a scan of the whole rendering proves nothing about *this* launch — a
/// field another test wrote satisfies it just as well. The container name is
/// unique per launch (EAC-FR-13), which makes it the one handle that attributes
/// a record.
pub(super) fn failure_record(container: &str) -> crate::logging::LogRecord {
    use crate::logging::LogFilter;

    let page = super::super::log_buffer()
        .query(&LogFilter::default(), None, 20_000)
        .expect("query");
    let mut found: Vec<_> = page
        .records
        .into_iter()
        .filter(|record| {
            record.message == "agent execution did not complete"
                && record.fields.get("container").and_then(|v| v.as_str()) == Some(container)
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one failure record for {container}"
    );
    found.remove(0)
}

/// The container name this invocation carries (EAC-FR-13).
///
/// Read by the flag that introduces it rather than by position, so a test that
/// cares which container ran is not also asserting where in the vector the
/// runtime flags happen to sit — that is
/// `claude_codes_generated_invocation_is_exactly_the_descriptor`'s job, and one
/// place is enough.
pub(super) fn container_name(argv: &[String]) -> String {
    argv.iter()
        .position(|a| a == "--name")
        .and_then(|i| argv.get(i + 1))
        .cloned()
        .expect("every invocation names its container")
}

/// A dispatched task document, read back as its two halves (EAC-FR-35).
///
/// The caller's `AgentTaskRequest` is decoded on its own terms, `deny_unknown_fields`
/// included, so a stray key in the part the caller supplied still fails here —
/// only the one field the executor adds is lifted off first.
pub(super) fn split_task_document(bytes: &[u8]) -> (AgentTaskRequest, String) {
    let (task, contract, _) = split_whole_task_document(bytes);
    (task, contract)
}

/// The same split, keeping the result schema the executor supplied (EAC-FR-43).
///
/// `None` where the caller named no result contract, which is what a document
/// that states no shape for its own answer looks like.
pub(super) fn split_whole_task_document(
    bytes: &[u8],
) -> (AgentTaskRequest, String, Option<serde_json::Value>) {
    let mut document: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(bytes).expect("the task document is a JSON object");
    let contract = document
        .remove("response_contract")
        .expect("every dispatched document carries the response contract (EAC-FR-35)");
    let contract = contract.as_str().expect("the contract is text").to_string();
    let schema = document.remove("result_schema");
    let task = serde_json::from_value(serde_json::Value::Object(document))
        .expect("the rest of the document is the caller's task, unaltered");
    (task, contract, schema)
}

pub(super) fn task(instruction: &str) -> AgentTaskRequest {
    AgentTaskRequest {
        protocol_version: 1,
        instruction: instruction.to_string(),
        input: None,
        result_contract: None,
        resume: None,
        execution: ExecutionControls {
            timeout_ms: 60_000,
            cancellation: Cancellation::CallerControlled,
        },
    }
}

/// The English for a small count, so a contract written for a reader satisfies
/// a bound stated as a number.
pub(super) fn spelled(number: &str) -> String {
    match number {
        "1" => "one",
        "2" => "two",
        "3" => "three",
        "5" => "five",
        "8" => "eight",
        "12" => "twelve",
        other => other,
    }
    .to_string()
}

pub(super) fn envelope_json(outcome: &str) -> String {
    let mut value = json!({
        "protocol_version": 1,
        "outcome": outcome,
        "summary": "did the thing",
        "result": null,
        "failure": null,
        "escalation": null,
        "session": { "session_id": "sess-1", "continuation_token": null },
        "metadata": null
    });
    match outcome {
        "failure" => {
            value["failure"] = json!({ "code": "blocked", "message": "cannot", "retryable": true })
        }
        "escalation_required" => {
            value["escalation"] = json!({
                "reason": "ambiguous",
                "questions": [{
                    "question": "which one?",
                    "options": [
                        {"answer": "a", "summary": "Take a", "description": "The first path."},
                        {"answer": "b", "summary": "Take b", "description": "The second path."}
                    ]
                }]
            })
        }
        _ => {}
    }
    value.to_string()
}

/// Claude Code's result event (CCP contract surface): the last line of the
/// stream, whose `result` is the text the agent produced.
///
/// Uses the text position rather than `structured_output`, so the fallback of
/// CCP-FR-09 is what the bulk of the suite exercises; `claude_stdout_structured`
/// covers the primary position.
pub(super) fn claude_result_line(envelope: &str) -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": envelope,
        "session_id": SESSION_PLACEHOLDER
    })
    .to_string()
}

/// The whole stream a turn writes (CCP-FR-25): progress events as the agent
/// works, and the result event last.
///
/// The bulk of the suite runs against this rather than against the result event
/// alone, because a stream is what the pinned CLI actually emits — an extractor
/// asserted only against a lone document would pass while being unable to read
/// a single real run.
pub(super) fn claude_stdout(envelope: &str) -> String {
    format!(
        "{}\n{}\n{}\n",
        json!({
            "type": "system",
            "subtype": "init",
            "session_id": SESSION_PLACEHOLDER,
            "model": "claude-sonnet-5",
            "tools": ["Read", "Write", "Bash"]
        }),
        json!({
            "type": "assistant",
            "session_id": SESSION_PLACEHOLDER,
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": "working on it" }]
            }
        }),
        claude_result_line(envelope),
    )
}

/// The same stream carrying the envelope at CCP-FR-09's *first* position.
pub(super) fn claude_stdout_structured(envelope: &str) -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "structured_output": serde_json::from_str::<serde_json::Value>(envelope).unwrap(),
        "result": envelope,
        "session_id": SESSION_PLACEHOLDER
    })
    .to_string()
}

pub(super) fn valid_claude_stdout() -> String {
    claude_stdout(&envelope_json("success"))
}

/// Codex's event stream (CDX contract surface): JSONL, the thread id on
/// `thread.started` and the envelope in the last completed `agent_message` item.
pub(super) fn codex_stdout(envelope: &str) -> String {
    format!(
        "{}\n{}\n{}\n{}\n",
        json!({ "type": "thread.started", "thread_id": CODEX_THREAD_ID }),
        json!({ "type": "turn.started" }),
        json!({
            "type": "item.completed",
            "item": { "id": "item_1", "type": "agent_message", "text": envelope }
        }),
        json!({
            "type": "turn.completed",
            "usage": { "input_tokens": 2, "cached_input_tokens": 0, "output_tokens": 4 }
        })
    )
}

/// The thread id Codex reports. Unlike Claude Code's, it is the vendor's own —
/// this CLI has no argument that supplies one for a new session (CDX-FR-20).
pub(super) const CODEX_THREAD_ID: &str = "019ce7d8-045d-7fe1-a9e3-47c6d90a73e1";

pub(super) fn run(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    task: AgentTaskRequest,
) -> Result<AgentExecution, AgentExecutionError> {
    run_with_cancel(harness, runtime, task, CancellationToken::new())
}

pub(super) fn run_with_cancel(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    task: AgentTaskRequest,
    cancellation: CancellationToken,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(runtime)
        .with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task,
            cancellation,
            activity: None,
            supplementary_mount: None,
        },
    ))
}

/// The stdout a scripted run of `vendor` replies with.
pub(super) fn stdout_for(vendor: &str, envelope: &str) -> String {
    match vendor {
        "codex" => codex_stdout(envelope),
        _ => claude_stdout(envelope),
    }
}

/// A bundle of the shape `GRB-graduation-rebase.md` generates.
/// A bundle shaped the way the producer emits one: three mirrors of one tree,
/// each keyed by the project path its material belongs to, and no document
/// (per `../core/GRB-graduation-rebase.md` GRB-FR-YPEX).
pub(super) fn rebase_bundle(root: &std::path::Path, name: &str) -> PathBuf {
    let bundle = root.join(name);
    for (mirror, body) in [
        ("source", &b"--- a/a.md\n+++ b/a.md\n"[..]),
        ("run", &b"--- a/a.md\n+++ b/a.md\n"[..]),
        ("merged", &b"merged\n"[..]),
    ] {
        std::fs::create_dir_all(bundle.join(mirror)).expect("bundle");
        std::fs::write(bundle.join(mirror).join("a.md"), body).expect("material");
    }
    bundle
}

/// [`run_with_mount`] standing somewhere other than the harness's own
/// workspace, so two executions of one harness can reach two working copies.
pub(super) fn run_in(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    execution_directory: PathBuf,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor =
        AgentCliExecutor::new(runtime).with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory,
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
}

pub(super) fn run_with_mount(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    mount: Option<SupplementaryMount>,
    activity: Option<Arc<dyn AgentActivitySink>>,
) -> Result<AgentExecution, AgentExecutionError> {
    run_as(harness, runtime, mount, activity, super::super::TurnKind::Work)
}

/// [`run_with_mount`] with the turn kind named, so EAC-FR-41's two masking
/// conditions can be exercised apart from one another.
pub(super) fn run_as(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    mount: Option<SupplementaryMount>,
    activity: Option<Arc<dyn AgentActivitySink>>,
    turn_kind: super::super::TurnKind,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor =
        AgentCliExecutor::new(runtime).with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity,
            supplementary_mount: mount,
        },
    ))
}

/// A linked worktree of a fresh primary checkout, which is the shape every
/// graduation run is given (`Repository::worktree`).
///
/// Returns the primary repository's Git directory and the worktree's own, which
/// are the two sources EAC-FR-FNFV mounts.
/// EAC-FR-ZKMR: the repository mounts one shape composes, asked directly.
///
/// The shape is a property of the execution directory's path, decided before
/// any container is composed — and the fallback shape is unreachable end to end
/// on a machine whose paths are all container paths, every POSIX one being able
/// to be a mount target. So the shape is named here rather than arranged for.
pub(super) fn repository_mounts_in(
    harness: &Harness,
    directory: &Path,
    aligned: bool,
) -> Option<Vec<BindMount>> {
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""))
        .with_session_state_root(harness.sessions_root());
    let workspace = if aligned {
        crate::changes::canonicalize_lenient(directory)
            .to_string_lossy()
            .into_owned()
    } else {
        descriptor::WORKSPACE_TARGET.to_string()
    };
    executor
        .repository_mounts(&harness.fs, directory, &workspace, aligned)
        .map(|(mounts, _, _)| mounts)
}

/// The target of one mount in a composed set, for the shape assertions.
pub(super) fn target_of<'a>(mounts: &'a [BindMount], contains: &str) -> &'a BindMount {
    mounts
        .iter()
        .find(|m| m.target.contains(contains))
        .unwrap_or_else(|| panic!("no mount whose target holds {contains}"))
}

pub(super) fn linked_worktree_at(root: &Path, worktree: &Path) -> (PathBuf, PathBuf) {
    linked_worktree_named(root, worktree, "run-1")
}

/// [`linked_worktree_at`] under a registration name of the caller's choosing, so
/// two fixtures can be told apart by what a generated pointer names.
pub(super) fn linked_worktree_named(root: &Path, worktree: &Path, name: &str) -> (PathBuf, PathBuf) {
    let primary = root.join("primary");
    let repo = git2::Repository::init(&primary).expect("a primary checkout");
    // A worktree needs a commit to point at.
    {
        let mut index = repo.index().unwrap();
        let tree = index.write_tree().unwrap();
        let tree = repo.find_tree(tree).unwrap();
        let who = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &who, &who, "base", &tree, &[]).unwrap();
    }
    // `worktree` already exists as the harness's workspace, and libgit2 refuses
    // a path that is already there — so the worktree is added beside it and its
    // contents moved into place.
    let staged = root.join("staged-worktree");
    repo.worktree(
        name,
        &staged,
        Some(git2::WorktreeAddOptions::new().reference(None)),
    )
    .expect("a linked worktree");
    std::fs::rename(staged.join(".git"), worktree.join(".git")).unwrap();
    // Canonical, which is what libgit2 records and what EAC-FR-ZKMR mounts the
    // store at. A raw spelling here would let an aligned turn assert over a
    // pointer naming a path its store mount does not carry.
    let git_dir = crate::changes::canonicalize_lenient(&primary)
        .join(".git/worktrees")
        .join(name);
    // The recorded gitdir names where the worktree now stands.
    std::fs::write(
        git_dir.join("gitdir"),
        format!("{}\n", worktree.join(".git").display()).as_bytes(),
    )
    .unwrap();
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", git_dir.display()).as_bytes(),
    )
    .unwrap();
    (primary.join(".git"), git_dir)
}

/// Copy a directory tree, for materialising what a container's mounts show.
pub(super) fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A sink that keeps everything it is handed, in order.
#[derive(Default)]
pub(super) struct CollectedActivity(StdMutex<Vec<AgentActivityEvent>>);

impl AgentActivitySink for CollectedActivity {
    fn activity(&self, event: AgentActivityEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl CollectedActivity {
    pub(super) fn events(&self) -> Vec<AgentActivityEvent> {
        self.0.lock().unwrap().clone()
    }

    pub(super) fn kinds(&self) -> Vec<String> {
        self.events().into_iter().map(|e| e.kind.to_string()).collect()
    }

    /// Everything the sink was handed, as one text, for a leak assertion.
    pub(super) fn rendered(&self) -> String {
        self.events()
            .iter()
            .map(|e| format!("{} {} {}", e.channel, e.summary, e.payload))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The same launch as `run`, with somebody watching it.
pub(super) fn run_watching(
    harness: &Harness,
    runtime: Arc<RecordingRuntime>,
    task: AgentTaskRequest,
    sink: Arc<CollectedActivity>,
) -> Result<AgentExecution, AgentExecutionError> {
    let executor = AgentCliExecutor::new(runtime)
        .with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task,
            cancellation: CancellationToken::new(),
            activity: Some(sink),
            supplementary_mount: None,
        },
    ))
}
