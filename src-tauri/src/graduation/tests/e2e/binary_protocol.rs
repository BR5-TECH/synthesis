//! Journey 10 of the coverage matrix
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Every other journey removes the execution agent at the dispatch seam, which
//! proves what the loop does and nothing about what crosses a process boundary.
//! This one launches the real `agentic-cli-mock` executable
//! (`../../../../../specifications/infra/ACM-agentic-cli-mock.md`) with the
//! **production** task document on its stdin, and reads what comes back with
//! the production protocol types. The mock refuses an invocation that does not
//! match its scenario before it emits a byte, so a run that observes a normal
//! outcome has already proved the bytes were the ones production composed
//! (GTE-FR-DKMF).
//!
//! The container argument vector the executor generates is asserted from the
//! far side of the same boundary by `crate::tools::agent_exec::mock_tests`,
//! which drives the shipped executor through the `DockerRuntime` seam. What is
//! new here is the **graduation** task document itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use std::io::Write;

use crate::graduation::driver::{prompts, task_input};
use crate::graduation::{GraduationRun, ReviewOutcome, ReviewVerdict};
use crate::tools::agent_exec::protocol::{
    AgentOutcome, AgentResponseEnvelope, AgentTaskRequest, Cancellation, ExecutionControls,
    ResultContract, PROTOCOL_VERSION,
};

/// ACM-FR-02: the mock is its own crate with its own `target/`, so
/// `CARGO_BIN_EXE_` cannot reach it. It is built if it is not there yet rather
/// than skipped: a test that quietly does nothing when its double is missing
/// reports success for a run that verified nothing.
fn mock_binary() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the repository root")
        .join("tools/agentic-cli-mock");
    let binary = crate_dir
        .join("target/debug")
        .join(format!("agentic-cli-mock{}", std::env::consts::EXE_SUFFIX));
    if !binary.exists() {
        let built = Command::new(env!("CARGO"))
            .args(["build", "--manifest-path"])
            .arg(crate_dir.join("Cargo.toml"))
            .status()
            .expect("cargo is on PATH");
        assert!(built.success(), "agentic-cli-mock did not build");
    }
    assert!(
        binary.exists(),
        "agentic-cli-mock is not at the path ACM-FR-02 documents: {}",
        binary.display()
    );
    binary
}

/// The review turn's task document, composed by the production path.
fn review_task() -> AgentTaskRequest {
    let mut run = GraduationRun::new_for_test("g1", "d1", "2026-09-06T09:00:00Z");
    run.input.prompt = "Add the empty state to the panel.".into();
    run.input.draft_name = "editor draft".into();
    run.stream_name = "editor".into();
    run.checkpoint.changed_paths = vec!["src/panel.ts".into()];
    let input = task_input::compose_input(
        &run,
        crate::graduation::driver::phases::PART_REVIEW,
        task_input::PURPOSE_GENERATE,
    );
    AgentTaskRequest {
        protocol_version: PROTOCOL_VERSION,
        instruction: prompts::instruction_for(crate::graduation::driver::phases::PART_REVIEW)
            .to_string(),
        input: Some(input.to_map()),
        result_contract: Some(ResultContract::ReviewVerdict),
        resume: None,
        execution: ExecutionControls {
            cancellation: Cancellation::CallerControlled,
            timeout_ms: 60_000,
        },
    }
}

/// What a scripted review answers with, in the protocol's own shape.
fn ready_envelope() -> String {
    serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "outcome": "success",
        "summary": "the review ran",
        "result": {
            "verdict": "ready",
            "rationale": "The work answers the prompt.",
            "findings": []
        }
    })
    .to_string()
}

/// One run of the mock: its own scenario file, the argument vector after `--`,
/// and the bytes written to its stdin.
struct MockRun {
    _directory: tempfile::TempDir,
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_mock(document: serde_json::Value, argv: &[&str], stdin: &[u8]) -> MockRun {
    let directory = tempfile::tempdir().expect("a scenario directory");
    let scenario = directory.path().join("scenario.json");
    // Through the guarded handle rather than `std::fs`, on the terms FSA-FR-19
    // binds every other write in the backend. A scratch directory is exactly
    // what a root is for.
    crate::fs::FsAccess::builder()
        .allow_root(directory.path())
        .build()
        .expect("a real directory")
        .write_text_atomic(
            &scenario,
            &serde_json::to_string_pretty(&document).expect("a scenario"),
        )
        .expect("the scenario is writable");

    let mut child = Command::new(mock_binary())
        .arg("--tool")
        .arg("claude")
        .arg("--scenario")
        .arg(&scenario)
        .arg("--")
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the mock launches");
    // ACM-FR-07: the mock reads stdin to EOF before it does anything else, so
    // the stream is always written and always closed.
    child
        .stdin
        .take()
        .expect("a stdin pipe")
        .write_all(stdin)
        .expect("the payload is written");
    let output = child.wait_with_output().expect("the mock exits");
    MockRun {
        _directory: directory,
        status: output.status.code().expect("an exit code"),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// GTE-FR-DKMF, GTE-FR-ZPNC / ACM-FR-12, ACM-FR-20, GXD-FR-ODGX, GRL-FR-VIAT: the mock
// replays a valid protocol response for the graduation task document
// production composes, and the production types read it back.
#[test]
fn the_mock_replays_a_valid_verdict_for_the_task_document_production_composes() {
    let task = review_task();
    let stdin = String::from_utf8(task.to_stdin_bytes().expect("within limits")).expect("utf8");

    let run = run_mock(
        serde_json::json!({
            "version": 1,
            "expected": {
                // The whole stdin stream, byte for byte. The adapter's own
                // token is required beside it; the complete ordered container
                // vector is asserted by `agent_exec::mock_tests`, which is
                // where the vector is generated.
                "required_args": [{ "name": "headless", "value": "-p" }],
                "stdin": { "encoding": "utf8", "value": stdin },
            },
            "response": {
                "stdout": ready_envelope(),
                "stderr": "",
                "exit_code": 0,
            },
        }),
        &["-p", "review"],
        stdin.as_bytes(),
    );

    assert_eq!(run.status, 0, "the mock accepted the invocation: {}", run.stderr);

    // Read back with the production envelope type and the production verdict
    // type, rather than by looking for a string.
    let envelope: AgentResponseEnvelope =
        serde_json::from_str(run.stdout.trim()).expect("the replayed bytes are an envelope");
    assert_eq!(envelope.protocol_version, PROTOCOL_VERSION);
    assert_eq!(envelope.outcome, AgentOutcome::Success);
    let verdict: ReviewVerdict = serde_json::from_value(serde_json::Value::Object(
        envelope.result.expect("a result object"),
    ))
    .expect("the result is a review verdict");
    assert_eq!(verdict.verdict, ReviewOutcome::Ready);
    assert!(verdict.findings.is_empty(), "a ready verdict carries no finding");
    assert!(!verdict.rationale.trim().is_empty(), "a verdict states why");
}

// GTE-FR-DKMF, GTE-FR-RSQD / ACM-FR-18, ACM-FR-19, ACM-FR-23, ACM-FR-26: an argument vector
// the adapter does not accept is an expectation mismatch. The configured
// response is suppressed entirely, so nothing downstream could mistake the run
// for one that answered.
#[test]
fn the_mock_refuses_an_argument_vector_the_adapter_does_not_accept() {
    let run = run_mock(
        serde_json::json!({
            "version": 1,
            "expected": {},
            "response": { "stdout": ready_envelope(), "stderr": "", "exit_code": 0 },
        }),
        // No `-p`, which the `claude` adapter requires.
        &["review"],
        b"",
    );

    assert_eq!(run.status, 65, "an expectation mismatch");
    assert!(run.stdout.is_empty(), "no configured byte is written");
    let record: serde_json::Value =
        serde_json::from_str(run.stderr.trim()).expect("exactly one failure record");
    assert_eq!(record["kind"], "expectation_mismatch");
    assert_eq!(record["code"], 65);
    assert_eq!(record["tool"], "claude");
    assert_eq!(record["check"], "adapter");
    assert_eq!(record["mismatch_type"], "missing_headless_argument");
    // ACM-FR-24: a mismatch is described by location, length and category
    // alone, so nothing the scenario held is disclosed.
    assert!(record.get("expected").is_none());
    assert!(record.get("actual").is_none());
}

// GTE-FR-DKMF, GTE-FR-ZPNC / ACM-FR-12, ACM-FR-17, ACM-FR-25: stdin that is not the payload
// the scenario demands is an expectation mismatch, so a caller that sent a
// different task document could not pass by accident.
#[test]
fn the_mock_refuses_stdin_that_is_not_the_payload_it_was_given() {
    let task = review_task();
    let stdin = String::from_utf8(task.to_stdin_bytes().expect("within limits")).expect("utf8");

    let run = run_mock(
        serde_json::json!({
            "version": 1,
            "expected": {
                "required_args": [{ "value": "-p" }],
                "stdin": { "encoding": "utf8", "value": stdin },
            },
            "response": { "stdout": ready_envelope(), "stderr": "", "exit_code": 0 },
        }),
        &["-p", "review"],
        b"{\"task\":\"something else entirely\"}",
    );

    assert_eq!(run.status, 65, "an expectation mismatch");
    assert!(run.stdout.is_empty(), "no configured byte is written");
    let record: serde_json::Value =
        serde_json::from_str(run.stderr.trim()).expect("exactly one failure record");
    assert_eq!(record["kind"], "expectation_mismatch");
    assert_eq!(record["check"], "stdin");
    assert_eq!(record["mismatch_type"], "length");
}

// GTE-FR-DKMF, GTE-FR-JYWB / ACM-FR-03, ACM-FR-05, ACM-FR-27: an invocation the mock cannot
// configure itself from is a configuration error, told apart from every
// simulated tool failure by its own exit code.
#[test]
fn the_mock_refuses_an_invocation_it_cannot_configure_itself_from() {
    let missing_scenario = Command::new(mock_binary())
        .args(["--tool", "claude", "--", "-p", "review"])
        .stdin(Stdio::null())
        .output()
        .expect("the mock runs");
    assert_eq!(missing_scenario.status.code(), Some(64));
    let record: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&missing_scenario.stderr).trim())
            .expect("exactly one failure record");
    assert_eq!(record["kind"], "configuration_error");
    assert_eq!(record["mismatch_type"], "missing_scenario_option");

    let unknown_tool = Command::new(mock_binary())
        .args(["--tool", "nothing", "--scenario", "/nowhere.json", "--"])
        .stdin(Stdio::null())
        .output()
        .expect("the mock runs");
    assert_eq!(unknown_tool.status.code(), Some(64));
    let record: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&unknown_tool.stderr).trim())
            .expect("exactly one failure record");
    assert_eq!(record["mismatch_type"], "unknown_tool");
    // ACM-FR-05: no adapter was selected, so the record names no tool.
    assert!(record.get("tool").is_none());
}
