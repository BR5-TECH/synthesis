//! Executable-level tests: every one of these launches the real binary.
//!
//! They cover what the in-process tests structurally cannot — a real argument
//! vector delivered by the operating system, real pipes, a real exit status,
//! real elapsed time during a delay, and a real process that a caller can
//! terminate mid-flight.

use std::io::Write;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// Cargo builds the binary for the integration-test target and hands us its
/// path, which is also the path ACM-FR-02 documents.
const MOCK: &str = env!("CARGO_BIN_EXE_agentic-cli-mock");

struct Fixture {
    _directory: tempfile::TempDir,
    scenario: std::path::PathBuf,
}

impl Fixture {
    fn new(scenario_text: &str) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let scenario = directory.path().join("scenario.json");
        std::fs::write(&scenario, scenario_text).expect("write scenario");
        Fixture {
            _directory: directory,
            scenario,
        }
    }

    fn command(&self, tool: &str, tool_args: &[&str]) -> Command {
        let mut command = Command::new(MOCK);
        command.arg("--tool").arg(tool);
        command.arg("--scenario").arg(&self.scenario);
        if !tool_args.is_empty() {
            command.arg("--");
            command.args(tool_args);
        }
        command
    }

    /// Run with stdin closed immediately. `Command::output` attaches a null
    /// stdin, which the mock sees as instant EOF, and drains both pipes
    /// concurrently so a large payload on one cannot deadlock the other.
    fn run(&self, tool: &str, tool_args: &[&str]) -> Output {
        self.command(tool, tool_args)
            .output()
            .expect("launch the mock")
    }

    /// Run with bytes written to stdin, then closed.
    fn run_with_stdin(&self, tool: &str, tool_args: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .command(tool, tool_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch the mock");

        write_stdin_and_close(&mut child, input);
        child.wait_with_output().expect("collect output")
    }
}

fn write_stdin_and_close(child: &mut Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // A large payload can exceed the pipe buffer, so the write has to happen off
    // the thread that will later wait on the child.
    let payload = input.to_vec();
    std::thread::spawn(move || {
        let _ = stdin.write_all(&payload);
        // Dropping the handle closes the pipe, which is the EOF the mock waits
        // for (ACM-FR-07).
    });
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("the mock exits, never signals")
}

/// The failure record, parsed — and asserted to be exactly one object followed
/// by exactly one newline (ACM-FR-23).
fn failure_record(output: &Output) -> Value {
    let text = String::from_utf8(output.stderr.clone()).expect("record is utf8");
    assert!(text.ends_with('\n'), "record must end with one newline");
    assert_eq!(text.matches('\n').count(), 1, "record must be one line");
    serde_json::from_str(text.trim_end()).expect("record is one JSON object")
}

const OK_CLAUDE: &str = r#"{
    "version": 1,
    "expected": {"args": ["-p", "summarise the diff"], "stdin": null},
    "response": {
        "stdout": {"encoding": "utf8", "value": "{\"result\":\"ok\"}\n"},
        "stderr": {"encoding": "utf8", "value": ""},
        "exit_code": 0,
        "delay_ms": 0
    }
}"#;

/// ACM-FR-04, ACM-FR-10, ACM-FR-18, ACM-FR-20
#[test]
fn a_claude_invocation_emits_its_configured_structured_json() {
    let fixture = Fixture::new(OK_CLAUDE);
    let output = fixture.run("claude", &["-p", "summarise the diff"]);

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"{\"result\":\"ok\"}\n");
    assert!(output.stderr.is_empty());
}

/// ACM-FR-18, ACM-FR-20, ACM-FR-22
#[test]
fn a_codex_invocation_emits_both_configured_streams() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["exec", "run the suite"]},
            "response": {
                "stdout": "suite passed\n",
                "stderr": "warning: 2 tests skipped\n",
                "exit_code": 0
            }
        }"#,
    );
    let output = fixture.run("codex", &["exec", "run the suite"]);

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"suite passed\n");
    assert_eq!(output.stderr, b"warning: 2 tests skipped\n");
}

/// ACM-FR-18, ACM-FR-26, ACM-FR-27
#[test]
fn a_wrong_headless_argument_never_emits_a_successful_response() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["--print", "summarise the diff"]},
            "response": {"stdout": "SHOULD NOT APPEAR", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("claude", &["--print", "summarise the diff"]);

    assert_eq!(code(&output), 65);
    assert!(output.stdout.is_empty());
    let record = failure_record(&output);
    assert_eq!(record["kind"], "expectation_mismatch");
    assert_eq!(record["tool"], "claude");
    assert_eq!(record["check"], "adapter");
    assert_eq!(record["mismatch_type"], "missing_headless_argument");

    // The same vector fails for Codex too, for the absence of `exec`.
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["--print", "x"]},
            "response": {"stdout": "SHOULD NOT APPEAR", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("codex", &["--print", "x"]);
    assert_eq!(code(&output), 65);
    assert!(output.stdout.is_empty());
    assert_eq!(failure_record(&output)["check"], "adapter");
}

/// ACM-FR-24, ACM-FR-25 — the sensitive-value case.
#[test]
fn an_argument_mismatch_locates_without_disclosing() {
    let secret = "sk-ant-oat01-REDACTEDVALUE";
    let fixture = Fixture::new(&format!(
        r#"{{
            "version": 1,
            "expected": {{"args": ["-p", "{secret}"]}},
            "response": {{"stdout": "o", "stderr": "", "exit_code": 0}}
        }}"#
    ));
    let output = fixture.run("claude", &["-p", "wrong"]);

    assert_eq!(code(&output), 65);
    assert!(output.stdout.is_empty());

    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(!stderr.contains(secret), "leaked the expected value");
    assert!(!stderr.contains("wrong"), "leaked the received value");
    assert!(!stderr.contains("scenario.json"), "leaked the path");
    assert!(!stderr.contains('/'), "leaked part of a path");

    let record = failure_record(&output);
    assert_eq!(record["check"], "args");
    assert_eq!(record["index"], 1);
    assert_eq!(record["expected_length"], 26);
    assert_eq!(record["actual_length"], 5);
    assert_eq!(record["mismatch_type"], "value");
}

/// ACM-FR-13, ACM-FR-20, ACM-FR-27
#[test]
fn a_configured_non_zero_exit_code_reaches_the_caller_unchanged() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"]},
            "response": {"stdout": "partial output\n", "stderr": "boom\n", "exit_code": 42}
        }"#,
    );
    let output = fixture.run("claude", &["-p"]);

    assert_eq!(code(&output), 42);
    assert_eq!(output.stdout, b"partial output\n");
    assert_eq!(output.stderr, b"boom\n");
}

/// ACM-FR-13, ACM-FR-20, ACM-FR-27 — the boundaries of the portable range.
#[test]
fn the_extremes_of_the_portable_range_survive_the_process_boundary() {
    for configured in [0u8, 1, 63, 66, 254, 255] {
        let fixture = Fixture::new(&format!(
            r#"{{"version":1,"expected":{{}},"response":{{"stdout":"x","stderr":"","exit_code":{configured}}}}}"#
        ));
        let output = fixture.run("claude", &["-p"]);
        assert_eq!(code(&output), i32::from(configured));
        assert_eq!(output.stdout, b"x");
    }
}

/// ACM-FR-21, ACM-FR-27 — the mock stays alive through the delay and can be terminated.
#[test]
fn a_configured_delay_keeps_the_process_alive_and_silent() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"]},
            "response": {"stdout": "TOO LATE", "stderr": "", "exit_code": 0, "delay_ms": 5000}
        }"#,
    );

    let mut child = fixture
        .command("claude", &["-p"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch the mock");

    // Poll for the caller's own 500 ms budget; the process must still be
    // running when it expires.
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut exited_early = false;
    while Instant::now() < deadline {
        if child.try_wait().expect("poll").is_some() {
            exited_early = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!exited_early, "the mock must outlive the caller's timeout");

    // The caller terminates it and classifies the run from its own termination
    // status — the mock produced no classification of its own (ACM-FR-27).
    child.kill().expect("terminate");
    let output = child.wait_with_output().expect("collect");

    assert!(output.stdout.is_empty(), "no response before the delay ended");
    assert!(output.stderr.is_empty());

    // The mock contributed no exit code of its own: it was killed, which on
    // Unix means no code at all and a signal instead. Asserting only
    // `code() != Some(0)` would pass for a process that had exited normally
    // with 1, which is the outcome this test exists to rule out.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            None,
            "a signalled process reports no exit code"
        );
        assert_eq!(output.status.signal(), Some(libc_sigkill()));
    }
    #[cfg(not(unix))]
    assert_ne!(output.status.code(), Some(0));
}

#[cfg(unix)]
fn libc_sigkill() -> i32 {
    // SIGKILL is 9 on every Unix this project builds for, and naming it here
    // avoids a `libc` dependency in a crate specified to have three.
    9
}

/// ACM-FR-21, ACM-FR-27 — and left alone, the response arrives only after the delay.
#[test]
fn without_a_timeout_the_response_arrives_after_the_delay_elapses() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"]},
            "response": {"stdout": "late\n", "stderr": "", "exit_code": 0, "delay_ms": 300}
        }"#,
    );

    let started = Instant::now();
    let output = fixture.run("claude", &["-p"]);
    let elapsed = started.elapsed();

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"late\n");
    assert!(
        elapsed >= Duration::from_millis(300),
        "returned after {elapsed:?}, before the configured delay"
    );
}

/// ACM-FR-08, ACM-FR-15, ACM-FR-16, ACM-FR-23, ACM-FR-27
#[test]
fn malformed_duplicate_and_invalid_scenarios_produce_deterministic_errors() {
    let cases = [
        ("{ not json", "malformed_json"),
        (
            r#"{"version":1,"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            "duplicate_key",
        ),
        (
            r#"{"version":2,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            "bad_version",
        ),
        (
            r#"{"version":1,"timeout_ms":5,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            "unknown_field",
        ),
        (
            r#"{"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":64}}"#,
            "bad_exit_code",
        ),
        (
            r#"{"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0,"delay_ms":10001}}"#,
            "bad_delay",
        ),
        (
            r#"{"version":1,"expected":{"stdin":{"encoding":"hex","value":"ff"}},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
            "bad_encoding",
        ),
    ];

    for (text, expected) in cases {
        let fixture = Fixture::new(text);
        let output = fixture.run("claude", &["-p"]);

        assert_eq!(code(&output), 64, "for {expected}");
        assert!(output.stdout.is_empty(), "for {expected}");
        let record = failure_record(&output);
        assert_eq!(record["kind"], "configuration_error");
        assert_eq!(record["code"], 64);
        assert_eq!(record["tool"], "claude");
        assert_eq!(record["check"], "scenario");
        assert_eq!(record["mismatch_type"], expected);
    }
}

/// ACM-FR-08, ACM-FR-15, ACM-FR-16, ACM-FR-23, ACM-FR-27 — an unreadable scenario, and no path in the record.
#[test]
fn an_unreadable_scenario_is_a_configuration_error_without_the_path() {
    let output = Command::new(MOCK)
        .args([
            "--tool",
            "claude",
            "--scenario",
            "/nonexistent/secret-dir/scenario.json",
            "--",
            "-p",
        ])
        .output()
        .expect("launch");

    assert_eq!(code(&output), 64);
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(!stderr.contains("secret-dir"));
    assert!(!stderr.contains('/'));
    assert_eq!(failure_record(&output)["mismatch_type"], "unreadable");
}

/// ACM-FR-09, ACM-FR-20 — exact byte preservation across the process boundary.
#[test]
fn payloads_survive_byte_for_byte_in_every_encoding() {
    // A bare JSON string with a trailing newline, multi-byte UTF-8, base64
    // bytes that are not valid UTF-8, and an empty payload.
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"]},
            "response": {
                "stdout": {"encoding": "base64", "value": "//7hIA=="},
                "stderr": "héllo — wörld\n\n",
                "exit_code": 0
            }
        }"#,
    );
    let output = fixture.run("claude", &["-p"]);

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, vec![0xff, 0xfe, 0xe1, 0x20]);
    assert_eq!(output.stderr, "héllo — wörld\n\n".as_bytes());

    // An empty payload writes nothing at all — not a blank line.
    let fixture = Fixture::new(
        r#"{"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
    );
    let output = fixture.run("claude", &["-p"]);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());

    // A payload with no trailing newline gets none added.
    let fixture = Fixture::new(
        r#"{"version":1,"expected":{},"response":{"stdout":"no newline here","stderr":"","exit_code":0}}"#,
    );
    let output = fixture.run("claude", &["-p"]);
    assert_eq!(output.stdout, b"no newline here");

    // The explicit `utf8` object form, carrying multi-byte characters and a
    // trailing newline, on stdout — and an explicitly empty `utf8` object on
    // stderr, which must still write nothing.
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {},
            "response": {
                "stdout": {"encoding": "utf8", "value": "héllo — wörld 🌍\n"},
                "stderr": {"encoding": "utf8", "value": ""},
                "exit_code": 0
            }
        }"#,
    );
    let output = fixture.run("claude", &["-p"]);
    assert_eq!(output.stdout, "héllo — wörld 🌍\n".as_bytes());
    assert!(output.stderr.is_empty());
}

/// ACM-FR-09, ACM-FR-20 — and stdin bytes are compared exactly.
#[test]
fn stdin_payloads_are_compared_byte_for_byte() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"], "stdin": {"encoding": "base64", "value": "AAH/"}},
            "response": {"stdout": "matched", "stderr": "", "exit_code": 0}
        }"#,
    );

    let output = fixture.run_with_stdin("claude", &["-p"], &[0x00, 0x01, 0xff]);
    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"matched");

    // One byte different, same length: a content mismatch.
    let output = fixture.run_with_stdin("claude", &["-p"], &[0x00, 0x01, 0xfe]);
    assert_eq!(code(&output), 65);
    let record = failure_record(&output);
    assert_eq!(record["check"], "stdin");
    assert_eq!(record["mismatch_type"], "content");
    assert_eq!(record["expected_length"], 3);
    assert_eq!(record["actual_length"], 3);

    // A trailing newline is a byte like any other.
    let output = fixture.run_with_stdin("claude", &["-p"], &[0x00, 0x01, 0xff, b'\n']);
    assert_eq!(code(&output), 65);
    assert_eq!(failure_record(&output)["mismatch_type"], "length");
}

/// ACM-FR-28
#[test]
fn one_scenario_file_drives_both_registered_tools() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"required_args": [{"name": "prompt", "value": "shared prompt"}]},
            "response": {"stdout": "same for both\n", "stderr": "", "exit_code": 0}
        }"#,
    );

    let claude = fixture.run("claude", &["-p", "shared prompt"]);
    let codex = fixture.run("codex", &["exec", "shared prompt"]);

    assert_eq!(code(&claude), 0);
    assert_eq!(code(&codex), 0);
    assert_eq!(claude.stdout, codex.stdout);
    assert_eq!(claude.stdout, b"same for both\n");
}

/// ACM-FR-04 — the terminator preserves the vector exactly.
#[test]
fn the_received_vector_survives_the_operating_system_intact() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "", "--tool", "-p", "--"]},
            "response": {"stdout": "preserved", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("claude", &["-p", "", "--tool", "-p", "--"]);

    assert_eq!(code(&output), 0, "stderr was {:?}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"preserved");
}

/// ACM-FR-04 — quoting boundaries and repeats.
#[test]
fn embedded_spaces_quotes_and_repeats_are_preserved() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "two  spaces and \"quotes\"", "-p", "-p"]},
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("claude", &["-p", "two  spaces and \"quotes\"", "-p", "-p"]);
    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"ok");
}

/// ACM-FR-03, ACM-FR-05, ACM-FR-23, ACM-FR-25
#[test]
fn each_invocation_error_has_its_own_category_and_tool_presence() {
    let fixture = Fixture::new(OK_CLAUDE);
    let path = fixture.scenario.to_str().unwrap();

    let cases: [(Vec<&str>, &str, bool); 4] = [
        (vec!["--scenario", path], "missing_tool_option", false),
        (
            vec!["--tool", "opencode", "--scenario", path],
            "unknown_tool",
            false,
        ),
        (vec!["--tool", "claude"], "missing_scenario_option", true),
        (
            vec!["--tool", "claude", "--scenario", path, "--verbose"],
            "unknown_option",
            true,
        ),
    ];

    for (args, expected, carries_tool) in cases {
        let output = Command::new(MOCK).args(&args).output().expect("launch");

        assert_eq!(code(&output), 64, "for {args:?}");
        assert!(output.stdout.is_empty(), "for {args:?}");
        let record = failure_record(&output);
        assert_eq!(record["mismatch_type"], expected, "for {args:?}");
        assert_eq!(
            record.get("tool").is_some(),
            carries_tool,
            "tool presence for {args:?}"
        );
    }
}

/// ACM-FR-07, ACM-FR-12 — the three stdin states, over real pipes.
#[test]
fn absent_null_and_exact_stdin_differ_over_a_real_pipe() {
    let absent = Fixture::new(
        r#"{"version":1,"expected":{"args":["-p"]},"response":{"stdout":"o","stderr":"","exit_code":0}}"#,
    );
    let null = Fixture::new(
        r#"{"version":1,"expected":{"args":["-p"],"stdin":null},"response":{"stdout":"o","stderr":"","exit_code":0}}"#,
    );
    let exact = Fixture::new(
        r#"{"version":1,"expected":{"args":["-p"],"stdin":"payload"},"response":{"stdout":"o","stderr":"","exit_code":0}}"#,
    );

    // A caller that writes bytes and closes.
    assert_eq!(code(&absent.run_with_stdin("claude", &["-p"], b"payload")), 0);
    let output = null.run_with_stdin("claude", &["-p"], b"payload");
    assert_eq!(code(&output), 65);
    assert_eq!(failure_record(&output)["mismatch_type"], "length");
    assert_eq!(code(&exact.run_with_stdin("claude", &["-p"], b"payload")), 0);

    // A caller that writes nothing and closes.
    assert_eq!(code(&absent.run_with_stdin("claude", &["-p"], b"")), 0);
    assert_eq!(code(&null.run_with_stdin("claude", &["-p"], b"")), 0);
    assert_eq!(code(&exact.run_with_stdin("claude", &["-p"], b"")), 65);

    // A stdin already at EOF when the process starts — here `/dev/null`, which
    // is what `Command::output` attaches — is the same state as an empty pipe.
    assert_eq!(code(&null.run("claude", &["-p"])), 0);
    assert_eq!(code(&exact.run("claude", &["-p"])), 65);
}

/// ACM-FR-11, ACM-FR-25
#[test]
fn required_args_are_positional_independent_and_names_are_never_matched() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"required_args": [{"value": "-p"}, {"name": "prompt", "value": "hello"}]},
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );

    // Position does not matter.
    let output = fixture.run("claude", &["hello", "--model", "opus", "-p"]);
    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"ok");

    // An unsatisfied named requirement reports index, name and expected length
    // — and no actual length, because nothing was found to measure.
    let output = fixture.run("claude", &["-p", "goodbye"]);
    assert_eq!(code(&output), 65);
    let record = failure_record(&output);
    assert_eq!(record["check"], "required_args");
    assert_eq!(record["index"], 1);
    assert_eq!(record["name"], "prompt");
    assert_eq!(record["expected_length"], 5);
    assert!(record.get("actual_length").is_none());
    assert_eq!(record["mismatch_type"], "absent");

    // A name is metadata: a vector carrying the *name* satisfies nothing.
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"required_args": [{"name": "prompt", "value": "hello"}]},
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("claude", &["-p", "prompt"]);
    assert_eq!(code(&output), 65);
    // And it failed for the right reason: the requirement went unsatisfied,
    // rather than the run tripping over some other check.
    let record = failure_record(&output);
    assert_eq!(record["check"], "required_args");
    assert_eq!(record["mismatch_type"], "absent");
    assert_eq!(record["name"], "prompt");
}

/// ACM-FR-10, ACM-FR-11, ACM-FR-17
#[test]
fn args_and_required_args_both_apply() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {
                "args": ["-p", "hello"],
                "required_args": [{"value": "not in the vector"}]
            },
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );
    // The vector matches `args` exactly and the run still fails.
    let output = fixture.run("claude", &["-p", "hello"]);
    assert_eq!(code(&output), 65);
    assert_eq!(failure_record(&output)["check"], "required_args");

    // An empty requirement list alongside matching args passes.
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "hello"], "required_args": []},
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );
    assert_eq!(code(&fixture.run("claude", &["-p", "hello"])), 0);
}

/// ACM-FR-17, ACM-FR-19 — exactly one record, and the first failing check wins.
#[test]
fn exactly_one_record_is_written_whatever_else_would_also_have_failed() {
    let all_would_fail = r#"{
        "version": 1,
        "expected": {
            "args": ["-p", "right"],
            "required_args": [{"name": "needle", "value": "absent"}],
            "stdin": "expected-input"
        },
        "response": {"stdout": "o", "stderr": "", "exit_code": 0}
    }"#;
    let fixture = Fixture::new(all_would_fail);

    let output = fixture.run_with_stdin("claude", &["wrong"], b"different");
    assert_eq!(failure_record(&output)["check"], "adapter");

    let output = fixture.run_with_stdin("claude", &["-p", "wrong"], b"different");
    let record = failure_record(&output);
    assert_eq!(record["check"], "args");
    assert_eq!(record["index"], 1);

    let output = fixture.run_with_stdin("claude", &["-p", "right"], b"different");
    assert_eq!(failure_record(&output)["check"], "required_args");
}

/// ACM-FR-26
#[test]
fn a_mismatch_suppresses_the_configured_response_entirely() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "right"]},
            "response": {
                "stdout": "CONFIGURED-STDOUT",
                "stderr": "CONFIGURED-STDERR",
                "exit_code": 9
            }
        }"#,
    );
    let output = fixture.run("claude", &["-p", "wrong"]);

    assert_eq!(code(&output), 65, "the configured code is not used");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(!stderr.contains("CONFIGURED-STDERR"));
    assert_eq!(failure_record(&output)["kind"], "expectation_mismatch");
}

/// ACM-FR-22 — streams stay separate and whole at size, and the process ends
/// on its own.
#[test]
fn large_payloads_are_neither_merged_nor_truncated() {
    let out_payload = "a".repeat(1024 * 1024);
    let err_payload = "b".repeat(1024 * 1024);
    let fixture = Fixture::new(&format!(
        r#"{{"version":1,"expected":{{}},"response":{{"stdout":"{out_payload}","stderr":"{err_payload}","exit_code":0}}}}"#
    ));

    let output = fixture.run("claude", &["-p"]);

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout.len(), 1024 * 1024);
    assert_eq!(output.stderr.len(), 1024 * 1024);
    assert!(output.stdout.iter().all(|b| *b == b'a'));
    assert!(output.stderr.iter().all(|b| *b == b'b'));
}

/// ACM-FR-23 — byte-identical diagnostics across repeated runs.
#[test]
fn a_failing_invocation_is_byte_identical_every_time() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "right"]},
            "response": {"stdout": "o", "stderr": "", "exit_code": 0}
        }"#,
    );

    let first = fixture.run("claude", &["-p", "wrong"]);
    let baseline = first.stderr.clone();
    assert!(!baseline.is_empty());

    for _ in 0..9 {
        let output = fixture.run("claude", &["-p", "wrong"]);
        assert_eq!(output.stderr, baseline);
        assert_eq!(code(&output), 65);
    }

    // And the documented field order.
    let text = String::from_utf8(baseline).unwrap();
    assert_eq!(
        text,
        "{\"kind\":\"expectation_mismatch\",\"code\":65,\"tool\":\"claude\",\"check\":\"args\",\"index\":1,\"expected_length\":5,\"actual_length\":5,\"mismatch_type\":\"value\"}\n"
    );
}

/// ACM-FR-30 / ACM-FR-29 — *every* shipped example is valid, not just the two
/// this file happens to name. An example that drifts from the schema has to
/// fail the suite, which means the suite must discover them rather than list
/// them.
#[test]
fn every_shipped_example_scenario_is_valid_under_the_schema() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");

    let examples: Vec<_> = std::fs::read_dir(&directory)
        .expect("scenarios/ exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();

    assert!(!examples.is_empty(), "scenarios/ ships no examples");

    // Every registered adapter has at least one example named after it.
    for tool in ["claude", "codex", "docker"] {
        assert!(
            examples.iter().any(|path| path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(tool))),
            "no example scenario for {tool}"
        );
    }

    // Loading each one proves it parses under the schema: a document the mock
    // rejects exits 64 with `check: "scenario"`, whatever the arguments were.
    for path in &examples {
        let output = Command::new(MOCK)
            .arg("--tool")
            .arg("claude")
            .arg("--scenario")
            .arg(path)
            .args(["--", "-p", "irrelevant to loading"])
            .output()
            .expect("launch");

        if code(&output) == 64 {
            panic!(
                "example {} is not valid under the schema: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// ACM-FR-29, ACM-FR-30 — and the two documented examples run end to end.
#[test]
fn the_shipped_example_scenarios_run_end_to_end() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");

    let claude = directory.join("claude-headless.json");
    let output = Command::new(MOCK)
        .arg("--tool")
        .arg("claude")
        .arg("--scenario")
        .arg(&claude)
        .args(["--", "-p", "Summarise the staged diff in one sentence."])
        .output()
        .expect("launch");
    assert_eq!(
        code(&output),
        0,
        "claude example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty());
    // The example's stdout is exact serialized JSON the mock never re-parsed.
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("example emits valid JSON");
    assert_eq!(parsed["result"], "ok");

    let codex = directory.join("codex-headless.json");
    let output = Command::new(MOCK)
        .arg("--tool")
        .arg("codex")
        .arg("--scenario")
        .arg(&codex)
        .args(["--", "exec", "cargo test --manifest-path Cargo.toml"])
        .output()
        .expect("launch");
    assert_eq!(
        code(&output),
        0,
        "codex example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty());
    assert!(!output.stderr.is_empty(), "the codex example exercises stderr");
}

/// ACM-FR-10, ACM-FR-12, ACM-FR-18, ACM-FR-20 — the `docker` example run end to end, which is what standing in
/// for the container runtime looks like from a caller's side: the whole `docker
/// run` vector is validated, the vendor's own vector rides along as its tail,
/// the task payload arrives on stdin, and the configured bytes come back as the
/// container's streams.
///
/// The vector below is the shape `EAC`'s `docker_run_args` generates. Two parts
/// of a real one are deliberately matched by `required_args` rather than as an
/// exact vector, because they vary by machine and by launch: the container name
/// (unique per request) and the host `uid:gid`. Everything a reviewer would
/// want pinned — the flags, the mount, the env *name*, and the whole vendor
/// tail — is pinned.
#[test]
fn the_docker_example_asserts_the_runtime_and_vendor_vectors_together() {
    let scenario = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenarios")
        .join("docker-run.json");

    let vector = [
        "run",
        "--rm",
        "--interactive",
        "--name",
        "synthesis-agent-4242-1700000000000000000-0",
        "--network",
        "bridge",
        "--user",
        "501:20",
        "--workdir",
        "/workspace",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--mount",
        "type=bind,source=/host/worktree,target=/workspace",
        // The session-state mount, without which no session survives the
        // container that created it (EAC-FR-31).
        "--mount",
        "type=bind,source=/host/sessions,target=/home/agent/.claude",
        "--env",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "--env",
        "CLAUDE_CONFIG_DIR=/home/agent/.claude",
        "ghcr.io/br5-tech/synthesis-agent-claude-code@sha256:abababababababababababababababababababababababababababababababab",
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        // The envelope schema is fixed text the executor compiles in; the
        // scenario pins the flag and leaves the kilobyte of JSON beside it to
        // the executor's own tests, which compare it byte for byte.
        "--json-schema",
        "{\"type\":\"object\"}",
        "--permission-mode",
        "bypassPermissions",
        "--session-id",
        "00000000-0000-4000-8000-000000000000",
    ];
    let stdin = concat!(
        r#"{"protocol_version":1,"instruction":"revise the draft","input":null,"#,
        r#""resume":null,"execution":{"timeout_ms":60000,"cancellation":"caller_controlled"}}"#
    );

    let output = run_at(&scenario, "docker", &vector, stdin.as_bytes());
    assert_eq!(
        code(&output),
        0,
        "docker example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The replayed stdout is the vendor's own event stream: one JSON object per
    // line, with the response envelope carried by the result event where the
    // executor's descriptor says to look for it.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let events: Vec<Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid JSON"))
        .collect();
    assert!(events.len() > 1, "the replay is a stream, not one document");
    let result = events
        .iter()
        .rev()
        .find(|event| event["type"] == "result")
        .expect("a result event");
    let envelope: Value =
        serde_json::from_str(result["result"].as_str().expect("result is JSON text"))
            .expect("the envelope parses");
    assert_eq!(envelope["outcome"], "success");

    // The same vector's tail is the claude vendor vector, so the claude adapter
    // validates it too — one recorded invocation covers both halves.
    let output = run_at(&scenario, "claude", &vector, stdin.as_bytes());
    assert_eq!(code(&output), 0);

    // A tail that differs from what the scenario expected fails, so the vendor
    // half is genuinely asserted rather than carried along. The regression
    // chosen is a real one: `acceptEdits` leaves a container-isolated agent
    // unable to run the commands its turn depends on, and it is exactly what
    // this vector used to carry.
    let mut wrong = vector;
    let at = wrong
        .iter()
        .position(|a| *a == "bypassPermissions")
        .expect("the permission mode");
    wrong[at] = "acceptEdits";
    let output = run_at(&scenario, "docker", &wrong, stdin.as_bytes());
    assert_eq!(code(&output), 65);
    let record = failure_record(&output);
    assert_eq!(record["check"], "required_args");

    // And stdin is compared byte for byte: a task the caller did not send is a
    // mismatch, not a pass.
    let output = run_at(&scenario, "docker", &vector, b"{}");
    assert_eq!(code(&output), 65);
    assert_eq!(failure_record(&output)["check"], "stdin");
}

/// Run the mock against a scenario that already exists on disk, writing `input`
/// to stdin and closing it. The `Fixture` helper writes its own scenario, which
/// is the wrong shape for the examples the crate ships.
fn run_at(scenario: &std::path::Path, tool: &str, tool_args: &[&str], input: &[u8]) -> Output {
    let mut command = Command::new(MOCK);
    command.arg("--tool").arg(tool);
    command.arg("--scenario").arg(scenario);
    command.arg("--").args(tool_args);

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch the mock");

    write_stdin_and_close(&mut child, input);
    child.wait_with_output().expect("collect output")
}

/// ACM-FR-18, ACM-FR-28 — the runtime's own refusal code passes through unchanged, so a
/// caller can still tell "the runtime would not start a container" from "the
/// agent failed".
#[test]
fn a_runtime_refusal_exit_code_passes_through_unchanged() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": { "required_args": [{ "value": "run" }] },
            "response": {
                "stdout": "",
                "stderr": "docker: Error response from daemon: no such image.\n",
                "exit_code": 125
            }
        }"#,
    );

    let output = fixture.run("docker", &["run", "missing-image"]);
    assert_eq!(code(&output), 125);
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no such image"));
    // Not a failure record: 125 is the configured response, not a mock verdict.
    assert!(serde_json::from_slice::<Value>(&output.stderr).is_err());
}

/// ACM-FR-02, ACM-FR-06 / ACM-FR-01 — the crate's isolation, as far as the manifest can
/// prove it: no workspace membership, no path dependency on the application,
/// and exactly the three dependencies the spec allows.
///
/// This is what makes "builds with `src-tauri/` absent" true. The claim itself
/// is not asserted by a run — a test that shelled out to `cargo build` in a
/// scratch directory would need a warm registry to be reliable — so what is
/// checked here is the property that produces it. The `CI / mock` lane
/// separately proves the crate needs none of the system packages the
/// application links against, by installing none of them and still compiling.
#[test]
fn the_manifest_keeps_the_crate_independent_of_the_application() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("read manifest");

    // Declarations only — the manifest's comments legitimately mention the
    // application by name while explaining why nothing here depends on it.
    let manifest: String = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !manifest.contains("path ="),
        "a path dependency ties this crate to its neighbours"
    );
    assert!(
        !manifest.contains("synthesis"),
        "this crate must not depend on the application"
    );
    assert!(
        !manifest.contains("[workspace]"),
        "this crate is its own workspace root and declares no membership"
    );

    // Exactly the dependency set ACM-FR-01 allows: a JSON reader and a base64
    // codec, plus a test-only temp-directory helper.
    let (runtime, development) = manifest
        .split_once("[dev-dependencies]")
        .expect("manifest declares dev-dependencies");
    let runtime = runtime
        .split_once("[dependencies]")
        .expect("manifest declares dependencies")
        .1;

    for crate_name in ["serde", "serde_json", "base64"] {
        assert!(runtime.contains(crate_name), "missing {crate_name}");
    }
    for forbidden in ["tauri", "git2", "keyring", "ureq", "reqwest", "tokio"] {
        assert!(
            !runtime.contains(forbidden),
            "{forbidden} is not permitted in this crate"
        );
    }
    assert!(development.contains("tempfile"));
}

/// ACM-FR-01, ACM-FR-02, ACM-FR-06 — the scenario is the only file touched, and it is read-only.
///
/// "Opened no socket" and "spawned no child process" are not observed here:
/// doing so portably would need a syscall tracer this suite deliberately does
/// not depend on. What is asserted is every filesystem effect a run could have.
#[test]
fn a_run_creates_nothing_and_leaves_the_scenario_untouched() {
    let workspace = tempfile::tempdir().expect("temp dir");
    let scenario = workspace.path().join("scenario.json");
    let text = r#"{
        "version": 1,
        "expected": {"args": ["-p", "hi"], "stdin": "input\n"},
        "response": {"stdout": "out\n", "stderr": "err\n", "exit_code": 3}
    }"#;
    std::fs::write(&scenario, text).expect("write scenario");

    let before = std::fs::metadata(&scenario).expect("stat").modified().ok();

    // A working directory of its own, so anything the mock wrote relative to
    // the cwd would land somewhere observable.
    let cwd = tempfile::tempdir().expect("temp cwd");
    let mut child = Command::new(MOCK)
        .current_dir(cwd.path())
        .arg("--tool")
        .arg("claude")
        .arg("--scenario")
        .arg(&scenario)
        .args(["--", "-p", "hi"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch");
    write_stdin_and_close(&mut child, b"input\n");
    let output = child.wait_with_output().expect("collect");

    assert_eq!(code(&output), 3);
    assert_eq!(output.stdout, b"out\n");

    // Nothing created anywhere the run could reach by a relative path.
    let created: Vec<_> = std::fs::read_dir(cwd.path())
        .expect("read cwd")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert!(created.is_empty(), "the run created {created:?}");

    // The scenario is input, not a workspace: same bytes, and not rewritten.
    assert_eq!(std::fs::read_to_string(&scenario).expect("reread"), text);
    let after = std::fs::metadata(&scenario).expect("stat").modified().ok();
    assert_eq!(before, after, "the scenario file was written to");

    // And the directory holding it gained nothing either — no lock file, no
    // sibling temp file.
    let siblings: Vec<_> = std::fs::read_dir(workspace.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(siblings.len(), 1, "the scenario's directory gained {siblings:?}");
}

/// ACM-FR-07 — a caller may always write and close.
#[test]
fn writing_to_a_scenario_that_ignores_stdin_never_breaks_the_pipe() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"]},
            "response": {"stdout": "ok", "stderr": "", "exit_code": 0}
        }"#,
    );

    // A megabyte, far past any pipe buffer, into a scenario that omits
    // `expected.stdin` entirely.
    let payload = vec![b'x'; 1024 * 1024];
    let output = fixture.run_with_stdin("claude", &["-p"], &payload);

    assert_eq!(code(&output), 0);
    assert_eq!(output.stdout, b"ok");
}

/// ACM-FR-07, second clause — a caller that never closes stdin gets no
/// response, and its own timeout is what ends the run.
///
/// This is the property that makes the mock usable from a harness at all: if
/// the blocking read were ever "fixed" into a non-blocking one, the mock would
/// start answering before the caller had finished writing.
#[test]
fn a_stdin_that_never_closes_keeps_the_run_pending() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p"], "stdin": "anything"},
            "response": {"stdout": "TOO EARLY", "stderr": "", "exit_code": 0}
        }"#,
    );

    // stdin is piped and the handle is deliberately kept alive, so the pipe
    // never reaches EOF.
    let mut child = fixture
        .command("claude", &["-p"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch the mock");

    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        assert!(
            child.try_wait().expect("poll").is_none(),
            "the mock answered before stdin reached EOF"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    child.kill().expect("terminate");
    let output = child.wait_with_output().expect("collect");
    assert!(output.stdout.is_empty(), "no response while stdin was open");
    assert!(output.stderr.is_empty());
}

/// ACM-FR-25 — the `arity` record shape, across the process boundary: element
/// counts and no index.
#[test]
fn an_arity_mismatch_reports_counts_and_no_index() {
    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "one", "two"]},
            "response": {"stdout": "o", "stderr": "", "exit_code": 0}
        }"#,
    );
    let output = fixture.run("claude", &["-p"]);

    assert_eq!(code(&output), 65);
    let record = failure_record(&output);
    assert_eq!(record["check"], "args");
    assert_eq!(record["mismatch_type"], "arity");
    assert!(
        record.get("index").is_none(),
        "an arity failure has no single position to blame"
    );
    assert_eq!(record["expected_length"], 3);
    assert_eq!(record["actual_length"], 1);
    assert!(record.get("name").is_none());
}

/// ACM-FR-25 — `bad_type`, end to end rather than only in the parser's own
/// tests.
#[test]
fn a_structurally_wrong_scenario_reports_bad_type() {
    let fixture = Fixture::new(
        r#"{"version":1,"expected":{"args":"-p"},"response":{"stdout":"","stderr":"","exit_code":0}}"#,
    );
    let output = fixture.run("claude", &["-p"]);

    assert_eq!(code(&output), 64);
    let record = failure_record(&output);
    assert_eq!(record["kind"], "configuration_error");
    assert_eq!(record["mismatch_type"], "bad_type");
}

/// ACM-FR-04 — an argument that is not valid Unicode reaches the comparison as
/// the bytes the operating system delivered. This is what `args_os` in
/// `main.rs` buys, and without a test the deliberate choice looks incidental.
#[cfg(unix)]
#[test]
fn a_non_utf8_argument_is_compared_as_bytes_and_never_disclosed() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let fixture = Fixture::new(
        r#"{
            "version": 1,
            "expected": {"args": ["-p", "abc"]},
            "response": {"stdout": "o", "stderr": "", "exit_code": 0}
        }"#,
    );

    // Four bytes, one of them a lone continuation byte: not valid UTF-8.
    let invalid = OsString::from_vec(vec![b'a', 0xff, b'c', b'd']);
    let output = fixture
        .command("claude", &[])
        .arg("--")
        .arg("-p")
        .arg(&invalid)
        .output()
        .expect("launch");

    assert_eq!(code(&output), 65, "the argument does not match `abc`");
    let record = failure_record(&output);
    assert_eq!(record["check"], "args");
    assert_eq!(record["mismatch_type"], "value");
    assert_eq!(record["index"], 1);
    assert_eq!(record["expected_length"], 3);
    // Four bytes were delivered and four were counted — nothing was lossily
    // converted on the way in.
    assert_eq!(record["actual_length"], 4);

    // And the record is still clean UTF-8 containing none of those bytes.
    let stderr = String::from_utf8(output.stderr.clone()).expect("record is utf8");
    assert!(!stderr.as_bytes().contains(&0xff));
}

/// ACM-FR-30 — the README documents what the spec requires it to. Without this
/// the documentation requirement is the one FR nothing holds in place.
#[test]
fn the_readme_documents_everything_the_spec_requires() {
    let readme =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
            .expect("read README.md");

    let required: [(&str, &str); 9] = [
        ("the build command", "cargo build --manifest-path tools/agentic-cli-mock/Cargo.toml"),
        ("the executable path", "tools/agentic-cli-mock/target/debug/agentic-cli-mock"),
        (
            "the command-line grammar",
            "--tool <claude|codex|docker|docker+claude|docker+codex>",
        ),
        ("the delay bound", "0..=10000"),
        ("the configuration-error code", "`64`"),
        ("the expectation-mismatch code", "`65`"),
        ("the failure record shape", "\"mismatch_type\""),
        ("the stdin-closure requirement", "stdin must reach EOF"),
        ("the harness-responsibility note", "harness's responsibility"),
    ];

    for (what, needle) in required {
        assert!(
            readme.contains(needle),
            "README.md does not document {what} (looked for {needle:?})"
        );
    }

    // The record shape section must also say what it never contains.
    assert!(
        readme.contains("never contains an expected or actual value"),
        "README.md must state the disclosure rule of ACM-FR-24"
    );
}
