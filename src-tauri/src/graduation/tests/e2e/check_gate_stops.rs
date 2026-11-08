//! Journey 44 of the coverage matrix, continued: a stop while a step of the
//! check gate is open (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`
//! GTE-FR-QMVR).
//!
//! The loop saves the open gate before it dispatches a step, so a relaunch, a
//! shutdown, a pause or a spent total time leaves the step for Continue to
//! send, in the session the refused turn reported.

use super::scenario::{Scenario, Setting, TurnScript as Script};
use super::script::{check_failed, check_passed};
use crate::graduation::{GraduationInterruptionReason, GraduationRunState};
use serde_json::json;

const HOUR: u64 = 60 * 60 * 1000;

/// A turn that stops the run while it runs.
type Stopping = fn() -> Script;

fn red() -> Script {
    Script::reports_checks(vec![check_failed("tests", 1)])
}

// GTE-FR-QMVR / GRL-FR-BVTS, GXD-FR-UREF, GRD-FR-XVUD, GRL-FR-SMQK: the gate is
// in the stored record before its check correction is dispatched. A relaunch
// abandons that correction, the run rests on `execution_abandoned` with the
// gate kept, and Continue sends the saved step in the saved session.
#[test]
fn a_check_correction_abandoned_by_a_relaunch_is_sent_again_after_continue() {
    Scenario::named("relaunch during the check correction")
        .work_turn(red().writes("src/panel.ts", "1\n").leaves_a_session())
        .work_turn(
            Script::reports_checks(vec![check_passed("tests")])
                .then(|ctx| {
                    let stored = crate::graduation::load_run(&ctx.app, &ctx.run_id)
                        .expect("the stored run record");
                    let gate = stored
                        .checkpoint
                        .check_gate
                        .expect("GRL-FR-BVTS: the gate is stored before its step is dispatched");
                    assert_eq!(
                        (gate.cause.as_str(), gate.step.as_str()),
                        ("checks_red", "correct"),
                        "the stored step is the check correction"
                    );
                    assert_eq!(
                        gate.resume.and_then(|resume| resume.session_id).as_deref(),
                        Some("s1"),
                        "the stored gate names the session to resume"
                    );
                })
                .abandoned_by_a_relaunch(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_abandoned")
        .expect_check_gate(Some(("checks_red", "correct")))
        .expect_no_rest()
        .continued(vec![
            Script::reports_checks(vec![check_passed("tests")]).writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| {
            d.part("work")
                .resumes_session("s1")
                .field_contains("check_correction", "did not pass")
                .has_no_field("correction")
        })
        .expect_check_gate(None)
        .expect_pass(1);
}

// GTE-FR-QMVR / GRL-FR-BVTS, GXD-FR-UREF, GRL-FR-SUBP, GRL-FR-LCSB,
// GLG-FR-WLFB: a pause or a shutdown during the escalate correction keeps the
// gate at `escalate`. Continue sends the escalate correction again in the
// saved session: it carries `correction` and no `check_correction`, has no
// milestone block, and its record says it is corrected.
#[test]
fn a_stopped_escalate_correction_is_sent_again_after_continue() {
    let cases: [(&str, Stopping, &str); 2] = [
        (
            "pause during the escalate correction",
            || Script::is_cancelled().pausing_the_run(),
            "author_pause",
        ),
        (
            "shutdown during the escalate correction",
            || {
                Script::is_cancelled()
                    .stopping_the_application(GraduationInterruptionReason::ApplicationShutdown)
            },
            "application_shutdown",
        ),
    ];
    for (name, stopping, reason) in cases {
        Scenario::named(name)
            .work_turn(red().writes("src/panel.ts", "1\n").leaves_a_session())
            .work_turn(red().leaves_a_session())
            .work_turn(stopping())
            .run()
            .expect_state(GraduationRunState::Interrupted)
            .expect_interruption(reason)
            .expect_check_gate(Some(("checks_red", "escalate")))
            .expect_no_rest()
            .expect_dispatch(2, |d| d.field_contains("correction", "You may not report success"))
            .continued(vec![Script::escalates(&["May the tests fail for this change?"])])
            .expect_state(GraduationRunState::AwaitingAuthor)
            .expect_escalation_questions(&["May the tests fail for this change?"])
            .expect_dispatch_order(&["work"])
            .expect_dispatch(0, |d| {
                // GRL-FR-LCSB: the escalate correction has no milestone block.
                d.part("work")
                    .instruction_lacks("## Milestones")
                    .resumes_session("s1")
                    .field_contains("correction", "You may not report success")
                    .has_no_field("check_correction")
            })
            .expect_turns(4, 0)
            // GRD-FR-SWOJ: the stop committed what the refused turns wrote, as
            // for any stopped turn. The gate committed nothing.
            .expect_commits(1)
            .expect_commit_message_contains(0, "Abandoned graduation turn")
            .expect_records(
                "work turn started",
                "work_turn",
                &[
                    &[("check_correction", json!(false)), ("corrected", json!(false))],
                    &[("check_correction", json!(true)), ("corrected", json!(false))],
                    &[("check_correction", json!(false)), ("corrected", json!(true))],
                    &[("check_correction", json!(false)), ("corrected", json!(true))],
                ],
            );
    }
}

// GTE-FR-QMVR / GRL-FR-BVTS, GRL-FR-SPRE, GRL-FR-SMQK: where the check
// correction spends the total time, the run rests on `total_time_limit` with
// the gate at `escalate`. Continue resets the used time and sends the escalate
// correction in the saved session.
#[test]
fn continue_from_a_total_time_rest_sends_the_saved_escalate_correction() {
    Scenario::named("no time for the escalate correction")
        .with_setting(Setting::TotalTimeLimitMs, HOUR)
        .work_turn(red().taking(HOUR / 4).leaves_a_session())
        .work_turn(red().taking(3 * HOUR / 4).leaves_a_session())
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HOUR)
        .expect_check_gate(Some(("checks_red", "escalate")))
        .expect_dispatch_count(2)
        .expect_commits(0)
        .continued(vec![Script::escalates(&["May the tests fail?"]).taking(HOUR / 10)])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_no_rest()
        .expect_escalation_questions(&["May the tests fail?"])
        .expect_dispatch_count(1)
        .expect_dispatch(0, |d| {
            d.resumes_session("s1")
                .field_contains("correction", "You may not report success")
                .has_no_field("check_correction")
        })
        // GRL-FR-SPRE: the used time was reset, so it holds the escalate turn alone.
        .expect_time_used(HOUR / 10, HOUR / 5);
}

// GTE-FR-QMVR / GRD-FR-OERK, GRL-FR-WOBK, GRL-FR-SPRE: a `checks_not_passed`
// rest whose used time is the whole limit. Continue does not reset the time, so
// the run rests on `total_time_limit` with no dispatch, and keeps the continued
// check correction. The next Continue resets the time and sends it.
#[test]
fn a_checks_rest_with_no_time_left_keeps_the_continued_correction_through_a_time_rest() {
    Scenario::named("checks rest at the time limit")
        .with_setting(Setting::TotalTimeLimitMs, HOUR)
        .work_turn(red().taking(HOUR / 3).leaves_a_session())
        .work_turn(red().taking(HOUR / 3).leaves_a_session())
        .work_turn(red().taking(HOUR / 3).leaves_a_session())
        .run()
        .expect_checks_rest("checks_red", &[("tests", true, Some(1))])
        .expect_time_used(HOUR, HOUR + 1)
        .continued(Vec::new())
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_total_time_rest(HOUR)
        .expect_dispatch_count(0)
        .expect_check_gate(None)
        .expect_continued_check_correction(Some("checks_red"))
        .continued(vec![
            Script::reports_checks(vec![check_passed("tests")]).writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| {
            d.resumes_no_session()
                .field_contains("check_correction", "did not pass")
                .has_no_field("correction")
        })
        .expect_continued_check_correction(None)
        .expect_time_used(0, HOUR / 3)
        .expect_pass(1)
        .expect_commits(1);
}
