//! Journey 43 of the coverage matrix: a refused answer is asked again once, in
//! the session of the turn that gave it
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::GraduationRunState;
use crate::tools::agent_exec::protocol::EnvelopeInvalid;
use crate::tools::agent_exec::{AgentExecutionError, DirectoryProblem};

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
        .expect_committed_paths(&["src/panel.ts"]);
}

// GRL-FR-JVNB, GXD-FR-UZHX, GXD-FR-WJOW, GLG-FR-RLQZ: a second refused answer
// rests the run on `unreadable_answer`. The detail and the record name the
// refusal, and one commit holds the work of both turns.
#[test]
fn a_second_refused_answer_rests_the_run_with_one_commit() {
    Scenario::named("refused twice")
        .work_turn(Script::answers_refused(EnvelopeInvalid::TooLarge).writes("src/panel.ts", "1\n"))
        .work_turn(Script::answers_refused(EnvelopeInvalid::TooLarge).writes("src/panel.ts", "2\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("unreadable_answer")
        .expect_interruption_detail_contains("a value in the answer was too long")
        .expect_interrupted_record(
            "unreadable_answer",
            "error",
            &[("refusal", serde_json::json!("too_large"))],
        )
        .expect_dispatch_count(2)
        .expect_turns(2, 0)
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);
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
            .expect_turns(1, 0);
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
    outcome
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(3)
        .expect_dispatch(2, |d| {
            d.part("review")
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
        .expect_commits(1);
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
