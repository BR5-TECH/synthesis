//! The apply run (`OCR-openspec-change-runs.md` OCR-FR-XKTV through
//! OCR-FR-IWOW, OCR-FR-XIBE; `../ai/GRL-graduation-loop.md` GRL-FR-EUSF,
//! GRL-FR-SZQK, GRL-FR-NWSC, GRL-FR-YFGA, GRL-FR-BHGJ, GRL-FR-EQDL,
//! GRL-FR-MXXA, GRL-FR-KDYL), driven with the agent scripted and the OpenSpec
//! CLI faked.

use super::change_runs::*;
use super::run_summary::ready_with;
use super::*;
use crate::openspec::tests::ok;
use crate::openspec::ProcessOutput;

/// An apply run that waits in its queue, started on the production path.
fn apply_run(fx: &Fixture, stream: &crate::streams::WorkStream) -> GraduationRun {
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(fx, stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    run
}

/// The work turn of an apply run that implements the tasks and marks them.
fn implementing() -> Turn {
    Turn::work().writing("src/logout.ts", "export const logout = () => {};\n").writing(
        &format!("openspec/changes/{CHANGE}/tasks.md"),
        "- [x] Add the button\n- [x] Wire the call\n",
    )
}

// GRL-FR-DXLU, GRL-FR-EUSF, GRL-FR-SZQK, GRL-FR-OEFM, GRL-FR-NWSC,
// GRL-FR-NZMK, GRL-FR-YFGA, GRL-FR-GADT:
// the work turn carries `work.md` and then the apply work block, and the
// review turn `review.md` and then the apply review block, with the change
// name filled in.
#[test]
fn the_turns_of_an_apply_run_carry_their_blocks() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![implementing(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let work = &dispatch.of_part("work")[0].instruction;
    assert!(work.starts_with(crate::graduation::driver::prompts::WORK_PROMPT.trim_end()));
    assert!(work.contains(&format!("openspec instructions apply --change {CHANGE} --json")));
    assert!(work.contains("mark done in `tasks.md`") || work.contains("mark each one done"));
    assert!(work.contains("other than `tasks.md`"));
    // GRL-FR-SZQK, GRL-FR-OEFM: a wrong plan is a plan update, written whole
    // into the scratch folder, with up to eight questions; a question not
    // about the plan is an ordinary escalation.
    assert!(work.contains("Give a plan update"));
    assert!(work.contains(&format!("`.synthesis-plan/openspec/changes/{CHANGE}/<file>`")));
    assert!(work.contains("`kind: \"plan_update\"`"));
    assert!(work.contains("between 0 and 8 `questions`"));
    assert!(work.contains("ordinary escalation"));
    assert!(!work.contains("{{ change-name }}"));
    assert!(!work.contains("Completeness"), "the review block is not the work block");

    let review = &dispatch.of_part("review")[0].instruction;
    assert!(review.starts_with(crate::graduation::driver::prompts::REVIEW_PROMPT.trim_end()));
    for word in ["Completeness", "Correctness", "Coherence"] {
        assert!(review.contains(word), "{word}");
    }
    assert!(review.contains("CRITICAL issue is a finding of severity `critical`"));
    assert!(review.contains("WARNING is `major`"));
    assert!(review.contains("SUGGESTION is `minor`"));
    assert!(review.contains("input.openspec_apply_progress"));
    assert!(review.contains("input.openspec_validation"));
    // GRL-FR-NZMK: a finding about the plan is `kind: "plan"`.
    assert!(review.contains("`kind: \"plan\"`") && review.contains("`kind: \"work\"`"));
    assert!(!review.contains("Propose"), "no propose block");
}

// GRL-FR-MXXA, GRL-FR-BHGJ, GXD-FR-XQJR, OCR-FR-XKTV: every turn names the
// phase and the change, the turns are `work` and `review` turns, and the
// review reads the task progress and the validation of its own checkout.
#[test]
fn the_review_of_an_apply_run_reads_progress_and_validation_of_its_checkout() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![implementing(), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    for seen in dispatch.seen() {
        assert_eq!(
            seen.input["openspec"],
            serde_json::json!({ "phase": "apply", "change": CHANGE })
        );
        assert_eq!(seen.input["graduation_input_version"], 1);
    }
    assert!(dispatch.of_part("work")[0].input.get("openspec_apply_progress").is_none());
    assert_eq!(dispatch.of_part("work")[0].turn_kind, TurnKind::Work);
    assert_eq!(dispatch.of_part("review")[0].turn_kind, TurnKind::Review);
    assert_eq!(
        canonical(&dispatch.of_part("work")[0].directory),
        canonical(&stream.worktree()),
        "an apply run works in the stream's working copy"
    );
    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.input["openspec_apply_progress"]["state"], "all_done");
    assert_eq!(review.input["openspec_apply_progress"]["total"], 2);
    assert_eq!(
        review.input["openspec_apply_progress"]["tasks"][1],
        serde_json::json!({ "description": "Wire the call", "done": true })
    );
    assert_eq!(review.input["openspec_validation"]["valid"], true);
    let reads: Vec<_> = runner
        .calls()
        .into_iter()
        .filter(|c| c.args[0] == "instructions" || c.args[0] == "validate")
        .filter(|c| c.cwd == review.directory)
        .collect();
    assert_eq!(reads.len(), 2, "both read in the review checkout");
}

// OCR-FR-UZNC, OCR-FR-QMCG, GRD-FR-PQMX, GRD-FR-UUGP: a ready review commits
// the work onto the stream branch under `Apply OpenSpec change <change>` on
// one line, and the run keeps the run summary.
#[test]
fn a_ready_apply_review_commits_under_its_own_message() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        implementing(),
        ready_with(serde_json::json!({ "title": "Add logout", "description": "Adds logout." })),
    ]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.commits.len(), 1);
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), run.commits[0]);
    assert_eq!(head.message().unwrap_or_default(), format!("Apply OpenSpec change {CHANGE}"));
    assert_eq!(run.run_summary.as_ref().map(|s| s.title.as_str()), Some("Add logout"));
}

/// `validate` of a change that does not pass.
fn invalid() -> Result<ProcessOutput, crate::openspec::ProcessError> {
    const INVALID: &str = r#"{ "items": [ { "id": "add-logout-3f2a", "type": "change", "valid": false,
  "issues": [ { "level": "ERROR", "path": "specs/auth/spec.md", "message": "No scenario" } ] } ] }"#;
    Ok(ProcessOutput { exit_status: Some(1), stdout: INVALID.as_bytes().to_vec() })
}

/// The three causes of the apply gate, as its finding names them.
const OPEN_TASKS: &str = "task(s) of the change are not done";
const NOT_VALID: &str = "does not pass `openspec validate --strict`";
const PLANNING_FILE: &str = "other than tasks.md";

/// Drive an apply run over `turns`, whose reviews would end the loop, and
/// assert that the gate replaced each with a revise of one critical finding
/// that names `cause` alone, so the run spends its passes and commits nothing.
fn assert_gated_by(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    turns: Vec<Turn>,
    cause: &str,
) {
    let run = apply_run(fx, stream);
    let dispatch = ScriptedDispatch::new(turns);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor, "the pass budget is spent");
    assert!(run.commits.is_empty(), "no ready commit");
    assert_eq!(dispatch.of_part("work").len(), 2, "the gate started another pass");
    let instruction =
        dispatch.of_part("work")[1].instruction_field("loop_instruction").unwrap_or_default();
    assert_eq!(instruction.matches("[critical]").count(), 1, "{instruction}");
    for other in [OPEN_TASKS, NOT_VALID, PLANNING_FILE] {
        assert_eq!(instruction.contains(other), other == cause, "{other} in {instruction}");
    }
}

// GRL-FR-EQDL: a ready review while a task is not done, the change valid and
// only `tasks.md` and code written, is a revise that names the open tasks.
#[test]
fn a_ready_on_open_tasks_alone_starts_another_pass() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let turns = vec![implementing(), Turn::ready(), Turn::work(), Turn::ready()];
    assert_gated_by(&fx, &stream, turns, OPEN_TASKS);
}

// GRL-FR-EQDL: a ready review on a change that does not validate, every task
// done and only `tasks.md` and code written, is a revise that names the
// validation.
#[test]
fn a_ready_on_an_invalid_change_alone_starts_another_pass() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    runner.set("validate", invalid());
    let stream = change_stream(&fx, "editor");
    let turns = vec![implementing(), Turn::ready(), Turn::work(), Turn::ready()];
    assert_gated_by(&fx, &stream, turns, NOT_VALID);
}

// GRL-FR-EQDL / GRL-FR-SZQK: a ready review on a work that writes
// `proposal.md`, every task done and the change valid, is a revise that names
// the file.
#[test]
fn a_ready_on_an_edited_planning_file_alone_starts_another_pass() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let turns = vec![
        implementing().writing(&format!("openspec/changes/{CHANGE}/proposal.md"), "# Why\n"),
        Turn::ready(),
        Turn::work(),
        Turn::ready(),
    ];
    assert_gated_by(&fx, &stream, turns, PLANNING_FILE);
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().ok(), Some("Propose the change"), "the branch is as it was");
}

// GRL-FR-EQDL / GRL-FR-UQNV: a revise of minor findings, which ends the loop
// as `ready` does, is gated too: with a task open it starts another pass.
#[test]
fn an_advisory_revise_on_open_tasks_starts_another_pass() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let turns = vec![
        implementing(),
        Turn::revise(ReviewSeverity::Minor, "A small wording point."),
        Turn::work(),
        Turn::revise(ReviewSeverity::Minor, "A small wording point."),
    ];
    assert_gated_by(&fx, &stream, turns, OPEN_TASKS);
}

// GRL-FR-KDYL, GRD-FR-UJII: a progress read that cannot run dispatches no
// review turn and blocks the run; Continue reads and reviews again.
#[test]
fn a_progress_read_that_fails_blocks_the_review() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    runner.set("instructions", Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new() }));
    let dispatch = ScriptedDispatch::new(vec![implementing(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Blocked);
    assert_eq!(run.blocker.as_ref().unwrap().code, "openspec_validation_unavailable");
    assert!(dispatch.of_part("review").is_empty());

    runner.set("instructions", Ok(ok(ALL_DONE)));
    let resumed = fx.continue_run(&run);
    let resumed = fx.drive(&resumed, dispatch.clone());
    assert_eq!(resumed.state, GraduationRunState::Completed);
    assert_eq!(dispatch.of_part("work").len(), 1, "the work turn is not run again");
}

// OCR-FR-IWOW, GRD-FR-SWOJ: a turn of an apply run that stops without a
// review commits what it wrote as an abandoned turn on the stream branch.
#[test]
fn a_stopped_apply_turn_commits_an_abandoned_turn() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch =
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))
            .writing("src/logout.ts", "half\n")]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Interrupted);
    assert_eq!(run.commits.len(), 1);
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let message = head.message().unwrap_or_default().to_string();
    assert!(message.starts_with("Abandoned graduation turn"), "{message}");
    assert!(message.contains(&format!("Apply {CHANGE}")), "{message}");
}

// OCR-FR-QMCG, GRD-FR-UXXF, WKS-FR-JXPV, GRD-FR-BHFP: when the stream merges,
// a completed apply run records a landing, and the branch listing returns it
// with the kind `change_apply` and its title.
#[test]
fn a_completed_apply_run_lands_and_is_listed_as_a_change_apply() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let run = fx.drive(&run, ScriptedDispatch::new(vec![implementing(), Turn::ready()]));
    assert_eq!(run.state, GraduationRunState::Completed);

    let base = stream.base_branch.clone();
    // The base branch reaches the stream's work, as a fast-forward merge
    // leaves it.
    let repo = fx.repo();
    let tip = git2::Oid::from_str(&run.commits[0]).unwrap();
    repo.reference(&format!("refs/heads/{base}"), tip, true, "test merge").unwrap();
    let recorded =
        crate::graduation::record_stream_landings(&fx.app, &stream.id, &base, &run.commits[0]);
    assert_eq!(recorded, 1);
    let listed = crate::graduation::branch_completed_runs_blocking(&fx.app, &base).unwrap();
    let entry = listed.iter().find(|entry| entry.run_id == run.id).expect("the apply run");
    assert_eq!(entry.kind, crate::graduation::CompletedRunKind::ChangeApply);
    assert_eq!(entry.title, format!("Apply {CHANGE}"));
    assert_eq!(entry.draft_id, "");
    let wire = serde_json::to_value(entry).unwrap();
    assert_eq!(wire["kind"], "change_apply");
}

/// The lines of the statistics log of the stream's change.
fn change_log(fx: &Fixture, stream_id: &str) -> Vec<serde_json::Value> {
    let path = fx
        .root()
        .join(crate::repository_store::CHANGES_DIRNAME)
        .join(stream_id)
        .join(CHANGE)
        .join("statistics.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

// OCR-FR-XIBE, GRD-FR-NHRY, CST-FR-RJPT: an operation of an apply run and of a
// rework run with one provider-reported usage record appends one
// `token_usage` line of scope `graduation` to the change's log, beside its
// `agent_operation` line, and no line to the log of a draft.
//
// The dispatch seam of an execution turn reports no usage of its own, so the
// operation is measured here with the recorder that every turn uses.
#[test]
fn a_reported_usage_of_a_change_run_is_a_token_usage_line_of_its_change() {
    for phase in [crate::graduation::PHASE_APPLY, crate::graduation::PHASE_REWORK] {
        let (fx, _, _) = change_fixture(ALL_DONE);
        let stream = change_stream(&fx, "editor");
        let run = if phase == crate::graduation::PHASE_APPLY {
            apply_run(&fx, &stream)
        } else {
            fx.allow_execution();
            fx.app.state::<GraduationState>().set_loop_enabled(false);
            start_rework(&fx, &stream, "Drop the second task.").expect("a rework start").run
        };
        let mut recorder = crate::graduation::statistics::OperationRecorder::begin(
            crate::graduation::statistics::bucket_for_part("work"),
        );
        let reply = crate::agent_conversations::ModelReply {
            prompt_tokens: Some(120),
            output_tokens: Some(30),
            ..Default::default()
        };
        recorder.observed(1, 1, &reply);
        recorder.finish(&fx.app, &run);
        crate::statistics::wait_for_writer();

        let lines = change_log(&fx, &stream.id);
        let usage: Vec<&serde_json::Value> =
            lines.iter().filter(|v| v["type"] == "token_usage").collect();
        assert_eq!(usage.len(), 1, "{phase}: {lines:?}");
        assert_eq!(usage[0]["scope"], "graduation", "{phase}: {lines:?}");
        let operation =
            lines.iter().find(|v| v["type"] == "agent_operation").expect("the operation");
        assert_eq!(operation["runId"], serde_json::json!(run.id), "{phase}");
        assert!(!fx.root().join("statistics").exists(), "{phase}: no draft log is written");
    }
}

// OCR-FR-XIBE, CST-FR-RJPT, GXD-FR-CYIW, GRD-FR-NHRY: every turn of a change
// run appends one `agent_operation` to the change's statistics log with the
// run id, and no line reaches the log of a draft.
#[test]
fn every_turn_of_an_apply_run_is_counted_against_its_change() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let run = fx.drive(&run, ScriptedDispatch::new(vec![implementing(), Turn::ready()]));
    crate::statistics::wait_for_writer();

    let operations: Vec<serde_json::Value> = change_log(&fx, &stream.id)
        .into_iter()
        .filter(|v| v["type"] == "agent_operation")
        .collect();
    assert_eq!(operations.len(), 2, "one line per turn: {operations:?}");
    assert!(operations.iter().all(|op| op["runId"] == serde_json::json!(run.id)));
    assert!(!fx.root().join("statistics").exists(), "no draft log is written");

    let store = fx.app.state::<crate::project::ProjectState>().require_store().unwrap();
    let stats =
        crate::change_statistics::read_change_statistics_impl(&store, &stream.id, CHANGE, Ok(1))
            .unwrap();
    assert!(stats.run_time_ms.value.is_some(), "CST-FR-HJGG: the run time is counted");
}

// OCR-FR-CXPK: the phase `apply` of an apply run is not the resume phase
// `apply` of a merge run: an apply run whose checkpoint names that phase still
// dispatches its work turn, and no merge apply is tried.
#[test]
fn the_apply_phase_is_not_the_resume_phase_of_a_merge_run() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let mut run = apply_run(&fx, &stream);
    run.checkpoint.resume_part = Some(crate::graduation::driver::PART_APPLY.to_string());
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    let dispatch = ScriptedDispatch::new(vec![implementing(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(dispatch.of_part("work").len(), 1);
    assert!(run.merge.is_none());
}
