//! Journey 44 of the coverage matrix, continued: what the check gate writes to
//! the logs, a merge run through the whole gate, and the episodes an answer
//! correction and an escalation answer open
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`
//! GTE-FR-QMVR).

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::outcome::Outcome;
use super::scenario::{Scenario, TurnScript as Script};
use super::script::{check_failed, check_passed, check_waived};
use crate::graduation::{GraduationLogStream, GraduationRunState};
use crate::tools::agent_exec::protocol::EnvelopeInvalid;
use serde_json::json;

const SENTINEL_NAME: &str = "zz-sentinel-check";
const SENTINEL_COMMAND: &str = "run zz-sentinel";

fn red() -> Script {
    Script::reports_checks(vec![check_failed("tests", 1)])
}

/// GLG-FR-FSZH: a check record whose name and command no log may hold.
fn sentinel(exit_status: i64) -> serde_json::Value {
    json!({
        "name": SENTINEL_NAME,
        "command": SENTINEL_COMMAND,
        "finished": true,
        "exit_status": exit_status,
    })
}

/// The records the application's diagnostic buffer holds for one run.
fn diagnostics(run_id: &str) -> Vec<crate::logging::LogRecord> {
    let filter: crate::logging::LogFilter =
        serde_json::from_value(json!({ "query": run_id })).expect("a filter");
    crate::logging::BUFFER.query(&filter, None, 5000).expect("a page").records
}

fn assert_no_check_text(outcome: &Outcome) {
    for stream in [GraduationLogStream::Structured, GraduationLogStream::Activity] {
        let lines = super::super::lines_of(outcome.fixture(), outcome.run_id(), stream);
        assert!(!lines.is_empty(), "the run's {stream:?} stream holds records");
        for line in &lines {
            let text = line.to_string();
            for secret in [SENTINEL_NAME, SENTINEL_COMMAND] {
                assert!(!text.contains(secret), "the {stream:?} stream names {secret:?}: {text}");
            }
        }
    }
    let records = diagnostics(outcome.run_id());
    assert!(
        records
            .iter()
            .any(|record| record.message == "graduation check gate refused a work answer"),
        "the buffer holds the gate's records of this run"
    );
    for record in &records {
        let text = serde_json::to_string(record).expect("a record serializes");
        for secret in [SENTINEL_NAME, SENTINEL_COMMAND] {
            assert!(!text.contains(secret), "the diagnostic buffer names {secret:?}: {text}");
        }
    }
}

// GTE-FR-QMVR / GLG-FR-FSZH, GLG-FR-YCVA: the gate's records, the rest's
// record, the record of Continue and the record of the accepted answer carry
// counts. No line of the run's two log streams and no record of the
// diagnostic buffer names a check or its command.
#[test]
fn no_log_names_a_check_or_its_command() {
    let refused = || Script::reports_checks(vec![sentinel(101)]).leaves_a_session();
    let outcome = Scenario::named("check names stay out of the logs")
        .work_turn(refused().writes("src/panel.ts", "1\n"))
        .work_turn(refused())
        .work_turn(refused())
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_checks_rest("checks_red", &[(SENTINEL_NAME, true, Some(101))]);
    assert_no_check_text(&outcome);
    let outcome = outcome
        .continued(vec![
            Script::reports_checks(vec![sentinel(0)]).writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed);
    assert_no_check_text(&outcome);
}

// GTE-FR-QMVR / GRL-FR-XWNQ, GRL-FR-SUBP, GRL-FR-FGWT, GRD-FR-OERK: a merge
// run passes the whole gate. Its `merge_work` answer rests the run on
// `checks_not_passed`, the escalate correction resumes the session and carries
// no check correction, and Continue sends the check correction to a
// `merge_work` turn whose green answer goes to `merge_review` and is applied.
#[test]
fn a_merge_run_passes_the_whole_gate_and_is_applied_after_continue() {
    conflicted_merge("merge run through the whole gate")
        .with_merge(as_one_commit())
        .merge_work_turn(red().writes("README.md", "both sides\n").leaves_a_session())
        .merge_work_turn(red().leaves_a_session())
        .merge_work_turn(red().leaves_a_session())
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_checks_rest("checks_red", &[("tests", true, Some(1))])
        .expect_check_gate(Some(("checks_red", "escalate")))
        .expect_dispatch_order(&["merge_work", "merge_work", "merge_work"])
        .expect_dispatch(1, |d| {
            d.part("merge_work")
                .resumes_session("s1")
                .field_contains("check_correction", "did not pass")
                .has_no_field("correction")
        })
        .expect_dispatch(2, |d| {
            d.part("merge_work")
                .resumes_session("s1")
                .field_contains("correction", "You may not report success")
                .has_no_field("check_correction")
        })
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[
                &[("part", json!("merge_work")), ("next", json!("correct"))],
                &[("part", json!("merge_work")), ("next", json!("escalate"))],
                &[("part", json!("merge_work")), ("next", json!("rest"))],
            ],
        )
        .continued(vec![
            Script::reports_checks(vec![check_passed("tests")]).writes("README.md", "both sides\n"),
            Script::answers_ready("Both intents stand."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_check_gate(None)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_work")
                .resumes_no_session()
                .field_contains("check_correction", "did not pass")
        })
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-QMVR / GRL-FR-BOHV, GRL-FR-IPNO, GRL-FR-QKDC, GRL-FR-SUBP: a check
// correction whose answer cannot be read is asked again once, in its session.
// That answer correction carries `correction` and no `check_correction`, and
// its red answer gets the escalate correction rather than a rest on
// `unreadable_answer`. Each of the four turns adds one to `work_turns`.
#[test]
fn an_unreadable_check_correction_is_asked_again_and_then_escalated() {
    Scenario::named("unreadable check correction")
        .work_turn(red().writes("src/panel.ts", "1\n").leaves_a_session())
        .work_turn(Script::answers_refused(EnvelopeInvalid::Malformed))
        .work_turn(red().leaves_a_session())
        .work_turn(Script::escalates(&["May the tests fail for this change?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["May the tests fail for this change?"])
        .expect_no_rest()
        .expect_blocker_absent()
        .expect_no_pending_correction()
        .expect_dispatch_order(&["work", "work", "work", "work"])
        .expect_dispatch(1, |d| {
            d.resumes_session("s1")
                .field_contains("check_correction", "did not pass")
                .has_no_field("correction")
        })
        .expect_dispatch(2, |d| {
            d.resumes_session("s1")
                .field_contains("correction", "It was not in the required shape")
                .has_no_field("check_correction")
        })
        .expect_dispatch(3, |d| {
            d.resumes_session("s1")
                .field_contains("correction", "You may not report success")
                .has_no_field("check_correction")
        })
        .expect_turns(4, 0)
        .expect_pass(1)
        .expect_records(
            "turn answer refused",
            "stage_transition",
            &[&[("part", json!("work")), ("refusal", json!("malformed"))]],
        )
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[&[("next", json!("correct"))], &[("next", json!("escalate"))]],
        );
}

// GTE-FR-QMVR / GRL-FR-LAVF, GRL-FR-LNIZ, GRL-FR-HLNB, GLG-FR-FSZH: the turn
// that takes the answers to a gate escalation starts a new gate episode. Its
// red answer, with one valid waiver beside the red check, gets a check
// correction and not a rest, and the corrected answer reaches the review.
#[test]
fn a_red_answer_after_a_gate_escalation_gets_a_check_correction() {
    Scenario::named("red after the answers")
        .work_turn(red().leaves_a_session())
        .work_turn(red().leaves_a_session())
        .work_turn(Script::escalates(&["May the tests fail?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_check_gate(Some(("checks_red", "escalate")))
        .answered(
            &["No. The lint check may fail, the tests may not."],
            vec![
                Script::reports_checks(vec![check_waived("lint", 1, 1), check_failed("tests", 1)])
                    .leaves_a_session(),
                Script::reports_checks(vec![check_waived("lint", 1, 1), check_passed("tests")]),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_no_rest()
        .expect_check_gate(None)
        .expect_dispatch_order(&["work", "work", "review"])
        .expect_dispatch(0, |d| d.has_no_field("check_correction").has_no_field("correction"))
        .expect_dispatch(1, |d| {
            d.resumes_session("s1")
                .field_contains("check_correction", "did not pass")
                .has_no_field("correction")
        })
        .expect_turns(5, 1)
        .expect_records(
            "work answer gated",
            "stage_transition",
            &[
                &[("next", json!("correct"))],
                &[("next", json!("escalate"))],
                &[
                    ("cause", json!("checks_red")),
                    ("next", json!("correct")),
                    ("checks", json!(2)),
                    ("red", json!(1)),
                    ("waived", json!(1)),
                    ("unfinished", json!(0)),
                ],
            ],
        );
}
