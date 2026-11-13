//! The OpenSpec runs, driven through the production loop with OpenSpec mode
//! active and the fake CLI of `openspec_cli.rs` behind the process seam:
//! the propose run, the rework run, the apply run and its plan update, the
//! transient retry of a CLI step, and the image probe (GTE-FR-QPMA,
//! GTE-FR-ZDCH, GTE-FR-WRPN, GTE-FR-BXHS, GTE-FR-NFYK, GTE-FR-HAQO,
//! GTE-FR-RTKU, GTE-FR-YMEC).

use super::super::change_plan_updates::{questions, REASON};
use super::super::probe_answer;
use super::openspec_arrange::change_file;
use super::openspec_cli::STALLED;
use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::GraduationRunState;

const CHANGE: &str = "add-logout";

/// The three files of a valid change.
fn change_files() -> [String; 3] {
    [
        change_file(CHANGE, "proposal.md"),
        change_file(CHANGE, "tasks.md"),
        change_file(CHANGE, "specs/auth/spec.md"),
    ]
}

/// A work turn of a propose run that writes the whole change.
fn writing_the_change() -> Script {
    let [proposal, tasks, spec] = change_files();
    Script::reports_success()
        .writes(&proposal, "# Why\n\nLogout is missing.\n")
        .writes(&tasks, "- [ ] Add the button\n")
        .writes(&spec, "## ADDED Requirements\n### Requirement: Logout\n")
}

/// A work turn of an apply run that implements every task and marks each one
/// done.
fn finishing() -> Script {
    Script::reports_success()
        .writes("src/logout.ts", "export const logout = () => {};\n")
        .writes(&change_file(CHANGE, "tasks.md"), "- [x] Add the button\n- [x] Wire the call\n")
}

// GTE-FR-QPMA, GTE-FR-WRPN, GRD-FR-LKGC, GRD-FR-SYHE, GRL-FR-CDNI: a work turn
// writes the change below `openspec/changes/<change>/`, the validate gate
// reads it as valid in the review checkout before the review, and the run
// completes with one commit that holds the change files alone. The draft
// reports `graduated`, and its run names the change.
#[test]
fn a_propose_run_writes_its_change_and_commits_the_change_files_alone() {
    let files = change_files();
    Scenario::named("openspec propose")
        .in_openspec_mode()
        .with_prompt("Add a logout button.")
        .proposing(CHANGE)
        .work_turn(writing_the_change())
        .review_turn(Script::answers_ready("The change answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(1, |d| d.part("review").openspec_validation(true))
        .expect_cli_processes("validate", 1)
        .expect_cli_read_the_directory_of("validate", 1)
        .expect_commits(1)
        .expect_committed_paths(&[&files[0], &files[1], &files[2]])
        .expect_draft_graduated_into(CHANGE)
        .expect_stream_released();
}

// GTE-FR-QPMA, GTE-FR-WRPN, GRL-FR-CDNI: the fake CLI validates what the
// review checkout holds, so a change that misses files is read as invalid,
// its `ready` review starts another pass, and the change completed on that
// pass is read as valid and committed once.
#[test]
fn a_change_the_validate_gate_reads_as_invalid_goes_to_another_pass() {
    let [proposal, tasks, spec] = change_files();
    Scenario::named("openspec propose invalid first")
        .in_openspec_mode()
        .proposing(CHANGE)
        .work_turn(Script::reports_success().writes(&proposal, "# Why\n"))
        .review_turn(Script::answers_ready("The change answers the prompt."))
        .work_turn(
            Script::reports_success()
                .writes(&tasks, "- [ ] Add the button\n")
                .writes(&spec, "## ADDED Requirements\n"),
        )
        .review_turn(Script::answers_ready("The change answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review", "work", "review"])
        .expect_dispatch(1, |d| d.openspec_validation(false))
        .expect_dispatch(2, |d| d.pass(2).field_contains("loop_instruction", CHANGE))
        .expect_dispatch(3, |d| d.openspec_validation(true))
        .expect_cli_processes("validate", 2)
        .expect_commits(1)
        .expect_committed_paths(&[&proposal, &tasks, &spec]);
}

// GTE-FR-BXHS, OCR-FR-ZNJT, OCR-FR-WBZX, OCR-FR-TQCW: the work turn of a
// rework run stands in its rework worktree and its review in a checkout
// derived from it, at the seed commit. Its `ready` review records one
// change edit proposal with the hunks of each changed file into the request
// thread, and the stream branch, the stream's working copy and the change
// files are unchanged.
#[test]
fn a_ready_rework_records_one_change_edit_proposal_and_writes_no_branch() {
    Scenario::named("openspec rework")
        .in_openspec_mode()
        .with_change(CHANGE)
        .reworking(CHANGE, "Drop the second task.")
        .work_turn(
            Script::reports_success()
                .writes(&change_file(CHANGE, "tasks.md"), "- [ ] Add the button\n")
                .writes(&change_file(CHANGE, "design.md"), "# Design\n"),
        )
        .review_turn(Script::answers_ready("One task is gone."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_turns_stood_off_the_stream()
        .expect_reviews_stood_in_checkouts_of_the_rework(
            &change_file(CHANGE, "design.md"),
            "# Design\n",
        )
        .expect_dispatch(1, |d| d.openspec_validation(true))
        .expect_one_change_edit_proposal(CHANGE, "One task is gone.", &["design.md", "tasks.md"])
        .expect_proposal_hunks_of(CHANGE, "tasks.md", "- [ ] Wire the call\n", "")
        .expect_commits(0)
        .expect_stream_untouched()
        .expect_stream_released();
}

// GTE-FR-NFYK, GTE-FR-QPMA, OCR-FR-XKTV, OCR-FR-UZNC: the work turns of an
// apply run tick tasks in `tasks.md` in the stream's working copy. The
// review reads the task counts from `instructions apply` over its checkout,
// not from the answer: one open task turns the first `ready` into another
// pass, and the `ready` over every task done lands one commit on the stream
// branch under its own message.
#[test]
fn an_apply_run_reads_its_task_counts_from_the_working_copy_and_lands_one_commit() {
    let tasks = change_file(CHANGE, "tasks.md");
    Scenario::named("openspec apply")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(
            Script::reports_success()
                .writes("src/logout.ts", "export const logout = () => {};\n")
                .writes(&tasks, "- [x] Add the button\n- [ ] Wire the call\n"),
        )
        .review_turn(Script::answers_ready("Every task is done."))
        .work_turn(
            Script::reports_success()
                .writes("src/wire.ts", "export const wire = 1;\n")
                .writes(&tasks, "- [x] Add the button\n- [x] Wire the call\n"),
        )
        .review_turn(Script::answers_ready("Every task is done."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review", "work", "review"])
        .expect_work_turns_stood_in_the_stream()
        .expect_dispatch(1, |d| d.apply_progress("ready", 1, 2))
        .expect_dispatch(2, |d| {
            d.pass(2).field_contains("loop_instruction", "task(s) of the change are not done")
        })
        .expect_dispatch(3, |d| d.apply_progress("all_done", 2, 2))
        .expect_cli_read_the_directory_of("instructions", 1)
        .expect_cli_read_the_directory_of("instructions", 3)
        .expect_commits(1)
        .expect_branch_head_message_contains(&format!("Apply OpenSpec change {CHANGE}"))
        .expect_committed_paths(&[&tasks, "src/logout.ts", "src/wire.ts"])
        .expect_stream_released();
}

// GTE-FR-HAQO, OPU-FR-YBLE, OPU-FR-QXUE, OPU-FR-THOZ, OPU-FR-CAGI: a plan
// update of the work turn rests the run `awaiting_author` on `plan_update`
// with its stream and slot held. Continue is refused with
// `plan_questions_pending` while the questions are open and changes nothing.
// Continue after the answers dispatches a work turn in the session that
// asked, whose ledger holds the answers in position order.
#[test]
fn a_plan_update_rests_the_run_and_continue_carries_the_answers() {
    Scenario::named("openspec plan update")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(Script::gives_plan_update(REASON, questions()))
        .run()
        .expect_plan_rest(REASON)
        .expect_dispatch_count(1)
        .expect_commits(0)
        .expect_stream_held()
        .expect_slot_held(true)
        .refusing_continue_and_changing_nothing("plan_questions_pending")
        .answering_the_plan_questions()
        .continued(vec![finishing(), Script::answers_ready("Every task is done.")])
        .expect_dispatch(0, |d| {
            d.part("work").purpose("escalation_answer").resumes_session("s1").decisions(&[
                ("Keep the old cookie name?", "Keep the name session_id.", "work", 1),
                ("Which table holds the sessions?", "A new sessions table.", "work", 1),
            ])
        })
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .expect_stream_released();
}

// GTE-FR-HAQO, OPU-FR-THOZ: an answer set that answers one of the two plan
// questions is refused, so the rest holds no answers, and Continue is then
// refused with `plan_questions_pending` and changes nothing.
#[test]
fn continue_with_one_of_two_plan_questions_answered_is_refused() {
    Scenario::named("openspec plan update half answered")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .work_turn(Script::gives_plan_update(REASON, questions()))
        .run()
        .expect_plan_rest(REASON)
        .answering_one_of_the_plan_questions()
        .expect_no_plan_answers()
        .refusing_continue_and_changing_nothing("plan_questions_pending")
        .expect_plan_rest(REASON)
        .expect_commits(0)
        .expect_stream_held()
        .expect_slot_held(true);
}

// GTE-FR-RTKU, GTE-FR-ZDCH, OFP-FR-PQNS, OFP-FR-MJZU: the validate step of a
// propose run whose first process times out runs once more after one
// recorded wait of 2000 ms, and the run goes on to its review and completes.
#[test]
fn a_cli_step_that_times_out_once_runs_again_after_one_recorded_wait() {
    Scenario::named("openspec retry once")
        .in_openspec_mode()
        .proposing(CHANGE)
        .with_cli_timing_out("validate", 1)
        .work_turn(writing_the_change())
        .review_turn(Script::answers_ready("The change answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_waits(&[2_000])
        .expect_cli_processes("validate", 2)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(1, |d| d.openspec_validation(true))
        .expect_commits(1);
}

// GTE-FR-RTKU, GTE-FR-ZDCH, OFP-FR-PQNS, OFP-FR-TBHX: a validate step whose
// two processes both time out rests the run `blocked` with the code, the
// cause and the diagnostics, dispatches no review turn, and keeps the stream
// and the slot. Continue runs the step again, and the review follows.
#[test]
fn a_cli_step_that_times_out_twice_blocks_the_run_with_its_diagnostics() {
    Scenario::named("openspec retry spent")
        .in_openspec_mode()
        .proposing(CHANGE)
        .with_cli_timing_out("validate", 2)
        .work_turn(writing_the_change())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("openspec_validation_unavailable")
        .expect_blocker_cause("cli_timed_out")
        .expect_blocker_diagnostic(STALLED)
        .expect_dispatch_count(1)
        .expect_waits(&[2_000])
        .expect_cli_processes("validate", 2)
        .expect_stream_held()
        .expect_slot_held(true)
        .continued(vec![Script::answers_ready("The change answers the prompt.")])
        .expect_dispatch_order(&["review"])
        .expect_cli_processes("validate", 3)
        .expect_state(GraduationRunState::Completed);
}

// GTE-FR-YMEC, GTE-FR-ZDCH, OFP-FR-BZDP, OFP-FR-WACS: a probe that passes
// before the first turn records the version of the image's CLI, not the
// host's, in the checkpoint, and the dispatch after Continue does not probe
// again.
#[test]
fn a_passing_image_probe_is_kept_in_the_checkpoint_and_not_run_again() {
    Scenario::named("openspec image probe passes")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .with_image_probes(vec![probe_answer(Some(0), "2.0.1\n")])
        .work_turn(Script::times_out())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_image_probes(1)
        .expect_image_cli(Some("2.0.1"))
        .continued(vec![finishing(), Script::answers_ready("Every task is done.")])
        .expect_image_probes(0)
        .expect_image_cli(Some("2.0.1"))
        .expect_state(GraduationRunState::Completed);
}

// GTE-FR-YMEC, OFP-FR-BZDP, OFP-FR-XBNJ, OFP-FR-WACS, OFP-FR-TBHX: a probe
// that finds no CLI in the image rests the run `blocked` with
// `openspec_image_cli_unavailable` and the state `not_found` before the first
// turn, records nothing, and keeps the stream and the slot. Continue probes
// again before it dispatches the turn.
#[test]
fn an_image_with_no_cli_blocks_the_run_before_the_first_turn() {
    Scenario::named("openspec image without a cli")
        .in_openspec_mode()
        .with_change(CHANGE)
        .applying(CHANGE)
        .with_image_probes(vec![probe_answer(Some(127), "")])
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("openspec_image_cli_unavailable")
        .expect_blocker_detail("not_found")
        .expect_dispatch_count(0)
        .expect_image_probes(1)
        .expect_image_cli(None)
        .expect_stream_held()
        .expect_slot_held(true)
        .continuing_with_image_probes(
            vec![probe_answer(Some(0), "2.0.1\n")],
            vec![finishing(), Script::answers_ready("Every task is done.")],
        )
        .expect_image_probes(1)
        .expect_image_cli(Some("2.0.1"))
        .expect_state(GraduationRunState::Completed);
}
