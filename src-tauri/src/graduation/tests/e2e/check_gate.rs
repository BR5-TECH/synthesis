//! Journey 44 of the coverage matrix: the check gate on the answer of a work
//! turn (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`
//! GTE-FR-QMVR).
//!
//! A work turn ends with every required check passed, or it asks the author.
//! An answer the gate refuses gets one check correction and one escalate
//! correction, and the run then rests for the author with nothing reviewed and
//! nothing committed.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Scenario, Setting, TurnScript as Script};
use super::script::{check_failed, check_passed, check_unfinished, check_waived};
use crate::graduation::GraduationRunState;
use serde_json::json;

const HOUR: u64 = 60 * 60 * 1000;

// GTE-FR-QMVR / GRL-FR-IJTT, GRL-FR-CPXI, GRL-FR-CHDT, GRL-FR-SMQK,
// GRL-FR-SUBP, GRL-FR-BOHV, GRL-FR-FGWT, GRL-FR-LCSB, GRL-FR-KYYW,
// GRD-FR-PYJN, GRD-FR-WKMD, GLG-FR-FSZH, GLG-FR-WLFB, GLG-FR-YCVA: a red check
// gets one check correction in its session and one escalate correction. An
// escalate correction that does not escalate rests the run `awaiting_author`
// on `checks_not_passed`, with no review, no commit, and the stream and the
// slot held.
#[test]
fn a_red_check_gets_one_check_correction_and_one_escalate_correction_then_rests() {
    let outcome = Scenario::named("red checks rest the run")
        .work_turn(
            Script::reports_checks(vec![check_passed("build"), check_failed("tests", 101)])
                .writes("src/panel.ts", "1\n")
                .writes(".synthesis-checks/report.md", "tests: 101\n")
                .leaves_a_session(),
        )
        .work_turn(
            Script::reports_checks(vec![check_failed("tests", 101)])
                .writes("src/panel.ts", "2\n")
                .leaves_a_session(),
        )
        .work_turn(Script::reports_checks(vec![check_failed("tests", 101)]).leaves_a_session())
        .run();
    let escalate = outcome.dispatches()[2].instruction.clone();
    assert!(!escalate.contains("## Milestones"), "GRL-FR-LCSB: no milestone block");
    assert!(outcome.dispatches()[1].instruction.contains("## Milestones"), "GRL-FR-LCSB");
    outcome
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_checks_rest("checks_red", &[("tests", true, Some(101))])
        // GRD-FR-PYJN: the rest keeps the checkpoint, the spent gate included,
        // until Continue clears it (GRL-FR-LNIZ).
        .expect_check_gate(Some(("checks_red", "escalate")))
        .expect_continued_check_correction(None)
        .expect_no_escalation()
        .expect_dispatch_order(&["work", "work", "work"])
        .expect_turns(3, 0)
        .expect_pass(1)
        .expect_commits(0)
        .expect_stream_held()
        .expect_slot_held(true)
        .worktree_holds("src/panel.ts", "2\n")
        .expect_dispatch(0, |d| d.has_no_field("check_correction").has_no_field("correction"))
        .expect_dispatch(1, |d| {
            d.part("work")
                .pass(1)
                .resumes_session("s1")
                .field_contains("check_correction", "You may change files")
                .has_no_field("correction")
                // GRL-FR-DNKA: no work turn is given an account.
                .has_no_field("agent_account")
                // GRL-FR-KYYW: the record of the refused turn is kept for it.
                .directory_held(".synthesis-checks/report.md", "tests: 101\n")
        })
        .expect_dispatch(2, |d| {
            d.part("work")
                .resumes_session("s1")
                .field_contains("correction", "You may not report success")
                .field_contains("correction", "`escalation_required`")
                .has_no_field("check_correction")
                .has_no_field("agent_account")
                // GRL-FR-KYYW: an escalate correction carries `correction`, so
                // the folder is kept for it too.
                .directory_held(".synthesis-checks/report.md", "tests: 101\n")
        })
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[
                &[
                    ("part", json!("work")),
                    ("cause", json!("checks_red")),
                    ("next", json!("correct")),
                    ("checks", json!(2)),
                    ("red", json!(1)),
                    ("unfinished", json!(0)),
                    ("waived", json!(0)),
                ],
                &[("next", json!("escalate")), ("checks", json!(1))],
                &[("next", json!("rest"))],
            ],
        )
        .expect_records(
            "work turn started",
            "work_turn",
            &[
                &[("check_correction", json!(false)), ("corrected", json!(false))],
                &[("check_correction", json!(true)), ("corrected", json!(false))],
                &[("check_correction", json!(false)), ("corrected", json!(true))],
            ],
        )
        .expect_records(
            "run rested for the author",
            "stage_transition",
            &[&[
                ("reason", json!("checks_not_passed")),
                ("cause", json!("checks_red")),
                ("checks", json!(1)),
            ]],
        )
        .expect_records("review turn started", "review_turn", &[]);
}

// GTE-FR-QMVR / GRL-FR-CPXI, GRL-FR-CHDT, GRL-FR-XWNQ, GRL-FR-LNIZ,
// GRL-FR-DNKA: an unfinished check and a missing record each get the check
// correction of their cause. A corrected answer that passes goes to the review
// with its own account, and the run completes in the same pass.
#[test]
fn an_unfinished_check_and_a_missing_record_are_corrected_before_the_review() {
    let cases = [
        (
            "unfinished",
            Script::reports_checks(vec![check_unfinished("tests")]),
            "checks_unfinished",
            "did not pass or did not finish",
        ),
        (
            "no result",
            Script::reports_result(json!({ "summary": "not a record" })),
            "record_missing",
            "no valid check record",
        ),
        ("empty", Script::reports_checks(Vec::new()), "record_missing", "no valid check record"),
    ];
    for (name, first, cause, text) in cases {
        Scenario::named(name)
            .work_turn(first.writes("src/panel.ts", "1\n"))
            .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
            .review_turn(Script::answers_ready("The work answers the prompt."))
            .run()
            .expect_state(GraduationRunState::Completed)
            .expect_dispatch_order(&["work", "work", "review"])
            .expect_dispatch(1, |d| d.resumes_no_session().field_contains("check_correction", text))
            .expect_dispatch(2, |d| d.field("agent_account", "scripted"))
            .expect_turns(2, 1)
            .expect_pass(1)
            .expect_commits(1)
            .expect_committed_paths(&["src/panel.ts"])
            .expect_check_gate(None)
            .expect_records("work answer gated", "stage_transition", &[&[("cause", json!(cause))]]);
    }
}

// GTE-FR-XKGB, GTE-FR-QMVR / GRL-FR-TYRV, GRL-FR-CHDT, GRL-FR-FGWT,
// GRD-FR-PYJN: an agent-reported failure goes to no review. It gets the
// `work_failed` correction, then the escalate correction, and the run rests
// with nothing committed.
#[test]
fn an_agent_reported_failure_is_gated_and_reaches_no_review() {
    Scenario::named("agent failure gated")
        .work_turn(
            Script::reports_failure("The panel module does not exist to add a state to.")
                .writes("src/notes.md", "what I found\n"),
        )
        .work_turn(Script::reports_failure("It still does not exist."))
        .work_turn(Script::reports_failure("I cannot make it exist."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_checks_rest("work_failed", &[])
        .expect_dispatch_order(&["work", "work", "work"])
        .expect_dispatch(1, |d| {
            d.field_contains("check_correction", "it reported `failure`").resumes_no_session()
        })
        .expect_dispatch(2, |d| d.field_contains("correction", "you may not report failure"))
        .expect_commits(0)
        .worktree_holds("src/notes.md", "what I found\n");
}

// GTE-FR-TCUW / GRL-FR-TYRV, GRL-FR-CHDT, GRL-FR-DNKA: a failure the check
// correction turns into a passing record reaches the review, which reads the
// account of the corrected turn and not the failure.
#[test]
fn an_agent_reported_failure_corrected_to_passing_checks_reaches_the_review() {
    Scenario::named("agent failure corrected")
        .work_turn(
            Script::reports_failure("The formatter would not run.")
                .writes("src/notes.md", "what I found\n")
                .writes("src/panel.ts", "export const panel=1;\n"),
        )
        .work_turn(Script::reports_success().writes("src/panel.ts", "export const panel = 1;\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_agent_report_contains("scripted")
        .expect_dispatch(2, |d| {
            d.part("review")
                .field("agent_account", "scripted")
                // What the refused turn wrote stays in the change set the
                // review reads.
                .changed_paths(&["src/notes.md", "src/panel.ts"])
        })
        .expect_commits(1)
        .expect_committed_paths(&["src/notes.md", "src/panel.ts"])
        .expect_stream_released();
}

// GTE-FR-QMVR / GRL-FR-HLNB, GRL-FR-WRHT, GRL-FR-LNIZ, GRL-FR-LAVF: a red check
// whose `waived_by` names a recorded decision passes the gate. One that names
// no decision is red. The answers to an escalation close the gate, and the
// turn that takes them starts a new episode.
#[test]
fn a_red_check_waived_by_a_recorded_decision_passes_the_gate() {
    Scenario::named("waived check")
        .work_turn(
            Script::reports_checks(vec![check_failed("lint", 1)])
                .writes(".synthesis-checks/report.md", "lint: 1\n")
                .leaves_a_session(),
        )
        .work_turn(Script::reports_checks(vec![check_waived("lint", 1, 1)]).leaves_a_session())
        .work_turn(Script::escalates(&["May the lint check fail for this change?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["May the lint check fail for this change?"])
        .expect_check_gate(Some(("checks_red", "escalate")))
        .answered(
            &["Yes, the lint check may fail."],
            vec![
                Script::reports_checks(vec![check_passed("tests"), check_waived("lint", 1, 1)]),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| {
            d.has_no_field("check_correction")
                .has_no_field("correction")
                // GRL-FR-KYYW: the turn that takes the answers starts a new
                // episode, so the folder holds no result of an earlier turn.
                .directory_lacks(".synthesis-checks/report.md")
        })
        .expect_check_gate(None)
        .expect_turns(4, 1)
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[&[("cause", json!("checks_red"))], &[("next", json!("escalate"))]],
        );
}

// GTE-FR-QMVR / GRD-FR-OERK, GRL-FR-LAVF, GRL-FR-LNIZ, GRL-FR-SPRE: Continue
// from `checks_not_passed` dispatches a fresh turn of the same pass that
// carries the check correction of the rest's cause. It starts a new gate
// episode, and it resets neither the used total time nor the pass budget.
#[test]
fn continue_from_checks_not_passed_starts_a_new_gate_episode() {
    let red = || Script::reports_checks(vec![check_failed("tests", 1)]).taking(HOUR / 4);
    Scenario::named("continue after red checks")
        .with_setting(Setting::TotalTimeLimitMs, 4 * HOUR)
        .work_turn(
            red()
                .writes("src/panel.ts", "1\n")
                .writes(".synthesis-checks/report.md", "tests: 1\n")
                .leaves_a_session(),
        )
        .work_turn(red().leaves_a_session())
        .work_turn(red().leaves_a_session())
        .run()
        .expect_checks_rest("checks_red", &[("tests", true, Some(1))])
        .expect_time_used(3 * HOUR / 4, HOUR)
        .continued(vec![
            red(),
            Script::reports_checks(vec![check_passed("tests")]).writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_check_gate(None)
        .expect_dispatch_order(&["work", "work", "review"])
        .expect_dispatch(0, |d| {
            d.pass(1)
                .resumes_no_session()
                .field_contains("check_correction", "did not pass or did not finish")
                .directory_held("src/panel.ts", "1\n")
                // GRL-FR-KYYW: the Continued turn carries `check_correction`,
                // so the report of the turns before it is kept.
                .directory_held(".synthesis-checks/report.md", "tests: 1\n")
        })
        .expect_dispatch(1, |d| d.field_contains("check_correction", "did not pass"))
        .expect_pass(1)
        .expect_pass_window(1, 2)
        .expect_time_used(HOUR, 2 * HOUR)
        .expect_turns(5, 1);
}

// GTE-FR-QMVR / GRL-FR-BVTS, GXD-FR-UREF, GRL-FR-KYYW: a pause during the check
// correction keeps the open gate, and Continue dispatches the saved step with
// the check record kept.
#[test]
fn a_paused_check_correction_is_sent_after_continue() {
    Scenario::named("paused check correction")
        .work_turn(
            Script::reports_checks(vec![check_unfinished("tests")])
                .writes(".synthesis-checks/report.md", "tests: not finished\n")
                .leaves_a_session(),
        )
        .work_turn(Script::is_cancelled().pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_check_gate(Some(("checks_unfinished", "correct")))
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.resumes_session("s1")
                .field_contains("check_correction", "did not finish")
                .directory_held(".synthesis-checks/report.md", "tests: not finished\n")
        })
        .expect_check_gate(None);
}

// GTE-FR-QMVR / GRL-FR-BVTS, GRL-FR-SMQK: where no total time is left for the
// check correction, the run rests on `total_time_limit` and keeps the gate.
// Continue resets the used time and dispatches the saved step.
#[test]
fn a_check_correction_with_no_total_time_left_rests_and_keeps_the_gate() {
    Scenario::named("no time for the correction")
        .with_setting(Setting::TotalTimeLimitMs, HOUR)
        .work_turn(Script::reports_checks(vec![check_failed("tests", 1)]).taking(HOUR))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HOUR)
        .expect_check_gate(Some(("checks_red", "correct")))
        .expect_dispatch_count(1)
        .continued(vec![
            Script::reports_success().taking(HOUR / 10),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_dispatch(0, |d| {
            // The refused turn reported no session, so the step runs fresh.
            d.resumes_no_session().field_contains("check_correction", "did not pass")
        })
        .expect_check_gate(None)
        // GRL-FR-SPRE: Continue reset the used time, so it holds the turns
        // after Continue alone.
        .expect_time_used(HOUR / 10, HOUR / 5);
}

// GTE-FR-QMVR / GRL-FR-XWNQ, GRL-FR-IJTT, GRL-FR-SUBP: a `merge_work` answer
// passes the same gate, and an escalate correction that escalates rests the run
// for the author's answers.
#[test]
fn a_merge_work_answer_passes_the_same_gate() {
    conflicted_merge("merge work gated")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::reports_checks(vec![check_failed("tests", 1)])
                .writes("README.md", "both sides\n")
                .leaves_a_session(),
        )
        .merge_work_turn(Script::reports_checks(vec![check_failed("tests", 1)]).leaves_a_session())
        .merge_work_turn(Script::escalates(&["May the tests fail for this merge?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["May the tests fail for this merge?"])
        .expect_dispatch(1, |d| {
            d.part("merge_work").field_contains("check_correction", "did not pass")
        })
        .expect_dispatch(2, |d| {
            d.part("merge_work").field_contains("correction", "escalation_required")
        })
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[&[("part", json!("merge_work"))], &[("part", json!("merge_work"))]],
        );
}

// GTE-FR-QMVR / GRL-FR-SMQK: a check correction whose resume the vendor does
// not honour ran nothing, so it runs as a fresh turn in the same working copy.
#[test]
fn a_check_correction_the_vendor_cannot_resume_runs_as_a_fresh_turn() {
    Scenario::named("check correction not resumed")
        .work_turn(
            Script::reports_checks(vec![check_failed("tests", 1)])
                .writes("src/panel.ts", "1\n")
                .writes(".synthesis-checks/report.md", "tests: 1\n")
                .leaves_a_session(),
        )
        .work_turn(Script::cannot_resume())
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "work", "work", "review"])
        .expect_dispatch(1, |d| {
            d.resumes_session("s1").field_contains("check_correction", "did not pass")
        })
        .expect_dispatch(2, |d| {
            d.resumes_no_session()
                .field_contains("check_correction", "did not pass")
                .directory_held("src/panel.ts", "1\n")
                // GRL-FR-KYYW: the fresh turn continues the same record.
                .directory_held(".synthesis-checks/report.md", "tests: 1\n")
        })
        .expect_turns(2, 1)
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-QMVR / GRL-FR-FGWT, GRL-FR-SUBP: an escalate correction may not end the
// episode with a success. A green success from it still rests the run on
// `checks_not_passed` with the cause of the last refusal, and no review runs.
#[test]
fn a_green_success_from_the_escalate_correction_still_rests_the_run() {
    let outcome = Scenario::named("escalate correction answers green")
        .work_turn(Script::reports_checks(vec![check_failed("tests", 101)]).leaves_a_session())
        .work_turn(Script::reports_checks(vec![check_failed("tests", 101)]).leaves_a_session())
        .work_turn(Script::reports_checks(vec![check_passed("tests")]).leaves_a_session())
        .run();
    outcome
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_no_escalation()
        .expect_dispatch_order(&["work", "work", "work"])
        .expect_turns(3, 0)
        .expect_commits(0)
        .expect_checks_rest("checks_red", &[])
        .expect_stream_held()
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[
                &[("cause", json!("checks_red")), ("next", json!("correct"))],
                &[("cause", json!("checks_red")), ("next", json!("escalate"))],
                // The green answer itself passed: the record has no red check,
                // and the rest names the cause of the last refusal.
                &[
                    ("cause", json!("checks_red")),
                    ("next", json!("rest")),
                    ("red", json!(0)),
                    ("checks", json!(1)),
                ],
            ],
        )
        .expect_records("review turn started", "review_turn", &[]);
}

// GTE-FR-QMVR / GRL-FR-XWNQ, GRL-FR-CHDT: an escalation answer whose escalation
// is not recorded asks the author nothing, so the gate refuses it as
// `record_missing` and sends the check correction. It never reaches the review.
#[test]
fn an_escalation_that_is_not_recorded_is_gated_as_a_missing_record() {
    // A blank reason fails `validate_escalation_request`, so nothing is recorded.
    let outcome = Scenario::named("unrecorded escalation")
        .work_turn(Script::escalates_because("   ", &["Which one?"]).leaves_a_session())
        .work_turn(Script::reports_checks(vec![check_passed("tests")]).leaves_a_session())
        .review_turn(Script::answers_ready("ok"))
        .run();
    outcome
        .expect_state(GraduationRunState::Completed)
        .expect_no_escalation()
        .expect_dispatch_order(&["work", "work", "review"])
        .expect_dispatch(1, |d| {
            d.part("work").field_contains("check_correction", "You may change files")
        });
}

// GTE-FR-QMVR / GXD-FR-HLQI, GRD-FR-OERK: a pause recorded while the turn after
// Continue ran makes its answer unsettled, so the continued correction is kept,
// and the turn after the next Continue still carries it.
#[test]
fn a_pause_during_the_continued_turn_keeps_its_check_correction() {
    let red = || Script::reports_checks(vec![check_failed("tests", 1)]).leaves_a_session();
    Scenario::named("pause during the continued turn")
        .work_turn(red())
        .work_turn(red())
        .work_turn(red())
        .run()
        .expect_checks_rest("checks_red", &[("tests", true, Some(1))])
        .continued(vec![red().pausing_the_run().finishing_anyway()])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .continued(vec![
            Script::reports_checks(vec![check_passed("tests")]),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.field_contains("check_correction", "did not pass"));
}
