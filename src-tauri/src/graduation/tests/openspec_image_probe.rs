//! The image probe of an OpenSpec run (`OFP-openspec-failure-policy.md`
//! OFP-FR-BZDP through OFP-FR-LXYP), driven through the probe seam of the
//! scripted dispatch: no container runtime, no image, and no real wait.

use super::change_runs::*;
use super::*;
use crate::tools::agent_exec::image_probe::{ImageProbe, ImageProbeOutcome};
use crate::tools::agent_exec::runtime::RunEnd;
use crate::tools::agent_exec::AgentExecutionError;

/// An apply run that waits in its queue.
fn queued_apply(fx: &Fixture) -> GraduationRun {
    let stream = change_stream(fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(fx, &stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    run
}

fn backend(message: &str) -> Vec<crate::logging::LogRecord> {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    let records = crate::logging::BUFFER.query(&filter, None, 2000).unwrap().records;
    records.into_iter().filter(|r| r.message == message).collect()
}

// OFP-FR-BZDP, OFP-FR-XBNJ, OFP-FR-WACS, OFP-FR-JBSO, OFP-FR-LXYP: a probe
// that passes runs before the first turn, records the version in the
// checkpoint, spends no pass and no turn time, and logs the run, the
// version, and the duration.
#[test]
fn a_passing_probe_records_the_version_before_the_first_turn() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    dispatch.script_probes(vec![probe_answer(Some(0), "1.17.0\n")]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    assert_eq!(dispatch.probes_taken(), 1);
    assert_eq!(done.checkpoint.openspec_image_cli.as_ref().unwrap().version, "1.17.0");
    let wire = serde_json::to_value(&done.checkpoint).unwrap();
    assert_eq!(wire["openspecImageCli"]["version"], "1.17.0");
    let passed: Vec<_> = backend("openspec image probe passed")
        .into_iter()
        .filter(|r| r.fields["run_id"] == run.id.as_str())
        .collect();
    assert_eq!(passed.len(), 1);
    assert_eq!(passed[0].level, crate::logging::LogLevel::Info);
    assert_eq!(passed[0].fields["run_id"], run.id.as_str());
    assert_eq!(passed[0].fields["version"], "1.17.0");
    assert!(passed[0].fields["durationMs"].is_u64());
}

// OFP-FR-XBNJ, OFP-FR-TBHX, GRD-FR-UJII, OFP-FR-WACS, OFP-FR-LXYP: an image
// with no `openspec` executable rests the run `blocked` with
// `openspec_image_cli_unavailable` and the state `not_found` before any turn.
// The probe records nothing, and Continue probes again before it dispatches.
#[test]
fn an_image_with_no_cli_rests_the_run_and_continue_probes_again() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    dispatch.script_probes(vec![probe_answer(Some(127), "")]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked, "{blocked:?}");
    let blocker = blocked.blocker.as_ref().unwrap();
    assert_eq!(blocker.code, "openspec_image_cli_unavailable");
    assert_eq!(blocker.detail.as_deref(), Some("not_found"));
    assert!(blocker.diagnostics.is_empty());
    assert_eq!(dispatch.turns_taken(), 0, "no turn is dispatched");
    assert!(blocked.checkpoint.openspec_image_cli.is_none());
    assert_eq!(blocked.pass(), run.pass(), "the probe spends no pass");
    assert_eq!(blocked.turn_time_used_ms, 0, "the probe spends no turn time");
    let failed = backend("openspec image probe failed");
    assert_eq!(failed[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(failed[0].fields["run_id"], run.id.as_str());
    assert_eq!(failed[0].fields["code"], "openspec_image_cli_unavailable");
    assert_eq!(failed[0].fields["state"], "not_found");
    let capacity = crate::graduation::get_graduation_capacity(fx.app.clone()).unwrap();
    assert_eq!(capacity.in_use, 1, "the blocked run keeps its project slot");

    let resumed = fx.continue_run(&blocked);
    let done = fx.drive(&resumed, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 2, "Continue probes again");
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
}

// OFP-FR-XBNJ: a version below 1 is `unsupported`. An exit 0 with no
// version, an exit 1, and an exit 2 are other outcomes of the CLI: `failed`,
// with no retry.
#[test]
fn an_old_cli_is_unsupported_and_every_other_cli_outcome_is_failed() {
    for (status, stdout, state) in [
        (0, "0.9.0\n", "unsupported"),
        (0, "hello\n", "failed"),
        (1, "", "failed"),
        (2, "", "failed"),
    ] {
        let (fx, _, _) = change_fixture(ALL_DONE);
        let run = queued_apply(&fx);
        let dispatch = ScriptedDispatch::new(vec![Turn::work()]);
        dispatch.script_probes(vec![probe_answer(Some(status), stdout)]);
        let blocked = fx.drive(&run, dispatch.clone());
        assert_eq!(blocked.blocker.as_ref().unwrap().detail.as_deref(), Some(state));
        assert_eq!(dispatch.probes_taken(), 1, "{state}: a final outcome runs no retry");
    }
}

// OFP-FR-BZDP, OFP-FR-PQNS, OFP-FR-MJZU, OFP-FR-ZQNU: a probe whose outcome
// is transient runs once more after one wait of 2 seconds through the wait
// seam, and the record of the retry names the code of the first outcome. An
// outcome after the retry that is not transient rests the run with `failed`.
#[test]
fn a_transient_probe_runs_once_more() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let timed_out = probe_ended(RunEnd::TimedOut, None, "");
    dispatch.script_probes(vec![timed_out.clone(), probe_answer(Some(0), "1.17.0\n")]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 2);
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    assert_eq!(waits(&fx), vec![2_000], "one wait of 2 seconds");
    let retry = backend("openspec cli runs again");
    assert_eq!(retry.len(), 1);
    assert_eq!(retry[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(retry[0].fields["subcommand"], "image_probe");
    assert_eq!(retry[0].fields["code"], "cli_timed_out");
    assert_eq!(retry[0].fields["attempt"], 2);
    assert_eq!(retry[0].fields["delayMs"], 2_000);

    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work()]);
    dispatch.script_probes(vec![timed_out, probe_answer(Some(1), "")]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 2);
    assert_eq!(blocked.blocker.as_ref().unwrap().detail.as_deref(), Some("failed"));
}

// OFP-FR-BZDP: a draft run is no OpenSpec run and runs no probe.
#[test]
fn only_an_openspec_run_runs_the_probe() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add a logout button.");
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 0);
}

// OFP-FR-WACS: a run whose checkpoint holds the version probes no more.
#[test]
fn a_recorded_version_is_not_probed_again() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let mut run = queued_apply(&fx);
    run.checkpoint.openspec_image_cli =
        Some(crate::graduation::OpenSpecImageCli { version: "1.17.0".into() });
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 0);
}

// OFP-FR-XBNJ, OFP-FR-PQNS: a probe that ends by a signal, and a probe whose
// container does not start, run once more.
#[test]
fn a_signal_and_a_container_that_does_not_start_run_once_more() {
    let unstarted: ProbeAnswer = Err(AgentExecutionError::LaunchFailed("no start".into()));
    for first in [probe_ended(RunEnd::Terminated, None, ""), unstarted] {
        let (fx, _, _) = change_fixture(ALL_DONE);
        let run = queued_apply(&fx);
        let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
        dispatch.script_probes(vec![first.clone(), probe_answer(Some(0), "1.17.0\n")]);
        let done = fx.drive(&run, dispatch.clone());
        assert_eq!(dispatch.probes_taken(), 2, "{first:?}");
        assert_eq!(done.state, GraduationRunState::Completed, "{first:?}: {done:?}");
        assert_eq!(waits(&fx), vec![2_000], "{first:?}");
        // OFP-FR-ZQNU: the code of the first outcome, the last record so far.
        let retry = backend("openspec cli runs again").pop().expect("a retry record");
        assert_eq!(retry.fields["code"], "cli_failed", "{first:?}");
    }
}

// OFP-FR-PQNS, OFP-FR-XBNJ: a second transient outcome rests the run with
// `failed`; no process runs a third time.
#[test]
fn two_transient_probe_outcomes_rest_the_run_with_failed() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let timed_out = probe_ended(RunEnd::TimedOut, None, "");
    dispatch.script_probes(vec![timed_out.clone(), timed_out, probe_answer(Some(0), "1.17.0\n")]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked, "{blocked:?}");
    assert_eq!(dispatch.probes_taken(), 2, "no third probe");
    let blocker = blocked.blocker.as_ref().unwrap();
    assert_eq!(blocker.code, "openspec_image_cli_unavailable");
    assert_eq!(blocker.detail.as_deref(), Some("failed"));
    assert_eq!(dispatch.turns_taken(), 0);
    assert_eq!(waits(&fx), vec![2_000], "one wait");
}

// OFP-FR-BZDP, OFP-FR-WACS: a propose run and a rework run probe their image
// before the first work turn, and record the version.
#[test]
fn a_propose_run_and_a_rework_run_probe_their_image() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = fx.stream("editor");
    let mut propose = fx.enqueue(&stream, "Add a logout button.");
    propose.openspec = Some(crate::graduation::openspec_start::propose_data(CHANGE));
    crate::graduation::save_run(&fx.app, &mut propose).unwrap();
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    dispatch.script_probes(vec![probe_answer(Some(0), "1.18.0\n")]);
    let done = fx.drive(&propose, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 1, "{done:?}");
    assert_eq!(done.checkpoint.openspec_image_cli.as_ref().unwrap().version, "1.18.0");

    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let rework = start_rework(&fx, &stream, "Drop the second task.").expect("a rework start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    dispatch.script_probes(vec![probe_answer(Some(127), "")]);
    let blocked = fx.drive(&rework.run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 1);
    assert_eq!(dispatch.turns_taken(), 0, "the probe comes before the first work turn");
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "openspec_image_cli_unavailable");
}

// OFP-FR-BZDP: a PR summary run in a project in OpenSpec mode runs no probe.
#[test]
fn a_pr_summary_run_runs_no_probe() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let (run, _, _) = super::pr_summary_turn::summary_run(&fx, PrSummaryMode::Diff);
    let dispatch = ScriptedDispatch::new(vec![super::pr_summary_turn::summary_answer(
        "Add the panel",
        "Adds it.",
    )]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    assert_eq!(dispatch.probes_taken(), 0);
    assert!(done.checkpoint.openspec_image_cli.is_none());
}

// OFP-FR-XBNJ: an error of the runtime or of the image setting is not a probe
// outcome. The probe records nothing, runs no retry, and the dispatch goes
// on, so the turn states the error on its own terms. Continue probes again.
#[test]
fn a_runtime_or_image_setting_error_is_no_probe_outcome() {
    for error in [
        AgentExecutionError::RuntimeUnavailable,
        AgentExecutionError::VendorImageUnresolved("vendor_image_unconfigured".into()),
        AgentExecutionError::ImageUnavailable("claude_code".into()),
    ] {
        let (fx, _, _) = change_fixture(ALL_DONE);
        let run = queued_apply(&fx);
        let dispatch =
            ScriptedDispatch::new(vec![Turn::answering(Answer::Unlaunchable(error.clone()))]);
        dispatch.script_probes(vec![Err(error.clone())]);
        let stopped = fx.drive(&run, dispatch.clone());
        assert_eq!(dispatch.probes_taken(), 1, "{error:?}: no retry");
        assert_eq!(dispatch.turns_taken(), 1, "{error:?}: the dispatch goes on");
        assert_eq!(stopped.state, GraduationRunState::Interrupted, "{error:?}: {stopped:?}");
        assert!(
            stopped.blocker.as_ref().is_none_or(|b| b.code != "openspec_image_cli_unavailable"),
            "{error:?}: no image blocker"
        );
        assert!(stopped.checkpoint.openspec_image_cli.is_none(), "{error:?}: nothing recorded");
        let not_run = backend("openspec image probe not run");
        let ours: Vec<_> =
            not_run.iter().filter(|r| r.fields["run_id"] == run.id.as_str()).collect();
        assert_eq!(ours.len(), 1, "{error:?}");
        assert_eq!(ours[0].level, crate::logging::LogLevel::Warn);
        assert_eq!(ours[0].fields["code"], error.kind());
    }
}

// OFP-FR-RKMB: under Microsandbox the probe is skipped. The dispatch goes on,
// the run records no version and no blocker, and one `WARN` record names the
// run and the code `image_probe_skipped`.
#[test]
fn a_skipped_probe_lets_the_dispatch_go_on_and_records_nothing() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let run = queued_apply(&fx);
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    dispatch.script_probes(vec![Ok(ImageProbeOutcome::Skipped)]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(dispatch.probes_taken(), 1, "a skip runs no retry");
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    assert!(done.checkpoint.openspec_image_cli.is_none(), "a skip records no version");
    assert!(done.blocker.is_none());
    let skipped: Vec<_> = backend("openspec image probe skipped")
        .into_iter()
        .filter(|r| r.fields["run_id"] == run.id.as_str())
        .collect();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(skipped[0].fields["code"], "image_probe_skipped");
    assert!(backend("openspec image probe failed")
        .iter()
        .all(|r| r.fields["run_id"] != run.id.as_str()));
}

// OFP-FR-XBNJ, OFP-FR-RKMB, OFP-FR-PQNS: what one probe outcome means.
#[test]
fn the_verdict_of_a_probe_outcome() {
    use crate::graduation::driver::image_probe::{verdict_of, ProbeVerdict};
    let ended = |end, status| {
        Ok(ImageProbeOutcome::Ran(ImageProbe {
            end,
            exit_status: status,
            stdout: b"1.2.3".to_vec(),
            stderr: Vec::new(),
        }))
    };
    assert_eq!(verdict_of(&ended(RunEnd::Exited, Some(0))), ProbeVerdict::Usable("1.2.3".into()));
    assert_eq!(
        verdict_of(&ended(RunEnd::EndedAfterFinalEvent, Some(0))),
        ProbeVerdict::Usable("1.2.3".into())
    );
    assert_eq!(verdict_of(&ended(RunEnd::Exited, Some(127))), ProbeVerdict::Unusable("not_found"));
    assert_eq!(verdict_of(&ended(RunEnd::Exited, Some(1))), ProbeVerdict::Unusable("failed"));
    assert_eq!(verdict_of(&ended(RunEnd::Exited, Some(2))), ProbeVerdict::Unusable("failed"));
    assert_eq!(verdict_of(&ended(RunEnd::Terminated, None)), ProbeVerdict::Transient("cli_failed"));
    assert_eq!(verdict_of(&ended(RunEnd::Exited, None)), ProbeVerdict::Transient("cli_failed"));
    assert_eq!(
        verdict_of(&ended(RunEnd::TimedOut, None)),
        ProbeVerdict::Transient("cli_timed_out")
    );
    assert_eq!(
        verdict_of(&ended(RunEnd::TimedOut, Some(127))),
        ProbeVerdict::Transient("cli_timed_out")
    );
    assert_eq!(verdict_of(&ended(RunEnd::Cancelled, None)), ProbeVerdict::Cancelled);
    assert_eq!(verdict_of(&Ok(ImageProbeOutcome::Skipped)), ProbeVerdict::Skipped);
    assert_eq!(
        verdict_of(&Err(AgentExecutionError::LaunchFailed("x".into()))),
        ProbeVerdict::Transient("cli_failed")
    );
    assert_eq!(
        verdict_of(&Err(AgentExecutionError::RuntimeUnavailable)),
        ProbeVerdict::NotAnOutcome("runtime_unavailable")
    );
    assert_eq!(
        verdict_of(&Err(AgentExecutionError::VendorImageUnresolved("x".into()))),
        ProbeVerdict::NotAnOutcome("vendor_image_unresolved")
    );
}
