//! Journey 21 of the coverage matrix: the escalation paths journey 6 does not
//! take — sessions, partial answers, a review's answers, escalations outside
//! the rules, and an escalation on a later pass
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Scenario, TurnScript as Script};
use crate::graduation::{GraduationEscalationOrigin, GraduationRunState, ReviewSeverity};

// GTE-FR-XKGB / GXD-FR-XPUR, GXD-FR-BJYT: a work turn's answers go into the
// continuation of the turn that asked, resuming its vendor session where it
// left one and resuming none where it did not. The review after them resumes
// no session.
#[test]
fn an_answered_escalation_resumes_the_session_that_asked() {
    Scenario::named("answers resume the session")
        .work_turn(Script::escalates(&["Which panel is meant?"]).leaves_a_session())
        .run()
        .expect_escalation_origin(GraduationEscalationOrigin::Work)
        .answered(
            &["The editor's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.purpose("escalation_answer").resumes_session("s1"))
        .expect_dispatch(1, |d| d.part("review").resumes_no_session())
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);

    Scenario::named("answers with no session to resume")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .answered(
            &["The editor's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.purpose("escalation_answer").resumes_no_session())
        .expect_commits(1);
}

// GTE-FR-RZKC / GXD-FR-BJYT: a set that does not answer every question is
// refused and records nothing. The whole set then continues the run.
#[test]
fn a_partial_answer_is_refused_and_changes_nothing() {
    Scenario::named("partial answers")
        .work_turn(Script::escalates(&["Which panel is meant?", "Should it be translated?"]))
        .run()
        .refusing_answers(&["The editor's."], "the answers do not cover every question")
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Which panel is meant?", "Should it be translated?"])
        .answered(
            &["The editor's.", "No."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| d.escalation_answers(&["The editor's.", "No."]))
        .expect_commits(1);
}

// GTE-FR-XKGB / GRL-FR-VBCL, GXD-FR-XPUR, GXD-FR-TJRV: a review turn's answers
// go into a fresh review turn. No work turn is composed, no session is
// resumed, and the pass does not advance.
#[test]
fn an_answered_review_escalation_asks_a_fresh_review_turn() {
    Scenario::named("review escalation answered")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::escalates(&["Is the panel's own test suite the one to run?"])
                .leaves_a_session(),
        )
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_origin(GraduationEscalationOrigin::Review)
        .expect_stream_held()
        .expect_commits(0)
        .answered(&["Yes."], vec![Script::answers_ready("The panel's suite passes.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_turns(1, 2)
        .expect_pass(1)
        .expect_dispatch(0, |d| {
            d.part("review")
                .escalation_answers(&["Yes."])
                .resumes_no_session()
                .changed_paths(&["src/panel.ts"])
        })
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);
}

// GTE-FR-XKGB / GXD-FR-XPUR, GRL-FR-GQAB: the answers a review asked for are
// spent once that review returns a verdict. A `revise` verdict starts a work
// turn that carries the findings and no answers.
#[test]
fn a_review_escalation_answered_into_a_revision_sends_no_answers_to_the_work_turn() {
    Scenario::named("review escalation answered, then revise")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::escalates(&["Is the empty state required?"]).leaves_a_session())
        .run()
        .answered(
            &["Yes."],
            vec![
                Script::answers_revise(ReviewSeverity::Major, "The empty state is not there."),
                Script::reports_success().writes("src/panel.ts", "2\n"),
                Script::answers_ready("The empty state is there now."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review", "work", "review"])
        .expect_pass(2)
        .expect_dispatch(1, |d| {
            d.purpose("revise")
                .escalation_answers(&[])
                .resumes_no_session()
                .field_contains("loop_instruction", "The empty state is not there.")
        })
        .expect_dispatch(2, |d| d.escalation_answers(&[]))
        .worktree_holds("src/panel.ts", "2\n");
}

// GTE-FR-XKGB / GXD-FR-XPUR, GRL-FR-TVXI: answers are cleared by a readable
// verdict alone, so the review asked again after an unreadable one still
// carries them.
#[test]
fn a_review_escalation_answered_into_an_unreadable_verdict_keeps_the_answers() {
    Scenario::named("review escalation answered, then unreadable")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::escalates(&["Is the empty state required?"]))
        .run()
        .answered(
            &["Yes."],
            vec![
                Script::answers_unreadably(),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review", "review"])
        .expect_dispatch(1, |d| {
            d.escalation_answers(&["Yes."]).field_contains("correction", "could not be read")
        })
        .expect_commits(1);
}

// GTE-FR-ZPNC / GRL-FR-VBCL, GRL-FR-TVXI: an escalation outside the rules is
// recorded nowhere. A work turn's goes on to its review; a review turn's is a
// result the application could not read, and the review is asked again.
#[test]
fn an_escalation_outside_the_rules_is_not_recorded() {
    Scenario::named("blank reason on a work turn")
        .work_turn(
            Script::escalates_because("   ", &["Which panel is meant?"])
                .writes("src/panel.ts", "1\n"),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_no_escalation()
        .expect_dispatch_order(&["work", "review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"]);

    Scenario::named("nine questions on a review turn")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::escalates(&["1?", "2?", "3?", "4?", "5?", "6?", "7?", "8?", "9?"]))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_no_escalation()
        .expect_turns(1, 2)
        .expect_dispatch(1, |d| d.has_no_field("correction"))
        .expect_dispatch(2, |d| d.field_contains("correction", "could not be read"))
        .expect_commits(1);
}

// GTE-FR-XKGB / GXD-FR-XPUR, GXD-FR-PWYD, GRL-FR-GQAB: an escalation on the
// second pass keeps that pass and the findings it answers, and its answers
// reach the turn that continues it.
#[test]
fn an_escalation_on_the_second_pass_keeps_the_pass_and_the_findings() {
    Scenario::named("escalation on pass two")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "The empty state is not there."))
        .work_turn(Script::escalates(&["Which empty state is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(2)
        .expect_loop_instruction_contains("The empty state is not there.")
        .answered(
            &["The editor's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "2\n"),
                Script::answers_ready("The empty state is there now."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_dispatch(0, |d| {
            d.purpose("escalation_answer")
                .pass(2)
                .escalation_answers(&["The editor's."])
                .field_contains("loop_instruction", "The empty state is not there.")
        })
        .expect_commits(1)
        .worktree_holds("src/panel.ts", "2\n");
}

// GTE-FR-HWQM / GXD-FR-XPUR, GXD-FR-TJRV, GRD-FR-MDQZ: the answers a review
// asked for are cleared by a readable verdict alone. A pause during the review
// that carries them keeps them, and the review Continue resumes carries them
// again.
#[test]
fn a_pause_during_the_answered_review_keeps_the_answers_for_the_resumed_review() {
    Scenario::named("pause during the answered review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::escalates(&["Is the panel's own test suite the one to run?"]))
        .run()
        .answered(&["Yes."], vec![Script::answers_ready("The suite passes.").pausing_the_run()])
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("author_pause")
        .expect_resume_phase("review")
        .expect_dispatch(0, |d| d.part("review").escalation_answers(&["Yes."]))
        .continued(vec![Script::answers_ready("The suite passes.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_dispatch(0, |d| d.escalation_answers(&["Yes."]))
        .expect_turns(1, 3)
        .expect_commits(1);
}

// GTE-FR-RZKC / GXD-FR-BJYT: a run that asks nothing takes no answers, and the
// refusal changes nothing.
#[test]
fn answers_to_a_run_that_asks_nothing_are_refused() {
    Scenario::named("answers to a run that asks nothing")
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_no_escalation()
        .refusing_answers(&["Anything."], "run_state_not_permitted")
        .expect_commits(1);
}

// GTE-FR-XKGB / GXD-FR-NWSL, GXD-FR-BTIN, GRL-FR-BNDD, GRD-FR-KQLD: every answer enters the
// run's decision ledger, and every later turn of every pass carries the whole
// ledger in order, the turn that asked and its pass included.
#[test]
fn every_later_turn_carries_the_whole_decision_ledger() {
    Scenario::named("ledger carried through passes")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_ledger(&[])
        .answered(
            &["The editor's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::escalates(&["Is the empty state required?"]),
            ],
        )
        .expect_ledger(&[("Which panel is meant?", "The editor's.", "work", 1)])
        .expect_dispatch(0, |d| {
            d.decisions(&[("Which panel is meant?", "The editor's.", "work", 1)])
        })
        .expect_dispatch(1, |d| {
            d.part("review").decisions(&[("Which panel is meant?", "The editor's.", "work", 1)])
        })
        .answered(
            &["Yes."],
            vec![
                Script::answers_revise(ReviewSeverity::Major, "The empty state is not there."),
                Script::reports_success().writes("src/panel.ts", "2\n"),
                Script::answers_ready("The empty state is there now."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_ledger(&[
            ("Which panel is meant?", "The editor's.", "work", 1),
            ("Is the empty state required?", "Yes.", "review", 1),
        ])
        .expect_dispatch_order(&["review", "work", "review"])
        .expect_dispatch(0, |d| {
            d.decisions(&[
                ("Which panel is meant?", "The editor's.", "work", 1),
                ("Is the empty state required?", "Yes.", "review", 1),
            ])
        })
        .expect_dispatch(1, |d| {
            d.decisions(&[
                ("Which panel is meant?", "The editor's.", "work", 1),
                ("Is the empty state required?", "Yes.", "review", 1),
            ])
        })
        .expect_dispatch(2, |d| {
            d.pass(2).decisions(&[
                ("Which panel is meant?", "The editor's.", "work", 1),
                ("Is the empty state required?", "Yes.", "review", 1),
            ])
        });
}

// GTE-FR-XKGB / GXD-FR-HRDM: a question the author answered before and answers
// again adds a new entry, and the old one stays.
#[test]
fn a_repeated_question_adds_a_new_ledger_entry() {
    Scenario::named("repeated question")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .answered(&["The editor's."], vec![Script::escalates(&["Which panel is meant?"])])
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Which panel is meant?"])
        .answered(
            &["The sidebar's."],
            vec![
                Script::reports_success().writes("src/panel.ts", "1\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_ledger(&[
            ("Which panel is meant?", "The editor's.", "work", 1),
            ("Which panel is meant?", "The sidebar's.", "work", 1),
        ])
        .expect_dispatch(0, |d| {
            d.decisions(&[
                ("Which panel is meant?", "The editor's.", "work", 1),
                ("Which panel is meant?", "The sidebar's.", "work", 1),
            ])
        });
}

// GTE-FR-XKGB / GXD-FR-NWSL: a refused answer set records nothing in the ledger.
#[test]
fn a_refused_answer_set_adds_no_ledger_entry() {
    Scenario::named("refused answers leave the ledger alone")
        .work_turn(Script::escalates(&["Which panel is meant?", "Should it be translated?"]))
        .run()
        .refusing_answers(&["The editor's."], "the answers do not cover every question")
        .expect_ledger(&[]);
}

// GTE-FR-XKGB / GXD-FR-WRVH, GXD-FR-QPFT, GRL-FR-RHST, GRL-FR-VHIS: a work turn
// carries the findings of the passes before the latest review, and a review
// turn carries no review history. The latest findings stay the work turn's
// `loop_instruction`.
#[test]
fn a_work_turn_carries_the_findings_of_the_earlier_passes() {
    Scenario::named("review history reaches the work turn")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "The first problem."))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_revise(ReviewSeverity::Major, "The second problem."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_review_history_passes(&[1, 2])
        .expect_dispatch(2, |d| d.part("work").review_history_passes(&[]))
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("Both problems are fixed."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_review_history_passes(&[1, 2])
        .expect_dispatch(0, |d| {
            d.part("work")
                .review_history_passes(&[1])
                .field_contains("loop_instruction", "The second problem.")
        })
        .expect_dispatch(1, |d| d.part("review").has_no_field("review_history"));
}
