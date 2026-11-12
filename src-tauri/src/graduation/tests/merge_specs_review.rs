//! The OpenSpec gate of a merge review (`GRL-graduation-loop.md` GRL-FR-ZDNL,
//! GRL-FR-ZXOE, GRL-FR-ZFMZ, GRL-FR-AGOA; `GRD-graduation.md` GRD-FR-UJII),
//! driven with the agent scripted and the OpenSpec CLI faked.

use super::archive_support::*;
use super::*;
use crate::openspec::tests::ok;
use crate::openspec::ProcessOutput;
use crate::streams::{StreamMergePublication, StreamMergeResult};

const INVALID_SPECS: &str = r#"{ "items": [
  { "id": "auth", "type": "spec", "valid": false,
    "issues": [ { "level": "ERROR", "path": "requirements[0]", "message": "Duplicate requirement" } ] }
] }"#;
const VALID_SPECS: &str =
    r#"{ "items": [ { "id": "auth", "type": "spec", "valid": true, "issues": [] } ] }"#;

/// A conflict merge handed to a run in OpenSpec mode, with `validate`
/// answering `validation`.
fn openspec_merge_run(
    validation: Result<ProcessOutput, crate::openspec::ProcessError>,
) -> (Fixture, std::sync::Arc<ArchivingRunner>, GraduationRun) {
    let (fx, runner, run, _) = openspec_merge_run_probed(validation);
    (fx, runner, run)
}

/// The same, with the fake probe of the CLI.
fn openspec_merge_run_probed(
    validation: Result<ProcessOutput, crate::openspec::ProcessError>,
) -> (
    Fixture,
    std::sync::Arc<ArchivingRunner>,
    GraduationRun,
    std::sync::Arc<crate::openspec::tests::FakeProbe>,
) {
    let (fx, runner, probe) = archive_fixture(ALL_DONE);
    runner.inner.set("validate", validation);
    fx.allow_execution();
    let stream = fx.stream("feature");
    std::fs::write(stream.worktree().join("README.md"), "stream side\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "stream edit");
    std::fs::write(fx.root().join("README.md"), "base side\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "base edit");
    let result = merge(&fx, &stream, StreamMergePublication::Uncommitted).expect("handed off");
    let StreamMergeResult::Conflicted { run_id, .. } = result else { panic!("{result:?}") };
    let run = fx.reload(&run_id);
    (fx, runner, run, probe)
}

// GRL-FR-ZDNL, GRL-FR-ZFMZ, GRL-FR-MRVK, OPU-FR-EPXL: each merge review in
// OpenSpec mode reads `validate --specs --strict` of its checkout as
// `openspec_specs_validation`; a `ready` over invalid specs is replaced by a
// `revise` of one critical finding that names the capability, and the next
// pass runs; a `ready` over valid specs applies the merge.
#[test]
fn a_ready_merge_review_over_invalid_specs_is_not_applied() {
    let (fx, runner, run) = openspec_merge_run(Ok(ProcessOutput {
        exit_status: Some(1),
        stdout: INVALID_SPECS.into(),
    }));
    let flip = runner.clone();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready().doing(move || flip.inner.set("validate", Ok(ok(VALID_SPECS)))),
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let reviews = dispatch.of_part("merge_review");
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0].input["openspec_specs_validation"]["valid"], false);
    assert_eq!(reviews[0].input["openspec_specs_validation"]["items"][0]["id"], "auth");
    assert_eq!(reviews[1].input["openspec_specs_validation"]["valid"], true);
    let first = &done.observability.passes[0];
    assert_eq!(first.verdict, Some(ReviewOutcome::Revise));
    assert_eq!(first.findings.len(), 1);
    assert_eq!(first.findings[0].severity, ReviewSeverity::Critical);
    assert_eq!(first.findings[0].affected_files, ["openspec/specs/auth/spec.md"]);
    let validations: Vec<_> =
        runner.inner.calls().into_iter().filter(|c| c.args[0] == "validate").collect();
    assert_eq!(validations.len(), 2, "one validation before each review turn");
    assert_eq!(
        validations[0].args,
        ["validate", "--specs", "--strict", "--json", "--no-interactive"]
    );
    // Each validation runs in the review checkout of its own pass.
    assert_eq!(validations[0].cwd, reviews[0].directory);
    assert_eq!(validations[1].cwd, reviews[1].directory);
    assert_ne!(reviews[0].directory, reviews[1].directory);
    // GRL-FR-ZXOE: the review is told what the field is and what it means.
    assert!(reviews[0].instruction.contains("input.openspec_specs_validation"));
    assert!(reviews[0].instruction.contains("A specification that is not valid is a finding"));
}

// GRL-FR-AGOA, GRD-FR-UJII: a specs validation that cannot run dispatches no
// review turn and blocks the run with `openspec_validation_unavailable`; the
// blocked run keeps its slot, and Continue validates and reviews again.
#[test]
fn a_specs_validation_that_cannot_run_blocks_the_merge_run() {
    let (fx, runner, run) =
        openspec_merge_run(Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new() }));
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Blocked, "{run:?}");
    assert_eq!(run.blocker.as_ref().unwrap().code, "openspec_validation_unavailable");
    assert!(dispatch.of_part("merge_review").is_empty(), "no review turn is dispatched");
    let capacity = crate::graduation::get_graduation_capacity(fx.app.clone()).unwrap();
    assert_eq!(capacity.in_use, 1, "the blocked run keeps its project slot");

    runner.inner.set("validate", Ok(ok(VALID_SPECS)));
    let resumed = fx.continue_run(&run);
    let resumed = fx.drive(&resumed, dispatch.clone());
    assert_eq!(resumed.state, GraduationRunState::Completed, "{resumed:?}");
    assert_eq!(dispatch.of_part("merge_review").len(), 1);
}

// GRL-FR-AGOA: where the CLI is not usable, the specs validation cannot run,
// so the run is blocked with `openspec_validation_unavailable` and no review
// turn is dispatched.
#[test]
fn a_merge_review_with_no_usable_cli_blocks_the_merge_run() {
    let (fx, runner, run, probe) = openspec_merge_run_probed(Ok(ok(VALID_SPECS)));
    probe.executables.lock().unwrap().clear();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Blocked, "{run:?}");
    assert_eq!(run.blocker.as_ref().unwrap().code, "openspec_validation_unavailable");
    assert!(dispatch.of_part("merge_review").is_empty());
    assert_eq!(runner.inner.calls_of("validate"), 0);
}

// GRL-FR-ZDNL: outside OpenSpec mode the merge review carries no specs
// validation and no `validate` process runs.
#[test]
fn a_merge_review_outside_the_mode_carries_no_specs_validation() {
    let fx = Fixture::new();
    let runner = std::sync::Arc::new(crate::openspec::tests::FakeRunner::default());
    fx.app.manage(crate::openspec::OpenSpec::new(
        runner.clone(),
        std::sync::Arc::new(
            crate::openspec::tests::FakeProbe::default()
                .with_executable(crate::openspec::tests::CLI),
        ),
        crate::openspec::tests::env(),
    ));
    fx.allow_execution();
    let stream = fx.stream("feature");
    std::fs::write(stream.worktree().join("README.md"), "stream side\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "stream edit");
    std::fs::write(fx.root().join("README.md"), "base side\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "base edit");
    let result = merge(&fx, &stream, StreamMergePublication::Uncommitted).expect("handed off");
    let StreamMergeResult::Conflicted { run_id, .. } = result else { panic!("{result:?}") };
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&fx.reload(&run_id), dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    assert!(dispatch.of_part("merge_review")[0].input.get("openspec_specs_validation").is_none());
    assert_eq!(runner.calls_of("validate"), 0);
}
