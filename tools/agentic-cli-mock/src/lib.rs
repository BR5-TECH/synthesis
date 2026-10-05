//! `agentic-cli-mock` — a standalone stand-in for an agentic CLI.
//!
//! See `specifications/infra/ACM-agentic-cli-mock.md`. The process is a fixed
//! sequence, and the order matters to the contract:
//!
//! 1. parse the mock's own options (ACM-FR-03, ACM-FR-04, ACM-FR-05)
//! 2. read stdin to EOF (ACM-FR-07) — before the scenario is opened
//! 3. load and validate the scenario (ACM-FR-08 … ACM-FR-16)
//! 4. run the checks in order, first failure wins (ACM-FR-17)
//! 5. wait out any configured delay (ACM-FR-21)
//! 6. emit the configured response and exit (ACM-FR-20, ACM-FR-22)

pub mod adapter;
pub mod cli;
pub mod diagnostics;
pub mod matching;
pub mod scenario;

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::time::Duration;

use adapter::ToolAdapter;
use diagnostics::{FailureRecord, MismatchType};
use scenario::Scenario;

/// Everything the runner needs from the outside world, so the whole sequence
/// can be exercised without spawning a process. The real binary supplies the
/// process's own streams and `thread::sleep`.
pub struct Environment<'a> {
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    pub sleep: &'a dyn Fn(Duration),
}

/// Run the mock to completion and return the process exit code.
pub fn run(
    argv: &[OsString],
    registry: &[&(dyn ToolAdapter + 'static)],
    env: &mut Environment<'_>,
) -> u8 {
    // 1. The mock's own options. Nothing is read from stdin and no file is
    //    opened until this succeeds.
    let invocation = match cli::parse(argv, registry) {
        Ok(invocation) => invocation,
        Err(error) => {
            return emit_failure(
                env.stderr,
                FailureRecord::configuration_error(error.tool.as_deref(), error.mismatch_type),
            )
        }
    };
    let tool = invocation.adapter.id();

    // 2. ACM-FR-07: stdin is drained on every run that gets this far, whether
    //    or not the scenario validates it, and before the scenario is opened.
    //    That is what lets a caller always write-and-close without risking a
    //    broken pipe.
    let mut received_stdin = Vec::new();
    // A stdin that cannot be read is not a failure of its own — the scenario
    // still decides whether what arrived matches. Whatever *did* arrive is
    // kept: discarding it would let a read that failed part-way satisfy a
    // `null` expectation, reporting zero bytes for a stream that carried data.
    let _ = env.stdin.read_to_end(&mut received_stdin);

    // 3. ACM-FR-06: the scenario is the one file this process ever opens, and
    //    it opens it read-only.
    let text = match std::fs::read_to_string(&invocation.scenario_path) {
        Ok(text) => text,
        Err(_) => {
            return emit_failure(
                env.stderr,
                // ACM-FR-24: the path is never disclosed — it may itself be
                // sensitive.
                FailureRecord::configuration_error(Some(tool), MismatchType::Unreadable),
            )
        }
    };
    let scenario: Scenario = match scenario::parse(&text) {
        Ok(scenario) => scenario,
        Err(mismatch_type) => {
            return emit_failure(
                env.stderr,
                FailureRecord::configuration_error(Some(tool), mismatch_type),
            )
        }
    };

    // 4. ACM-FR-17 / ACM-FR-19: adapter first, then the scenario's own
    //    expectations, in a fixed order. The first failure is the one reported,
    //    and either kind of failure is an expectation mismatch.
    let received: Vec<&OsStr> = invocation
        .tool_args
        .iter()
        .map(OsString::as_os_str)
        .collect();

    let failure = invocation
        .adapter
        .validate(&received)
        .or_else(|| {
            scenario
                .expected
                .args
                .as_deref()
                .and_then(|expected| matching::match_args(expected, &received))
        })
        .or_else(|| {
            scenario
                .expected
                .required_args
                .as_deref()
                .and_then(|expected| matching::match_required_args(expected, &received))
        })
        .or_else(|| matching::match_stdin(&scenario.expected.stdin, &received_stdin));

    if let Some(mismatch) = failure {
        // ACM-FR-26: no configured byte on either stream, and the configured
        // exit code is not used.
        return emit_failure(
            env.stderr,
            FailureRecord::expectation_mismatch(tool, mismatch),
        );
    }

    // 5. ACM-FR-21: the wait happens after every check has passed and before
    //    any configured byte is written, so a caller that kills the process
    //    mid-delay sees empty streams rather than a partial response.
    if scenario.response.delay_ms > 0 {
        (env.sleep)(Duration::from_millis(scenario.response.delay_ms));
    }

    // 6. ACM-FR-20 / ACM-FR-22: exactly the configured bytes, on their own
    //    streams, flushed before the process ends.
    let _ = env.stdout.write_all(&scenario.response.stdout);
    let _ = env.stderr.write_all(&scenario.response.stderr);
    let _ = env.stdout.flush();
    let _ = env.stderr.flush();

    scenario.response.exit_code
}

fn emit_failure(stderr: &mut dyn Write, record: FailureRecord) -> u8 {
    let _ = stderr.write_all(record.to_line().as_bytes());
    let _ = stderr.flush();
    record.code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::REGISTRY;
    use crate::diagnostics::{Check, Kind, EXIT_CONFIGURATION_ERROR, EXIT_EXPECTATION_MISMATCH};
    use serde_json::Value;
    use std::cell::RefCell;
    use std::io::Cursor;

    struct Outcome {
        code: u8,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        slept: Vec<Duration>,
    }

    impl Outcome {
        /// The failure record, parsed. Also asserts the shape ACM-FR-23
        /// requires: exactly one object and exactly one newline.
        fn record(&self) -> Value {
            let text = String::from_utf8(self.stderr.clone()).expect("record is utf8");
            assert!(text.ends_with('\n'), "record must end with a newline");
            assert_eq!(
                text.matches('\n').count(),
                1,
                "record must be exactly one line"
            );
            serde_json::from_str(text.trim_end()).expect("record is one JSON object")
        }
    }

    fn invoke(args: &[&str], scenario_text: &str, stdin: &[u8]) -> Outcome {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("scenario.json");
        std::fs::write(&path, scenario_text).expect("write scenario");

        let argv: Vec<OsString> = args
            .iter()
            .map(|a| {
                if *a == "@scenario" {
                    path.clone().into_os_string()
                } else {
                    OsString::from(*a)
                }
            })
            .collect();

        invoke_raw(&argv, stdin)
    }

    fn invoke_raw(argv: &[OsString], stdin: &[u8]) -> Outcome {
        let slept = RefCell::new(Vec::new());
        let mut input = Cursor::new(stdin.to_vec());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = {
            let sleep = |d: Duration| slept.borrow_mut().push(d);
            let mut env = Environment {
                stdin: &mut input,
                stdout: &mut stdout,
                stderr: &mut stderr,
                sleep: &sleep,
            };
            run(argv, REGISTRY, &mut env)
        };

        Outcome {
            code,
            stdout,
            stderr,
            slept: slept.into_inner(),
        }
    }

    fn ok_scenario(args: &str) -> String {
        format!(
            r#"{{"version":1,"expected":{{"args":{args}}},"response":{{"stdout":"out","stderr":"err","exit_code":0}}}}"#
        )
    }

    // --- the success path --------------------------------------------------

    #[test]
    fn a_passing_run_emits_both_streams_and_the_configured_code() {
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "hi"],
            &ok_scenario(r#"["-p","hi"]"#),
            b"",
        );
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.stdout, b"out");
        assert_eq!(outcome.stderr, b"err");
        assert!(outcome.slept.is_empty());
    }

    #[test]
    fn a_configured_non_reserved_exit_code_is_passed_through() {
        let scenario = r#"{"version":1,"expected":{},"response":{"stdout":"o","stderr":"e","exit_code":42}}"#;
        let outcome = invoke(
            &["--tool", "codex", "--scenario", "@scenario", "--", "exec"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, 42);
        assert_eq!(outcome.stdout, b"o");
        assert_eq!(outcome.stderr, b"e");
    }

    /// ACM-FR-20: an empty payload writes nothing at all, not a blank line.
    #[test]
    fn an_empty_payload_writes_no_bytes() {
        let scenario =
            r#"{"version":1,"expected":{},"response":{"stdout":"","stderr":"","exit_code":7}}"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, 7);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.is_empty());
    }

    // --- ACM-FR-17 / ACM-FR-19: order and precedence -----------------------

    /// Every check would fail; each is satisfied in turn and the next one takes
    /// over, always exactly one record.
    #[test]
    fn the_first_failing_check_in_the_fixed_order_is_the_one_reported() {
        let scenario = r#"{
            "version":1,
            "expected":{
                "args":["-p","right"],
                "required_args":[{"name":"needle","value":"absent-value"}],
                "stdin":"expected-input"
            },
            "response":{"stdout":"out","stderr":"err","exit_code":0}
        }"#;

        // Adapter fails first.
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "wrong"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        assert_eq!(outcome.record()["check"], "adapter");

        // Adapter satisfied; args now fail, at the lowest differing index.
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "wrong"],
            scenario,
            b"",
        );
        let record = outcome.record();
        assert_eq!(record["check"], "args");
        assert_eq!(record["index"], 1);

        // Args satisfied; required_args now fail.
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "right"],
            scenario,
            b"",
        );
        let record = outcome.record();
        assert_eq!(record["check"], "required_args");
        assert_eq!(record["name"], "needle");

        // And with required_args satisfied too, stdin is what is left.
        let scenario_without_requirement = r#"{
            "version":1,
            "expected":{"args":["-p","right"],"stdin":"expected-input"},
            "response":{"stdout":"out","stderr":"err","exit_code":0}
        }"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "right"],
            scenario_without_requirement,
            b"different",
        );
        assert_eq!(outcome.record()["check"], "stdin");
    }

    /// ACM-FR-19: a scenario cannot waive its adapter's requirement.
    #[test]
    fn adapter_validation_applies_even_when_the_scenario_expects_that_vector() {
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "--print", "hi"],
            &ok_scenario(r#"["--print","hi"]"#),
            b"",
        );
        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        let record = outcome.record();
        assert_eq!(record["check"], "adapter");
        assert_eq!(record["mismatch_type"], "missing_headless_argument");
        assert!(outcome.stdout.is_empty());
    }

    /// ACM-FR-19: and satisfying the adapter exempts nothing.
    #[test]
    fn satisfying_the_adapter_does_not_exempt_the_scenario_expectations() {
        let scenario = r#"{
            "version":1,
            "expected":{"args":["-p","hi"],"required_args":[{"value":"missing"}]},
            "response":{"stdout":"out","stderr":"err","exit_code":0}
        }"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "hi"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        assert_eq!(outcome.record()["check"], "required_args");
    }

    #[test]
    fn an_empty_required_args_list_alongside_matching_args_passes() {
        let scenario = r#"{
            "version":1,
            "expected":{"args":["-p"],"required_args":[]},
            "response":{"stdout":"out","stderr":"","exit_code":0}
        }"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.stdout, b"out");
    }

    // --- ACM-FR-26: failures suppress the configured response --------------

    #[test]
    fn a_mismatch_emits_neither_configured_stream_nor_the_configured_code() {
        let scenario = r#"{"version":1,"expected":{"args":["-p","right"]},"response":{"stdout":"CONFIGURED-OUT","stderr":"CONFIGURED-ERR","exit_code":9}}"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "wrong"],
            scenario,
            b"",
        );

        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        assert!(outcome.stdout.is_empty());
        let stderr = String::from_utf8(outcome.stderr.clone()).unwrap();
        assert!(!stderr.contains("CONFIGURED-ERR"));
        assert_eq!(outcome.record()["kind"], "expectation_mismatch");
    }

    #[test]
    fn a_configuration_error_emits_neither_configured_stream() {
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            r#"{"version":9,"expected":{},"response":{"stdout":"CONFIGURED","stderr":"","exit_code":0}}"#,
            b"",
        );
        assert_eq!(outcome.code, EXIT_CONFIGURATION_ERROR);
        assert!(outcome.stdout.is_empty());
        let record = outcome.record();
        assert_eq!(record["kind"], "configuration_error");
        assert_eq!(record["mismatch_type"], "bad_version");
    }

    // --- ACM-FR-24: nothing sensitive in the record ------------------------

    #[test]
    fn a_value_mismatch_discloses_neither_value_nor_the_scenario_path() {
        let secret = "sk-ant-oat01-REDACTEDVALUE";
        let scenario = format!(
            r#"{{"version":1,"expected":{{"args":["-p","{secret}"]}},"response":{{"stdout":"o","stderr":"","exit_code":0}}}}"#
        );
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "wrong"],
            &scenario,
            b"",
        );

        let stderr = String::from_utf8(outcome.stderr.clone()).unwrap();
        assert!(!stderr.contains(secret), "leaked the expected value");
        assert!(!stderr.contains("wrong"), "leaked the received value");
        assert!(!stderr.contains("scenario.json"), "leaked the path");
        assert!(!stderr.contains('/'), "leaked part of a path");

        let record = outcome.record();
        assert_eq!(record["check"], "args");
        assert_eq!(record["index"], 1);
        assert_eq!(record["expected_length"], 26);
        assert_eq!(record["actual_length"], 5);
        assert_eq!(record["mismatch_type"], "value");
    }

    #[test]
    fn an_unreadable_scenario_does_not_disclose_the_path() {
        let argv: Vec<OsString> = [
            "--tool",
            "claude",
            "--scenario",
            "/nonexistent/secret-directory/scenario.json",
            "--",
            "-p",
        ]
        .iter()
        .map(OsString::from)
        .collect();

        let outcome = invoke_raw(&argv, b"");
        assert_eq!(outcome.code, EXIT_CONFIGURATION_ERROR);
        let stderr = String::from_utf8(outcome.stderr.clone()).unwrap();
        assert!(!stderr.contains("secret-directory"));
        assert!(!stderr.contains('/'));

        let record = outcome.record();
        assert_eq!(record["mismatch_type"], "unreadable");
        assert_eq!(record["tool"], "claude");
        assert_eq!(record["check"], "scenario");
    }

    // --- ACM-FR-03 / ACM-FR-05: invocation errors --------------------------

    #[test]
    fn each_invocation_error_reports_its_own_category() {
        let cases: [(&[&str], &str, bool); 4] = [
            (&["--scenario", "@scenario"], "missing_tool_option", false),
            (
                &["--tool", "opencode", "--scenario", "@scenario"],
                "unknown_tool",
                false,
            ),
            (&["--tool", "claude"], "missing_scenario_option", true),
            (
                &["--tool", "claude", "--scenario", "@scenario", "--verbose"],
                "unknown_option",
                true,
            ),
        ];

        for (args, expected, carries_tool) in cases {
            let outcome = invoke(args, &ok_scenario("[]"), b"");
            assert_eq!(outcome.code, EXIT_CONFIGURATION_ERROR, "for {args:?}");
            let record = outcome.record();
            assert_eq!(record["kind"], "configuration_error");
            assert_eq!(record["check"], "scenario");
            assert_eq!(record["mismatch_type"], expected, "for {args:?}");
            assert_eq!(
                record.get("tool").is_some(),
                carries_tool,
                "tool presence for {args:?}"
            );
        }
    }

    // --- ACM-FR-07: stdin -------------------------------------------------

    #[test]
    fn stdin_is_read_even_when_the_scenario_does_not_validate_it() {
        // The scenario omits `expected.stdin` entirely; a megabyte still goes in
        // and influences nothing.
        let input = vec![b'x'; 1024 * 1024];
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            &ok_scenario(r#"["-p"]"#),
            &input,
        );
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.stdout, b"out");
    }

    #[test]
    fn null_stdin_and_absent_stdin_behave_differently_end_to_end() {
        let absent = r#"{"version":1,"expected":{"args":["-p"]},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let null = r#"{"version":1,"expected":{"args":["-p"],"stdin":null},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let exact = r#"{"version":1,"expected":{"args":["-p"],"stdin":"payload"},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let args = ["--tool", "claude", "--scenario", "@scenario", "--", "-p"];

        // With bytes on stdin.
        assert_eq!(invoke(&args, absent, b"payload").code, 0);
        let outcome = invoke(&args, null, b"payload");
        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        let record = outcome.record();
        assert_eq!(record["check"], "stdin");
        assert_eq!(record["mismatch_type"], "length");
        assert_eq!(record["expected_length"], 0);
        assert_eq!(record["actual_length"], 7);
        assert_eq!(invoke(&args, exact, b"payload").code, 0);

        // With none.
        assert_eq!(invoke(&args, absent, b"").code, 0);
        assert_eq!(invoke(&args, null, b"").code, 0);
        assert_eq!(invoke(&args, exact, b"").code, EXIT_EXPECTATION_MISMATCH);
    }

    // --- ACM-FR-21: the delay is requested before anything is written ------

    #[test]
    fn a_configured_delay_is_waited_out_before_the_response() {
        let scenario = r#"{"version":1,"expected":{},"response":{"stdout":"o","stderr":"","exit_code":0,"delay_ms":250}}"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            scenario,
            b"",
        );
        assert_eq!(outcome.slept, vec![Duration::from_millis(250)]);
        assert_eq!(outcome.stdout, b"o");
    }

    /// A failing run never reaches the wait.
    #[test]
    fn a_failing_run_does_not_wait() {
        let scenario = r#"{"version":1,"expected":{"args":["-p","x"]},"response":{"stdout":"o","stderr":"","exit_code":0,"delay_ms":5000}}"#;
        let outcome = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
            scenario,
            b"",
        );
        assert_eq!(outcome.code, EXIT_EXPECTATION_MISMATCH);
        assert!(outcome.slept.is_empty());
    }

    // --- ACM-FR-28: a third adapter changes nothing shared -----------------

    #[test]
    fn one_scenario_drives_every_registered_adapter() {
        // No tool-specific field anywhere in the document.
        let scenario = r#"{
            "version":1,
            "expected":{"required_args":[{"name":"prompt","value":"shared"}]},
            "response":{"stdout":"same","stderr":"","exit_code":0}
        }"#;

        let claude = invoke(
            &["--tool", "claude", "--scenario", "@scenario", "--", "-p", "shared"],
            scenario,
            b"",
        );
        let codex = invoke(
            &["--tool", "codex", "--scenario", "@scenario", "--", "exec", "shared"],
            scenario,
            b"",
        );

        assert_eq!(claude.code, 0);
        assert_eq!(codex.code, 0);
        assert_eq!(claude.stdout, codex.stdout);
        assert_eq!(claude.stdout, b"same");
    }

    #[test]
    fn a_newly_registered_adapter_runs_through_the_same_shared_pipeline() {
        static EXTRA: adapter::HeadlessTokenAdapter =
            adapter::HeadlessTokenAdapter::requiring("demo", &["--headless"]);
        let registry: &[&(dyn ToolAdapter + 'static)] = &[REGISTRY[0], REGISTRY[1], &EXTRA];

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("scenario.json");
        std::fs::write(
            &path,
            r#"{"version":1,"expected":{"required_args":[{"value":"shared"}]},"response":{"stdout":"same","stderr":"","exit_code":3}}"#,
        )
        .expect("write");

        let argv: Vec<OsString> = vec![
            OsString::from("--tool"),
            OsString::from("demo"),
            OsString::from("--scenario"),
            path.clone().into_os_string(),
            OsString::from("--"),
            OsString::from("--headless"),
            OsString::from("shared"),
        ];

        let slept = RefCell::new(Vec::new());
        let mut input = Cursor::new(Vec::new());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = {
            let sleep = |d: Duration| slept.borrow_mut().push(d);
            let mut env = Environment {
                stdin: &mut input,
                stdout: &mut stdout,
                stderr: &mut stderr,
                sleep: &sleep,
            };
            run(&argv, registry, &mut env)
        };

        assert_eq!(code, 3);
        assert_eq!(stdout, b"same");

        // And its own headless rule is enforced by the same shared path.
        let argv: Vec<OsString> = vec![
            OsString::from("--tool"),
            OsString::from("demo"),
            OsString::from("--scenario"),
            path.into_os_string(),
            OsString::from("--"),
            OsString::from("shared"),
        ];
        let mut input = Cursor::new(Vec::new());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = {
            let sleep = |_: Duration| {};
            let mut env = Environment {
                stdin: &mut input,
                stdout: &mut stdout,
                stderr: &mut stderr,
                sleep: &sleep,
            };
            run(&argv, registry, &mut env)
        };
        assert_eq!(code, EXIT_EXPECTATION_MISMATCH);
        assert!(stdout.is_empty());
        let record: Value = serde_json::from_str(
            String::from_utf8(stderr).unwrap().trim_end(),
        )
        .unwrap();
        assert_eq!(record["tool"], "demo");
        assert_eq!(record["check"], "adapter");
    }

    // --- ACM-FR-23: determinism -------------------------------------------

    #[test]
    fn the_same_input_and_failure_produce_byte_identical_stderr() {
        let scenario = r#"{"version":1,"expected":{"args":["-p","right"]},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let args = ["--tool", "claude", "--scenario", "@scenario", "--", "-p", "wrong"];

        let first = invoke(&args, scenario, b"").stderr;
        for _ in 0..9 {
            assert_eq!(invoke(&args, scenario, b"").stderr, first);
        }
    }

    /// ACM-FR-23: `kind`, `code` and `check` on every record whatever failed —
    /// and nothing that does not apply to that failure.
    #[test]
    fn every_record_carries_kind_code_and_check_and_nothing_inapplicable() {
        let stdin_scenario = r#"{"version":1,"expected":{"args":["-p"],"stdin":null},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let required_scenario = r#"{"version":1,"expected":{"required_args":[{"value":"absent"}]},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;
        let args_scenario = r#"{"version":1,"expected":{"args":["-p","right"]},"response":{"stdout":"o","stderr":"","exit_code":0}}"#;

        // One failure of each `check`, in turn.
        let cases: [(Vec<&str>, &str, &str, &[u8]); 5] = [
            (
                vec!["--scenario", "@scenario"],
                "scenario",
                &ok_scenario("[]"),
                b"",
            ),
            (
                vec!["--tool", "claude", "--scenario", "@scenario", "--", "nope"],
                "adapter",
                args_scenario,
                b"",
            ),
            (
                vec!["--tool", "claude", "--scenario", "@scenario", "--", "-p", "wrong"],
                "args",
                args_scenario,
                b"",
            ),
            (
                vec!["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
                "required_args",
                required_scenario,
                b"",
            ),
            (
                vec!["--tool", "claude", "--scenario", "@scenario", "--", "-p"],
                "stdin",
                stdin_scenario,
                b"bytes",
            ),
        ];

        for (args, expected_check, scenario, stdin) in cases {
            let outcome = invoke(&args, scenario, stdin);
            let record = outcome.record();

            assert!(record.get("kind").is_some(), "kind for {expected_check}");
            assert!(record.get("code").is_some(), "code for {expected_check}");
            assert_eq!(record["check"], expected_check);
            assert!(
                record.get("mismatch_type").is_some(),
                "mismatch_type for {expected_check}"
            );

            // `index` and `name` belong only to the checks that have a location
            // and a caller-supplied label.
            if matches!(expected_check, "scenario" | "adapter" | "stdin") {
                assert!(
                    record.get("index").is_none(),
                    "{expected_check} has no index"
                );
            }
            if expected_check != "required_args" {
                assert!(record.get("name").is_none(), "{expected_check} has no name");
            }
            if matches!(expected_check, "scenario" | "adapter") {
                assert!(
                    record.get("expected_length").is_none(),
                    "{expected_check} has no lengths"
                );
                assert!(record.get("actual_length").is_none());
            }
        }
    }

    #[test]
    fn the_reserved_codes_map_to_the_documented_kinds() {
        assert_eq!(EXIT_CONFIGURATION_ERROR, 64);
        assert_eq!(EXIT_EXPECTATION_MISMATCH, 65);
        let record = FailureRecord::configuration_error(None, MismatchType::Unreadable);
        assert_eq!(record.kind, Kind::ConfigurationError);
        assert_eq!(record.check, Check::Scenario);
    }
}
