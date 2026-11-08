//! EAC-FR-WQNH — a Claude Code launch sets the two Bash timeouts of its CLI,
//! whatever the image and the project set, and leaves a timeout that the launch
//! environment names to that environment.

use super::*;

/// The `NAME=value` arguments of the Bash timeouts in a recorded argv.
fn timeout_literals(argv: &[String]) -> Vec<String> {
    argv.iter().filter(|a| a.starts_with("BASH_") && a.contains('=')).cloned().collect()
}

// EAC-FR-WQNH: a Claude Code container gets both timeouts at 15 minutes.
#[test]
fn a_claude_code_launch_sets_both_bash_timeouts() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");

    assert_eq!(
        timeout_literals(&runtime.only_run().argv),
        vec![
            "BASH_DEFAULT_TIMEOUT_MS=900000".to_string(),
            "BASH_MAX_TIMEOUT_MS=900000".to_string(),
        ]
    );
}

// EAC-FR-WQNH: a Codex container gets neither timeout.
#[test]
fn a_codex_launch_sets_no_bash_timeout() {
    let harness = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    run(&harness, runtime.clone(), task("go")).expect("runs");

    let recorded = runtime.only_run();
    assert!(timeout_literals(&recorded.argv).is_empty(), "{:?}", recorded.argv);
    assert!(!recorded.env.keys().any(|name| name.starts_with("BASH_")));
}

// EAC-FR-WQNH, EAC-FR-OWPP: a timeout that the author's entry names keeps the
// author's value, and the executor still sets the other one.
#[test]
fn a_timeout_the_launch_environment_names_keeps_its_value() {
    use crate::agentic::AgenticRecord;

    let harness = harness_for("claude_code");
    let (mut records, active) = harness.store.load_agentic_registry().expect("registry");
    let record: &mut AgenticRecord =
        records.iter_mut().find(|r| r.vendor == "claude_code").unwrap();
    record.env_vars = vec!["BASH_MAX_TIMEOUT_MS=3600000".to_string()];
    harness.store.save_agentic_registry(records, active).expect("saved");

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();

    assert_eq!(recorded.env.get("BASH_MAX_TIMEOUT_MS").map(String::as_str), Some("3600000"));
    assert_eq!(
        timeout_literals(&recorded.argv),
        vec!["BASH_DEFAULT_TIMEOUT_MS=900000".to_string()],
        "the executor sets only the timeout the author did not name"
    );
    assert_eq!(recorded.argv.iter().filter(|a| *a == "BASH_MAX_TIMEOUT_MS").count(), 1);
}

// EAC-FR-WQNH: the rule itself, for each vendor.
#[test]
fn the_timeouts_depend_on_the_vendor_and_the_named_variables() {
    assert_eq!(
        descriptor::bash_timeouts("claude_code", &[]),
        vec![
            ("BASH_DEFAULT_TIMEOUT_MS".to_string(), "900000".to_string()),
            ("BASH_MAX_TIMEOUT_MS".to_string(), "900000".to_string()),
        ]
    );
    assert!(descriptor::bash_timeouts("codex", &[]).is_empty());
    assert!(descriptor::bash_timeouts(
        "claude_code",
        &["BASH_DEFAULT_TIMEOUT_MS".to_string(), "BASH_MAX_TIMEOUT_MS".to_string()]
    )
    .is_empty());
}
