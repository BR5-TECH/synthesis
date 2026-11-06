//! Journey 43 of the coverage matrix: a refused answer is asked again once, in
//! the session of the turn that gave it
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{GraduationCheckpoint, GraduationRunState};
use crate::tools::agent_exec::protocol::EnvelopeInvalid;
use crate::tools::agent_exec::{AgentExecutionError, DirectoryProblem};
use serde_json::json;

// GRL-FR-QKDC, GRL-FR-XSFA, GRL-FR-NSEY, GLG-FR-ZJTP: a work turn whose answer
// was refused is asked again in its own session, carrying the correction. The
// corrected answer settles the turn, the work is committed once, and the pass
// does not advance.
#[test]
fn a_refused_work_answer_is_corrected_in_its_session() {
    Scenario::named("work answer corrected")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::EscalationShape).writes("src/panel.ts", "1\n"),
        )
        .work_turn(Script::reports_success())
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(3)
        .expect_dispatch(0, |d| d.part("work").resumes_no_session().has_no_field("correction"))
        .expect_dispatch(1, |d| {
            d.part("work")
                .pass(1)
                .resumes_session("s1")
                .field_contains("correction", "change no file")
                .field_contains("correction", "`escalation`")
        })
        .expect_dispatch(2, |d| d.part("review").resumes_no_session())
        .expect_turns(2, 1)
        .expect_pass(1)
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_no_pending_correction()
        .expect_records(
            "turn answer refused",
            "stage_transition",
            &[&[("part", json!("work")), ("refusal", json!("escalation_shape"))]],
        )
        .expect_records(
            "work turn started",
            "work_turn",
            &[&[("corrected", json!(false))], &[("corrected", json!(true))]],
        );
}

// GRL-FR-JVNB, GXD-FR-UZHX, GXD-FR-WJOW, GLG-FR-RLQZ: a second refused answer
// rests the run on `unreadable_answer`. The detail and the record name the
// refusal, and one commit holds the work of both turns.
#[test]
fn a_second_refused_answer_rests_the_run_with_one_commit() {
    Scenario::named("refused twice")
        .work_turn(Script::answers_refused(EnvelopeInvalid::TooLarge).writes("src/a.ts", "1\n"))
        .work_turn(Script::answers_refused(EnvelopeInvalid::TooLarge).writes("src/b.ts", "2\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_interruption_detail_contains("a value in the answer was too long")
        .expect_interrupted_record("unreadable_answer", "error", &[("refusal", json!("too_large"))])
        .expect_dispatch_count(2)
        .expect_turns(2, 0)
        .expect_commits(1)
        .expect_committed_paths(&["src/a.ts", "src/b.ts"])
        .expect_no_pending_correction();
}

// GRL-FR-MZHT: a refusal the agent cannot repair, and a refused answer with no
// session to resume, rest the run at once with no correction turn.
#[test]
fn an_uncorrectable_refusal_stops_at_once() {
    let endings = [
        ("vendor run failed", Script::answers_refused(EnvelopeInvalid::VendorRunFailed)),
        ("output truncated", Script::answers_refused(EnvelopeInvalid::OutputTruncated)),
        ("session mismatch", Script::answers_refused(EnvelopeInvalid::SessionMismatch)),
        ("no session", Script::answers_refused_without_a_session(EnvelopeInvalid::Malformed)),
    ];
    for (name, ending) in endings {
        Scenario::named(name)
            .work_turn(ending)
            .run()
            .expect_state(GraduationRunState::Interrupted)
            .expect_interruption("unreadable_answer")
            .expect_dispatch_count(1)
            .expect_turns(1, 0)
            .expect_no_pending_correction()
            .expect_records("turn answer refused", "stage_transition", &[]);
    }
}

// GRL-FR-QKDC, GRL-FR-OTRH: a refused review answer is corrected in the
// session of that review, in the same checkout, and the corrected verdict is
// read.
#[test]
fn a_refused_review_answer_is_corrected_in_the_review_session() {
    let outcome = Scenario::named("review answer corrected")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_refused(EnvelopeInvalid::Malformed))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run();
    let checkout = outcome.dispatches()[1].directory.clone();
    let home = outcome.run_home_path();
    outcome
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(3)
        .expect_dispatch(1, |d| d.directory_is_review_checkout_under(&home))
        .expect_dispatch(2, |d| {
            d.part("review")
                // GRL-FR-QEUP: within one dispatch, the checkout of the turn it
                // corrects.
                .directory(&checkout)
                .resumes_session("s1")
                .field_contains("correction", "required shape")
        })
        .expect_turns(1, 2)
        .expect_commits(1);
}

// GRL-FR-NSEY, GRL-FR-TVXI: a corrected review that returns an unreadable
// verdict is asked again under GRL-FR-TVXI, whose count the correction did not
// raise.
#[test]
fn a_correction_does_not_raise_the_verdict_refusal_count() {
    Scenario::named("correction then unreadable verdict")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_refused(EnvelopeInvalid::NotFound))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(4)
        .expect_dispatch(3, |d| d.part("review").resumes_no_session())
        .expect_turns(1, 3);
}

// GRL-FR-HWLD: a correction stopped by a pause stays saved, so the first turn
// after Continue is the correction, in the session of the refused turn.
#[test]
fn a_paused_correction_is_sent_after_continue() {
    Scenario::named("paused correction")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::SummaryEmpty).writes("src/panel.ts", "1\n"),
        )
        .work_turn(Script::is_cancelled().pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(2)
        .expect_dispatch(0, |d| {
            d.part("work").resumes_session("s1").field_contains("correction", "`summary`")
        })
        .expect_dispatch(1, |d| d.part("review").has_no_field("correction"));
}

// GRL-FR-HWLD: a correction that could not launch stays saved, and Continue
// sends it.
#[test]
fn a_correction_that_could_not_launch_is_sent_after_continue() {
    Scenario::named("unlaunchable correction")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::MultipleDocuments)
                .writes("src/panel.ts", "1\n"),
        )
        .work_turn(Script::cannot_launch(AgentExecutionError::ExecutionDirectoryInvalid(
            DirectoryProblem::Inaccessible,
        )))
        .run()
        .expect_interruption("launch_failed")
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.part("work").resumes_session("s1").field_contains("correction", "one document")
        });
}

// GRL-FR-PTCE: where the vendor does not resume the session, the answer of the
// correction turn is discarded and the refused turn settles as it ended.
#[test]
fn a_correction_that_cannot_resume_settles_the_refused_turn() {
    Scenario::named("correction not resumed")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::Exclusivity).writes("src/panel.ts", "1\n"),
        )
        .work_turn(Script::cannot_resume())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_interruption_detail_contains("did not match the fields it carried")
        .expect_dispatch_count(2)
        .expect_commits(1)
        .expect_no_pending_correction();
}

// GRL-FR-PTCE: where the vendor started a new session and reported success
// there, that answer is discarded and the refused turn settles as it ended.
#[test]
fn a_correction_answered_from_a_fresh_session_is_discarded() {
    Scenario::named("correction from a fresh session")
        .work_turn(Script::answers_refused(EnvelopeInvalid::Exclusivity).writes("src/a.ts", "1\n"))
        .work_turn(Script::answers_from_a_fresh_session().writes("src/b.ts", "2\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_interruption_detail_contains("did not match the fields it carried")
        .expect_dispatch_order(&["work", "work"])
        .expect_turns(2, 0)
        .expect_commits(1)
        .expect_no_pending_correction();
}

// GRL-FR-PTCE, GRL-FR-HWLD: a run paused while its refused turn ran settles that
// turn on the pause. The correction is kept, so the first turn after Continue
// is the correction.
#[test]
fn a_pause_during_the_refused_turn_keeps_the_correction() {
    Scenario::named("pause during the refused turn")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::Malformed)
                .writes("src/panel.ts", "1\n")
                .pausing_the_run()
                .finishing_anyway(),
        )
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_dispatch_count(1)
        .expect_pending_correction("work", "malformed")
        .expect_records("turn answer refused", "stage_transition", &[])
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.part("work").resumes_session("s1").field_contains("correction", "required shape")
        })
        .expect_no_pending_correction();
}

// GRL-FR-JVNB: a correction sent after Continue is the only one. Refused again,
// it rests the run, and the next Continue sends no correction.
#[test]
fn a_correction_sent_after_continue_is_not_corrected_again() {
    Scenario::named("pending correction refused")
        .work_turn(
            Script::answers_refused(EnvelopeInvalid::Malformed)
                .writes("src/panel.ts", "1\n")
                .pausing_the_run()
                .finishing_anyway(),
        )
        .run()
        .continued(vec![Script::answers_refused(EnvelopeInvalid::Malformed)])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_dispatch_count(1)
        .expect_no_pending_correction()
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.part("work").has_no_field("correction"));
}

// GRL-FR-HWLD: a work turn that runs first drops a pending review correction.
#[test]
fn a_work_turn_drops_a_pending_review_correction() {
    Scenario::named("work drops a review correction")
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "1\n").pausing_the_run())
        .run()
        .expect_interruption("author_pause")
        .rewriting_checkpoint(|checkpoint: &mut GraduationCheckpoint| {
            checkpoint.answer_correction = Some(crate::graduation::GraduationAnswerCorrection {
                part: "review".to_string(),
                reason: "malformed".to_string(),
                resume: crate::graduation::GraduationResumeRef {
                    session_id: Some("s1".to_string()),
                    continuation_token: None,
                },
            });
        })
        .continued(vec![
            Script::reports_success(),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.part("work").has_no_field("correction"))
        .expect_dispatch(1, |d| d.part("review").resumes_no_session().has_no_field("correction"))
        .expect_no_pending_correction();
}

// GRL-FR-QKDC, GRL-FR-MRVK: a `merge_work` turn is corrected on the same terms.
#[test]
fn a_refused_merge_work_answer_is_corrected() {
    conflicted_merge("merge work corrected")
        .with_merge(as_one_commit())
        .merge_work_turn(
            Script::answers_refused(EnvelopeInvalid::Malformed).writes("README.md", "both sides\n"),
        )
        .merge_work_turn(Script::reports_success())
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(1, |d| {
            d.resumes_session("s1").field_contains("correction", "required shape")
        });
}

// GRL-FR-QKDC, GRL-FR-MRVK: a `merge_review` turn is corrected in its own
// session and checkout.
#[test]
fn a_refused_merge_review_answer_is_corrected() {
    let outcome = conflicted_merge("merge review corrected")
        .with_merge(as_one_commit())
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_refused(EnvelopeInvalid::NotFound))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run();
    let checkout = outcome.dispatches()[1].directory.clone();
    outcome.expect_state(GraduationRunState::Completed).expect_dispatch_count(3).expect_dispatch(
        2,
        |d| {
            d.directory(&checkout)
                .resumes_session("s1")
                .field_contains("correction", "No answer was found")
        },
    );
}

// GRL-FR-QEUP, GRL-FR-HWLD, GRL-FR-BIQX: a review correction that is pending
// across Continue is sent in the refused turn's session, and stands in a new
// review checkout rather than in the refused turn's.
#[test]
fn a_review_correction_after_continue_stands_in_a_new_checkout() {
    let rested = Scenario::named("review correction after continue")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_refused(EnvelopeInvalid::Malformed))
        .review_turn(Script::is_cancelled().pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause");
    let home = rested.run_home_path();
    let refused_in = rested.dispatches()[1].directory.clone();
    assert!(!refused_in.exists(), "the refused turn's checkout went");
    rested
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.part("review")
                .resumes_session("s1")
                .field_contains("correction", "required shape")
                .directory_is_review_checkout_under(&home)
                .directory_is_not(&refused_in)
        })
        .expect_review_checkout_reclaimed();
}

// GRL-FR-QEUP, GRL-FR-PTCE: where the vendor does not honour the resume of a
// review correction sent after Continue, the refused turn settles as it ended.
#[test]
fn a_review_correction_after_continue_that_cannot_resume_settles_the_refused_turn() {
    Scenario::named("review correction after continue, not resumed")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_refused(EnvelopeInvalid::Malformed))
        .review_turn(Script::is_cancelled().pausing_the_run())
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .continued(vec![Script::cannot_resume()])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_no_pending_correction();
}
