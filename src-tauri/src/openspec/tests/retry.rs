//! The one more try of a transient process, the diagnostics a failure
//! carries, and the re-check of a failed CLI check
//! (`OFP-openspec-failure-policy.md`).
//!
//! The wait goes through a recorded wait seam: every test asserts the
//! requested delay and the count of process starts, and no test waits.
//! OFP-FR-XDXA: no OpenSpec CLI, no container runtime, no network, and no
//! real wait.

use super::*;
use crate::openspec::RecordedWait;

const SPECS: &str = r#"{ "specs": [ { "id": "auth", "requirementCount": 1 } ] }"#;

fn exit(status: Option<i32>, stdout: &str, stderr: &str) -> Answer {
    Ok(ProcessOutput {
        exit_status: status,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    })
}

/// A seam over `runner` whose wait is recorded.
fn waiting_seam(runner: FakeRunner) -> (OpenSpec, Arc<FakeRunner>, Arc<RecordedWait>) {
    let (seam, runner, _) = seam_with(runner);
    let wait = Arc::new(RecordedWait::default());
    (seam.with_wait(wait.clone()), runner, wait)
}

// OFP-FR-PQNS, OFP-FR-DGSC, OFP-FR-MJZU, OFP-FR-OFLU, OFP-FR-WNVO,
// OFP-FR-XDXA: a timeout,
// an exit by a signal, an exit status other than 0 and 1, and exit 1 with
// output that is not CLI JSON each run the process once more after a wait of
// 2 seconds, and no more. The read fails with the outcome of the second
// process and the standard error of the last one.
#[test]
fn a_transient_outcome_runs_once_more_after_two_seconds() {
    let (_dir, root) = openspec_project();
    let cases: Vec<(Answer, &str)> = vec![
        (Err(ProcessError::TimedOut { stderr: b"slow".to_vec() }), "cli_timed_out"),
        (exit(None, "", "killed"), "cli_failed"),
        (exit(Some(3), "", "boom"), "cli_failed"),
        (exit(Some(1), "Error: not json", "Error: bad"), "cli_failed"),
    ];
    for (answer, code) in cases {
        let (seam, runner, wait) = waiting_seam(
            FakeRunner::default()
                .then("list", vec![answer.clone()])
                .answering("list", exit(Some(2), "", "last words")),
        );
        let error = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap_err();
        assert_eq!(runner.calls_of("list"), 2, "{code}: two processes, no more");
        assert_eq!(wait.delays(), vec![2_000], "{code}: one wait of 2 seconds");
        assert_eq!(error.code(), "cli_failed", "{code}: the second outcome is the outcome");
        assert_eq!(error.diagnostics().len(), 1);
        assert_eq!(error.diagnostics()[0].message, "last words");
        assert_eq!(error.diagnostics()[0].severity, None);
    }
}

// OFP-FR-DGSC: output is CLI JSON where standard output decodes as one JSON
// object. An exit 1 with a JSON array, a JSON string, or a JSON object with
// text after it is `cli_failed`, and the process runs once more.
#[test]
fn exit_one_with_output_that_is_not_one_json_object_is_failed() {
    let (_dir, root) = openspec_project();
    for stdout in [r#"[{ "status": [] }]"#, r#""status""#, r#"{ "status": [] } trailing"#] {
        let (seam, runner, wait) =
            waiting_seam(FakeRunner::default().answering("list", exit(Some(1), stdout, "bad")));
        let error = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap_err();
        assert_eq!(error.code(), "cli_failed", "{stdout}");
        assert_eq!(runner.calls_of("list"), 2, "{stdout}: a transient outcome runs once more");
        assert_eq!(wait.delays(), vec![2_000], "{stdout}");
    }
}

// OFP-FR-WNVO, OFP-FR-QOET: the standard error diagnostic of a transient
// failure keeps its last 4 KiB. The 2 KiB cut of the other diagnostics does
// not apply to it.
#[test]
fn the_standard_error_diagnostic_keeps_more_than_two_kib() {
    let (_dir, root) = openspec_project();
    let stderr = format!("{}END", "e".repeat(3000));
    let (seam, _, _) =
        waiting_seam(FakeRunner::default().answering("list", exit(Some(3), "", &stderr)));
    let error = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap_err();
    let failure = crate::openspec::failure::OpenSpecFailure::of_read(&error);
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.diagnostics[0].message, stderr, "nothing is cut below 4 KiB");
    assert!(failure.diagnostics[0].message.len() > crate::openspec::failure::MAX_TEXT);
}

// OFP-FR-PQNS: a second process that succeeds makes the read succeed.
#[test]
fn a_second_process_that_succeeds_makes_the_read_succeed() {
    let (_dir, root) = openspec_project();
    let (seam, runner, wait) = waiting_seam(
        FakeRunner::default()
            .then("list", vec![Err(ProcessError::timed_out())])
            .json("list", SPECS),
    );
    let read = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(runner.calls_of("list"), 2);
    assert_eq!(wait.delays(), vec![2_000]);
}

// OFP-FR-UPDO, OSM-FR-WWWK, OFP-FR-NJSO: exit 1 with CLI JSON is an answer of
// the CLI: the read fails with `cli_rejected` and its `status` entries, and
// no process runs again. The expected shape beside the list changes nothing.
#[test]
fn exit_one_with_cli_json_is_rejected_and_not_run_again() {
    let (_dir, root) = openspec_project();
    let json = r#"{ "specs": [], "status": [
        { "severity": "error", "code": "spec_invalid", "message": "Bad spec", "fix": "Fix it" },
        { "severity": "warning", "message": "Old schema" } ] }"#;
    let (seam, runner, wait) =
        waiting_seam(FakeRunner::default().answering("list", exit(Some(1), json, "noise")));
    let error = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap_err();
    assert_eq!(runner.calls_of("list"), 1);
    assert!(wait.delays().is_empty());
    let OpenSpecReadError::CliRejected { diagnostics } = error else {
        panic!("not a rejection: {error:?}");
    };
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].code.as_deref(), Some("spec_invalid"));
    assert_eq!(diagnostics[0].fix.as_deref(), Some("Fix it"));
    assert_eq!(diagnostics[1].message, "Old schema");
}

// OFP-FR-UPDO, OSM-FR-XDRN: an environment failure and a validation that
// answers not valid run nothing again.
#[test]
fn an_answer_and_an_environment_failure_run_nothing_again() {
    let (_dir, root) = openspec_project();
    write(&_dir, "openspec/changes/add-logout/proposal.md", "# Why\n");
    let invalid = r#"{ "items": [ { "valid": false, "issues": [
        { "level": "ERROR", "path": "specs/auth/spec.md", "message": "No scenario" } ] } ] }"#;
    let (seam, runner, wait) =
        waiting_seam(FakeRunner::default().answering("validate", exit(Some(1), invalid, "")));
    let validation =
        seam.validate_change(&Silent, buffer(), Some(&root), &root, "add-logout").unwrap();
    assert!(!validation.valid);
    assert_eq!(runner.calls_of("validate"), 1);
    // OFP-FR-RUOP: its issues are its diagnostics.
    let diagnostics = validation.diagnostics();
    assert_eq!(diagnostics[0].severity.as_deref(), Some("error"));
    assert_eq!(diagnostics[0].path.as_deref(), Some("specs/auth/spec.md"));
    assert_eq!(diagnostics[0].message, "No scenario");

    let (_plain, plain_root) = plain_project();
    let error = seam.list_capabilities(&Silent, buffer(), Some(&plain_root)).unwrap_err();
    assert_eq!(error, OpenSpecReadError::OpenSpecModeInactive);
    assert!(error.diagnostics().is_empty());
    assert!(wait.delays().is_empty());
}

// OFP-FR-AZIV: a read of two subcommands runs again only the process whose
// outcome is transient.
#[test]
fn a_read_of_two_subcommands_runs_again_only_the_transient_one() {
    let (_dir, root) = openspec_project();
    let (seam, runner, wait) = waiting_seam(
        FakeRunner::default()
            .json("list", r#"{ "changes": [] }"#)
            .then("status", vec![Err(ProcessError::timed_out())])
            .json("status", r#"{ "changes": [] }"#),
    );
    seam.list_changes(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(runner.calls_of("list"), 1);
    assert_eq!(runner.calls_of("status"), 2);
    assert_eq!(wait.delays(), vec![2_000]);
}

// OFP-FR-MJZU: the wait holds no slot of OSM-FR-QHRV; the first process gave
// its slot back, and the second takes one again.
#[test]
fn the_wait_holds_no_slot() {
    let (_dir, root) = openspec_project();
    let (seam, _, wait) = waiting_seam(
        FakeRunner::default()
            .then("list", vec![Err(ProcessError::timed_out())])
            .json("list", SPECS),
    );
    let slots = seam.slots();
    let held = Arc::new(Mutex::new(Vec::new()));
    let seen = held.clone();
    wait.during(move || seen.lock().unwrap().push(slots.in_use()));
    seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap();
    assert_eq!(*held.lock().unwrap(), vec![0]);
}

// OFP-FR-ZQNU, OSM-FR-IFWM, OFP-FR-WRDX: each retry is one `WARN` record with
// the subcommand, the code of the first outcome, the attempt, and the delay.
// No record holds CLI output, standard error, or a diagnostic.
#[test]
fn each_retry_is_one_warn_record_with_no_output() {
    let (_dir, root) = openspec_project();
    let rejected = r#"{ "status": [ { "message": "SECRET-DIAGNOSTIC" } ] }"#;
    let (seam, _, _) = waiting_seam(
        FakeRunner::default()
            .then("list", vec![Err(ProcessError::TimedOut { stderr: b"SECRET-STDERR".to_vec() })])
            .answering("list", exit(Some(1), rejected, "SECRET-STDERR")),
    );
    let log = buffer();
    seam.list_capabilities(&Silent, log, Some(&root)).unwrap_err();
    let all = records(log);
    let retries: Vec<_> = all.iter().filter(|r| r.message == "openspec cli runs again").collect();
    assert_eq!(retries.len(), 1);
    let retry = retries[0];
    assert_eq!(retry.level, LogLevel::Warn);
    assert!(retry.domains.contains(&crate::logging::Domain::Backend));
    assert_eq!(retry.fields["subcommand"], "list");
    assert_eq!(retry.fields["code"], "cli_timed_out");
    assert_eq!(retry.fields["attempt"], 2);
    assert_eq!(retry.fields["delayMs"], 2_000);
    let text = serde_json::to_string(&all).unwrap();
    assert!(!text.contains("SECRET"), "a record holds CLI text: {text}");
}

// OFP-FR-FETF, OSM-FR-NQBE: a version check whose process times out, ends by
// a signal, or exits with a status other than zero runs once more before the
// check records `failed`; a file that cannot start runs once.
#[test]
fn a_version_check_runs_once_more_before_it_fails() {
    for (answer, processes) in [
        (Err(ProcessError::timed_out()), 2),
        (exit(None, "", ""), 2),
        (exit(Some(1), "", ""), 2),
        (Err(ProcessError::NotExecutable), 1),
    ] {
        let runner = Arc::new(FakeRunner::default().answering("--version", answer));
        let wait = Arc::new(RecordedWait::default());
        let seam = OpenSpec::new(
            runner.clone(),
            Arc::new(FakeProbe::default().with_executable(CLI)),
            env(),
        )
        .with_wait(wait.clone());
        assert!(matches!(seam.cli_state(&Silent, buffer()), OpenSpecCliState::Failed { .. }));
        assert_eq!(runner.calls_of("--version"), processes);
        assert_eq!(wait.delays().len(), processes - 1);
    }
    // A check whose second process answers is `available`.
    let runner = Arc::new(
        FakeRunner::default()
            .then("--version", vec![Err(ProcessError::timed_out())])
            .json("--version", "1.17.0\n"),
    );
    let seam =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env());
    assert!(seam.cli_state(&Silent, buffer()).is_available());
    assert_eq!(runner.calls_of("--version"), 2);
}

// OFP-FR-ITUD, OSM-FR-NQBE, OSM-FR-YXUC: a re-check of the author clears a
// kept `failed` result at once; a kept `available` or `unsupported` result
// stays kept.
#[test]
fn a_recheck_clears_a_failed_check_and_keeps_the_others() {
    let (_dir, root) = openspec_project();
    let runner =
        Arc::new(FakeRunner::default().answering("--version", Err(ProcessError::timed_out())));
    // A kept `failed` check stands for an hour here, so only the re-check can
    // clear it, also on a slow runner.
    let seam =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env())
            .with_failed_ttl(std::time::Duration::from_secs(3600));
    seam.status(&Silent, buffer(), Some(&root));
    assert_eq!(runner.calls_of("--version"), 2);
    seam.status(&Silent, buffer(), Some(&root));
    assert_eq!(runner.calls_of("--version"), 2, "the failure is kept");
    runner.set("--version", Ok(ok("1.17.0\n")));
    let status = seam.status_checked(&Silent, buffer(), Some(&root), true);
    assert!(status.cli.is_available(), "the re-check ran a new check");
    assert_eq!(runner.calls_of("--version"), 3);
    seam.status_checked(&Silent, buffer(), Some(&root), true);
    assert_eq!(runner.calls_of("--version"), 3, "a usable result stays kept");

    let old = Arc::new(FakeRunner::default().json("--version", "0.9.0\n"));
    let seam =
        OpenSpec::new(old.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env());
    seam.cli_state(&Silent, buffer());
    seam.clear_failed_checks(&Silent, buffer());
    assert!(matches!(seam.kept_cli_state(), Some(OpenSpecCliState::Unsupported { .. })));
    assert_eq!(old.calls_of("--version"), 1);
}

// OFP-FR-WNVO: a transient failure whose standard error is empty carries no
// diagnostic.
#[test]
fn empty_standard_error_adds_no_diagnostic() {
    let (_dir, root) = openspec_project();
    let (seam, _, _) =
        waiting_seam(FakeRunner::default().answering("list", Err(ProcessError::timed_out())));
    let error = seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap_err();
    assert_eq!(error, OpenSpecReadError::CliTimedOut { diagnostics: Vec::new() });
}

// OFP-FR-DGSC, OFP-FR-PQNS, OFP-FR-ZQNU: a read whose process does not start
// is `cli_failed` with no exit status, and it is transient: the process runs
// once more after one wait of 2 seconds, and the record of the retry names
// `cli_failed`.
#[test]
fn a_read_whose_process_does_not_start_runs_once_more() {
    let (_dir, root) = openspec_project();
    let (seam, runner, wait) =
        waiting_seam(FakeRunner::default().answering("list", Err(ProcessError::NotExecutable)));
    let log = buffer();
    let error = seam.list_capabilities(&Silent, log, Some(&root)).unwrap_err();
    assert_eq!(error, OpenSpecReadError::CliFailed { exit_status: None, diagnostics: Vec::new() });
    assert_eq!(runner.calls_of("list"), 2, "two processes, no more");
    assert_eq!(wait.delays(), vec![2_000]);
    let all = records(log);
    let retry = all.iter().find(|r| r.message == "openspec cli runs again").expect("a record");
    assert_eq!(retry.fields["subcommand"], "list");
    assert_eq!(retry.fields["code"], "cli_failed");

    // A second process that starts makes the read succeed.
    let (seam, runner, _) = waiting_seam(
        FakeRunner::default()
            .then("list", vec![Err(ProcessError::NotExecutable)])
            .json("list", SPECS),
    );
    assert_eq!(seam.list_capabilities(&Silent, buffer(), Some(&root)).unwrap().len(), 1);
    assert_eq!(runner.calls_of("list"), 2);
}

// OFP-FR-DGSC, OFP-FR-FETF, OSM-FR-ZNVZ, OFP-FR-ZQNU: the version check treats
// a process that does not start as `failed` with no retry: one process, no
// wait, and no retry record. A check whose process times out runs once more,
// and its retry record names the subcommand `--version`.
#[test]
fn the_version_check_does_not_run_a_process_that_does_not_start_again() {
    let runner =
        Arc::new(FakeRunner::default().answering("--version", Err(ProcessError::NotExecutable)));
    let wait = Arc::new(RecordedWait::default());
    let seam =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env())
            .with_wait(wait.clone());
    let log = buffer();
    let state = seam.cli_state(&Silent, log);
    assert!(
        matches!(state, OpenSpecCliState::Failed { reason: CliFailure::NotExecutable, .. }),
        "{state:?}"
    );
    assert_eq!(state.code(), "failed");
    assert_eq!(runner.calls_of("--version"), 1);
    assert!(wait.delays().is_empty());
    assert!(records(log).iter().all(|r| r.message != "openspec cli runs again"));

    let runner =
        Arc::new(FakeRunner::default().answering("--version", Err(ProcessError::timed_out())));
    let seam =
        OpenSpec::new(runner.clone(), Arc::new(FakeProbe::default().with_executable(CLI)), env());
    let log = buffer();
    seam.cli_state(&Silent, log);
    let all = records(log);
    let retry = all.iter().find(|r| r.message == "openspec cli runs again").expect("a record");
    assert_eq!(retry.level, LogLevel::Warn);
    assert_eq!(retry.fields["subcommand"], "--version");
    assert_eq!(retry.fields["code"], "cli_timed_out");
    assert_eq!(retry.fields["attempt"], 2);
    assert_eq!(retry.fields["delayMs"], 2_000);
}
