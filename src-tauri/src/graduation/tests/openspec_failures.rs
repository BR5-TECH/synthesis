//! The OpenSpec failures of the starts, the commands, and the review gates
//! (`OFP-openspec-failure-policy.md`), with the OpenSpec CLI faked.

use super::change_runs::*;
use super::*;
use crate::openspec::tests::ok;
use crate::openspec::{ProcessError, ProcessOutput};

/// A change that cannot be applied, with the CLI's warnings and instruction.
const NOT_READY: &str = r#"{ "changeName": "add-logout-3f2a", "progress": { "total": 0, "complete": 0 },
  "tasks": [], "state": "blocked", "missingArtifacts": ["design", "tasks"],
  "warnings": ["The proposal names no capability"], "instruction": "Write the design first." }"#;

fn rejected(message: &str) -> Result<ProcessOutput, ProcessError> {
    Ok(ProcessOutput {
        exit_status: Some(1),
        stdout: format!(
            r#"{{ "status": [ {{ "severity": "error", "code": "c1", "message": "{message}" }} ] }}"#
        )
        .into_bytes(),
        stderr: Vec::new(),
    })
}

fn start_apply_raw(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
) -> Result<GraduationRun, crate::openspec::Refusal> {
    crate::graduation::start_change_apply(fx.app.clone(), stream.id.clone(), CHANGE.into())
}

fn comment_files(fx: &Fixture) -> Vec<String> {
    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    super::tree_under(store.path()).into_keys().filter(|p| p.contains("/comments/")).collect()
}

// OFP-FR-SJML, OFP-FR-BWML, OFP-FR-QLQJ, OFP-FR-EOAD, OFP-FR-YKGQ,
// OCR-FR-LNKQ: an apply start refused with `change_not_ready` rejects with one
// `OpenSpecFailure`: the artifacts as its detail and the diagnostics of
// `instructions apply`. It posts no comment, starts nothing, and changes no
// state.
#[test]
fn a_change_that_is_not_ready_refuses_with_its_diagnostics() {
    let (fx, _, _) = change_fixture(NOT_READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let refusal = start_apply_raw(&fx, &stream).unwrap_err();
    let failure = refusal.openspec().expect("an OpenSpec failure");
    assert_eq!(failure.code, "change_not_ready");
    assert_eq!(failure.detail.as_deref(), Some("design,tasks"));
    assert_eq!(failure.change.as_deref(), Some(CHANGE));
    let severities: Vec<_> =
        failure.diagnostics.iter().map(|d| d.severity.clone().unwrap_or_default()).collect();
    assert_eq!(severities, ["error", "error", "warning", "info"]);
    assert!(failure.diagnostics[0].message.contains("design"));
    assert_eq!(failure.diagnostics[3].message, "Write the design first.");
    let wire = serde_json::to_value(&refusal).unwrap();
    assert_eq!(wire["code"], "change_not_ready");
    assert_eq!(wire["detail"], "design,tasks");
    assert_eq!(wire["diagnostics"].as_array().unwrap().len(), 4);
    assert!(crate::graduation::queue::project_queue(&fx.app).unwrap().runs.is_empty());
    assert!(comment_files(&fx).is_empty(), "no comment is posted");
}

// OFP-FR-QLQJ, OFP-FR-ZDEE, OFP-FR-NJSO, OCR-FR-LNKQ: a progress read that the
// CLI rejects refuses the start with `apply_progress_unavailable`, the read's
// code as its cause, and the CLI's `status` entries.
#[test]
fn a_rejected_progress_read_refuses_with_its_cause() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    fx.allow_execution();
    let stream = change_stream(&fx, "editor");
    runner.set("instructions", rejected("Unknown schema"));
    let refusal = start_apply_raw(&fx, &stream).unwrap_err();
    let failure = refusal.openspec().expect("an OpenSpec failure");
    assert_eq!(failure.code, "apply_progress_unavailable");
    assert_eq!(failure.cause.as_deref(), Some("cli_rejected"));
    assert_eq!(failure.diagnostics[0].message, "Unknown schema");
    assert_eq!(runner.calls_of("instructions"), 1, "a rejection runs nothing again");
}

// OFP-FR-QLQJ, OFP-FR-GPCO, GSU-FR-MTDA: a propose start with no usable CLI
// rejects with `cli_unavailable` and the CLI state as an object.
#[test]
fn a_propose_start_rejects_cli_unavailable_as_an_object() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.root().join("openspec")).unwrap();
    std::fs::write(fx.root().join("openspec/config.yaml"), "schema: spec-driven\n").unwrap();
    fx.app.manage(crate::openspec::OpenSpec::new(
        Arc::new(crate::openspec::tests::FakeRunner::default()),
        Arc::new(crate::openspec::tests::FakeProbe::default()),
        crate::openspec::tests::env(),
    ));
    let stream = fx.stream("editor");
    let refused = crate::graduation::start_graduation(
        fx.app.clone(),
        fx.draft("logout"),
        stream.id.clone(),
        StandingWork::Keep,
        None,
        Some(CHANGE.into()),
    )
    .unwrap_err();
    assert_eq!(
        serde_json::to_value(&refused).unwrap(),
        serde_json::json!({ "code": "cli_unavailable", "detail": "not_found", "diagnostics": [] })
    );
    // A refusal that is no OpenSpec failure keeps its own form.
    let other = crate::graduation::start_graduation(
        fx.app.clone(),
        fx.draft("other"),
        "no-such-stream".into(),
        StandingWork::Keep,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(serde_json::to_value(&other).unwrap(), serde_json::json!("unknown_stream"));
}

// OFP-FR-QLQJ, OSM-FR-PEAK: `"read apply progress"` rejects a rejected read
// with the object and its diagnostics.
#[test]
fn the_progress_command_rejects_with_the_object() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    runner.set("instructions", rejected("Bad tasks"));
    let failure = tauri::async_runtime::block_on(crate::openspec::apply::read_apply_progress(
        fx.app.clone(),
        stream.id.clone(),
        CHANGE.to_string(),
    ))
    .unwrap_err();
    assert_eq!(failure.code, "cli_rejected");
    assert_eq!(failure.diagnostics[0].message, "Bad tasks");
}

// OFP-FR-ITUD, OSM-FR-YXUC: `get_openspec_status` with `recheck` clears a
// kept `failed` check before it checks, and without it the failure stands.
#[test]
fn the_status_command_with_recheck_checks_again() {
    let (fx, runner, probe) = change_fixture(ALL_DONE);
    probe.touch(crate::openspec::tests::CLI, std::time::SystemTime::UNIX_EPOCH);
    runner.set("--version", Err(ProcessError::timed_out()));
    let failed = crate::openspec::status_now(&fx.app, false);
    assert_eq!(failed.cli.code(), "failed");
    runner.set("--version", Ok(ok("1.17.0\n")));
    assert_eq!(crate::openspec::status_now(&fx.app, false).cli.code(), "failed", "kept");
    assert_eq!(crate::openspec::status_now(&fx.app, true).cli.code(), "available");
}

// OFP-FR-TBHX, OFP-FR-UVPV, OFP-FR-WRDX, GRL-FR-KDYL, GRD-FR-UJII: an apply
// gate read that the CLI rejects dispatches no review turn and rests the run
// `blocked` with `openspec_validation_unavailable`, the cause `cli_rejected`,
// and the diagnostics on the blocker; the run log holds no diagnostic.
#[test]
fn a_rejected_gate_read_rests_the_run_with_its_diagnostics() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(&fx, &stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let flip = runner.clone();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().doing(move || flip.set("instructions", rejected("SECRET-DIAGNOSTIC")))
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Blocked, "{run:?}");
    let blocker = run.blocker.as_ref().unwrap();
    assert_eq!(blocker.code, "openspec_validation_unavailable");
    assert_eq!(blocker.cause.as_deref(), Some("cli_rejected"));
    assert_eq!(blocker.diagnostics[0].message, "SECRET-DIAGNOSTIC");
    assert!(dispatch.of_part("review").is_empty());
    let wire = serde_json::to_value(blocker).unwrap();
    assert_eq!(wire["cause"], "cli_rejected");
    assert_eq!(wire["diagnostics"][0]["message"], "SECRET-DIAGNOSTIC");
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    let records = crate::logging::BUFFER.query(&filter, None, 2000).unwrap().records;
    let logged = serde_json::to_string(&records).unwrap();
    assert!(!logged.contains("SECRET-DIAGNOSTIC"), "a record holds a diagnostic");
}

// OFP-FR-ITUD, OFP-FR-TBHX, GRD-FR-UJII: Continue of a run that rests on an
// OpenSpec failure clears a kept `failed` CLI check, so the gate runs again
// on a fresh check and the review is dispatched.
#[test]
fn continue_clears_a_failed_check_and_runs_the_gate_again() {
    let (fx, runner, probe) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(&fx, &stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let (flip, touched) = (runner.clone(), probe.clone());
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().doing(move || {
            // The CLI changes, and its new check fails.
            touched.touch(crate::openspec::tests::CLI, std::time::SystemTime::UNIX_EPOCH);
            flip.set("--version", Err(ProcessError::timed_out()));
        }),
        Turn::ready(),
    ]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked, "{blocked:?}");
    assert_eq!(blocked.blocker.as_ref().unwrap().cause.as_deref(), Some("cli_unavailable"));
    assert_eq!(blocked.blocker.as_ref().unwrap().detail.as_deref(), Some("failed"));
    runner.set("--version", Ok(ok("1.17.0\n")));
    let checks = runner.calls_of("--version");
    let resumed = fx.continue_run(&blocked);
    let done = fx.drive(&resumed, dispatch.clone());
    assert!(runner.calls_of("--version") > checks, "Continue checked the CLI again");
    assert_eq!(dispatch.of_part("review").len(), 1, "{done:?}");
}

/// A work turn that turns OpenSpec mode off for the project, as the author
/// does while the run works.
fn turning_the_mode_off(fx: &Fixture) -> Turn {
    let config = fx.root().join("openspec/config.yaml");
    Turn::work().doing(move || {
        let _ = std::fs::remove_file(&config);
    })
}

/// OFP-FR-NQPY, OFP-FR-TBHX: the gate of the review rests the run `blocked`
/// with the cause `openspec_mode_inactive`, and no review turn is dispatched.
fn assert_rests_on_the_mode(run: &GraduationRun, dispatch: &ScriptedDispatch) {
    assert_eq!(run.state, GraduationRunState::Blocked, "{run:?}");
    let blocker = run.blocker.as_ref().unwrap();
    assert_eq!(blocker.code, "openspec_validation_unavailable");
    assert_eq!(blocker.cause.as_deref(), Some("openspec_mode_inactive"));
    assert!(blocker.diagnostics.is_empty());
    assert!(dispatch.of_part("review").is_empty(), "the gate is not skipped");
}

// OFP-FR-NQPY, OFP-FR-TBHX, GRL-FR-KDYL: an apply run whose mode turns off
// before its review gate continues to the gate, and the gate rests the run.
#[test]
fn a_mode_that_turns_off_before_the_apply_gate_rests_the_run() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(&fx, &stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let dispatch = ScriptedDispatch::new(vec![turning_the_mode_off(&fx), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_rests_on_the_mode(&run, &dispatch);
}

// OFP-FR-NQPY, OFP-FR-TBHX, GRL-FR-KDYL: the same for the gate of a rework
// run.
#[test]
fn a_mode_that_turns_off_before_the_rework_gate_rests_the_run() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let started = start_rework(&fx, &stream, "Drop the second task.").expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let dispatch = ScriptedDispatch::new(vec![turning_the_mode_off(&fx), Turn::ready()]);
    let run = fx.drive(&started.run, dispatch.clone());
    assert_rests_on_the_mode(&run, &dispatch);
}

// OFP-FR-NQPY, OPU-FR-PXDY, OFP-FR-QLQJ: Continue from a plan rest after the
// mode turned off refuses with one `OpenSpecFailure` whose cause is
// `openspec_mode_inactive`, and changes nothing of the run.
#[test]
fn a_mode_that_turns_off_refuses_continue_from_a_plan_rest() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = super::change_plan_continue::rested_on_findings(&fx, &stream);
    std::fs::remove_file(fx.root().join("openspec/config.yaml")).unwrap();
    let before = fx.reload(&rested.id);
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let refused =
        crate::graduation::continue_graduation_run(fx.app.clone(), rested.id.clone()).unwrap_err();
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    assert_object(&refused, "openspec_validation_unavailable");
    let failure = refused.openspec().unwrap();
    assert_eq!(failure.cause.as_deref(), Some("openspec_mode_inactive"));
    assert_eq!(failure.change.as_deref(), Some(CHANGE));
    let after = fx.reload(&rested.id);
    assert_eq!(after.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(after.rest, before.rest, "a refusal changes nothing");
    assert_eq!(after.checkpoint.decisions, before.checkpoint.decisions);
}
