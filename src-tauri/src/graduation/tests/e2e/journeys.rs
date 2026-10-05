//! Journeys 1 to 9 of the coverage matrix
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Each test is one journey, written in the scenario language of
//! `scenario.rs`. Journey 10 stands in `binary_protocol.rs` and journey 11 in
//! `budget.rs`, both because they need a collaborator these do not.

use super::scenario::{finding, Scenario, Setting, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};
use crate::tools::agent_exec::{AgentExecutionError, DirectoryProblem, TurnKind};

// ---------------------------------------------------------------------------
// 1 — a ready review completes the run
// ---------------------------------------------------------------------------

// GTE-FR-QVHM, GTE-FR-BWKD, GTE-FR-ZPNC, GTE-FR-MDVQ, GTE-FR-PFEO / GRL-FR-VYNX,
// GRL-FR-ARPX, GRD-FR-ARLT, GRD-FR-BNTC: one pass is a work turn followed by a
// review turn, and a `ready` verdict commits exactly what changed.
#[test]
fn a_ready_review_completes_the_run_and_commits_only_what_changed() {
    Scenario::named("ready first pass")
        .with_stream("editor")
        .with_prompt("Add the empty state to the panel.")
        .work_turn(Script::reports_success().writes("src/panel.ts", "export const panel = 1;\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(1)
        .expect_turns(1, 1)
        .expect_dispatch_order(&["work", "review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released()
        .expect_dispatch(0, |d| {
            d.part("work")
                .purpose("generate")
                .pass(1)
                .turn_kind(TurnKind::Work)
                .field("prompt", "Add the empty state to the panel.")
                .has_no_field("loop_instruction");
        })
        .expect_dispatch(1, |d| {
            d.part("review")
                .pass(1)
                .turn_kind(TurnKind::Review)
                .instruction(&crate::graduation::driver::prompts::REVIEW_PROMPT)
                .changed_paths(&["src/panel.ts"]);
        });
}

// GTE-FR-XKGB / GRL-FR-DXLU, GRL-FR-YKRI: each turn stands where its phase
// says it stands and carries that phase's whole instruction — the work turn in
// the stream's own working copy, the review turn in a throwaway checkout that
// is not the stream.
#[test]
fn each_turn_stands_where_its_phase_says_and_carries_that_phases_instruction() {
    let scenario = Scenario::named("each turn's directory and instruction")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run();
    let worktree = scenario.worktree().to_path_buf();

    scenario
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.part("work")
                .directory(&worktree)
                .instruction(&crate::graduation::driver::prompts::WORK_PROMPT);
        })
        .expect_dispatch(1, |d| {
            d.part("review")
                .directory_is_not(&worktree)
                .instruction(&crate::graduation::driver::prompts::REVIEW_PROMPT);
        });
}

// GTE-FR-HNSA / GRD-FR-ARLT: a turn that removes a path, and one that removes a
// whole directory, each have that removal read from the working copy and
// carried into the commit — a run that deleted something is not a run that
// silently kept it.
#[test]
fn a_turn_that_removes_a_path_has_the_removal_committed() {
    Scenario::named("a removal reaches the commit")
        // A directory the first pass creates, so the second can remove it whole
        // as a turn that reorganizes a module does.
        .work_turn(
            Script::reports_success()
                .writes("src/old/one.ts", "1\n")
                .writes("src/old/two.ts", "2\n"),
        )
        .review_turn(Script::answers_revise(
            ReviewSeverity::Major,
            "The module is in the wrong place.",
        ))
        .work_turn(
            Script::reports_success()
                .removes_dir("src/old")
                // `README.md` is the seed the fixture's repository already
                // holds, so removing it is a real deletion rather than the
                // removal of something this run created.
                .deletes("README.md")
                .writes("src/panel.ts", "1\n"),
        )
        .review_turn(Script::answers_ready("The module is where it belongs now."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        // Nothing of `src/old` survives, and the seed file is gone.
        .expect_committed_paths(&["README.md", "src/panel.ts"])
        .expect_dispatch(3, |d| d.changed_paths(&["README.md", "src/panel.ts"]));
}

// GTE-FR-HNSA / FSA-FR-ZUCF: a symbolic link a turn installs — which is what a
// project's own dependency install writes — is read back and committed like any
// other path, rather than stopping the run.
#[cfg(unix)]
#[test]
fn a_symbolic_link_a_turn_installs_is_read_back_like_any_other_path() {
    Scenario::named("a symbolic link the turn installed")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .links("src/alias.ts", "panel.ts"),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .expect_committed_paths(&["src/alias.ts", "src/panel.ts"]);
}

// GTE-FR-MDVQ / GRD-FR-ARLT, GRL-FR-NVBZ: what the commit holds is read from
// the working copy, and an envelope that claims a change set of its own is not
// evidence about the filesystem.
#[test]
fn a_commit_holds_what_the_working_copy_holds_and_not_what_an_envelope_claimed() {
    Scenario::named("claimed paths are not evidence")
        .work_turn(Script::claims_paths(&["src/invented.ts", "src/also-invented.ts"]))
        .review_turn(Script::answers_ready("Nothing was asked for and nothing changed."))
        .run()
        .expect_state(GraduationRunState::Completed)
        // The turn wrote nothing, so there is nothing to commit however much
        // its summary claimed.
        .expect_commits(0)
        .expect_committed_paths(&[])
        .expect_dispatch(1, |d| d.changed_paths(&[]));
}

// ---------------------------------------------------------------------------
// 2 — a revise review, then a ready one
// ---------------------------------------------------------------------------

// GTE-FR-XKGB / GRL-FR-ARPX, GRL-FR-GQAB: a `revise` verdict starts one further
// pass, and the findings travel whole into the work turn that answers them.
#[test]
fn a_revise_review_sends_its_findings_into_the_next_work_turn() {
    Scenario::named("revise then ready")
        .work_turn(Script::reports_success().writes("src/panel.ts", "export const panel = 1;\n"))
        .review_turn(Script::answers_revise(
            ReviewSeverity::Major,
            "The empty state is not there.",
        ))
        .work_turn(Script::reports_success().writes("src/panel.ts", "export const panel = 2;\n"))
        .review_turn(Script::answers_ready("The empty state is there now."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_turns(2, 2)
        .expect_dispatch_order(&["work", "review", "work", "review"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released()
        .worktree_holds("src/panel.ts", "export const panel = 2;\n")
        .expect_dispatch(2, |d| {
            d.part("work")
                .purpose("revise")
                .pass(2)
                .field_contains("loop_instruction", "The empty state is not there.");
        });
}

// ---------------------------------------------------------------------------
// 3 — two substantive revise reviews
// ---------------------------------------------------------------------------

// GTE-FR-TCUW, GTE-FR-PFEO / GRL-FR-ARPX, GRL-FR-XBUE, GRD-FR-WKMD: a second
// `revise` verdict rests the run for the author with the findings, and no
// third pass is composed.
#[test]
fn two_revise_reviews_rest_the_run_for_the_author_with_no_third_pass() {
    Scenario::named("two revise reviews")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(
            ReviewSeverity::Major,
            "The empty state is not there.",
        ))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_revise(
            ReviewSeverity::Critical,
            "The empty state is still not there.",
        ))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(2)
        .expect_pass_window(1, 2)
        .expect_turns(2, 2)
        .expect_dispatch_count(4)
        .expect_dispatch_order(&["work", "review", "work", "review"])
        .expect_loop_instruction_contains("The empty state is still not there.")
        .expect_no_escalation()
        .expect_rest_reason("pass_budget_exhausted", 2)
        // GRL-FR-BRJO: the loop creates no commit, and a run resting for the
        // author has not been through the commit path either.
        .expect_commits(0)
        .expect_stream_released();
}

// ---------------------------------------------------------------------------
// 4 — an advisory ready
// ---------------------------------------------------------------------------

// GTE-FR-XKGB / GRL-FR-UQNV: a `revise` verdict of at most two findings, all of
// severity `minor`, ends the loop as `ready` does and composes no further work
// turn.
#[test]
fn two_minor_findings_complete_the_run_as_advisory_remarks() {
    Scenario::named("advisory ready")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise_with(vec![
            finding(ReviewSeverity::Minor, "The comment could be shorter."),
            finding(ReviewSeverity::Minor, "The import order is unusual."),
        ]))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(1)
        .expect_turns(1, 1)
        .expect_dispatch_count(2)
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .expect_stream_released();
}

// GTE-FR-WUAP / GRL-FR-UQNV: three minor findings are past the bound, so they start a pass
// like any other `revise` verdict rather than ending the loop.
#[test]
fn three_minor_findings_are_a_revision_rather_than_a_remark() {
    Scenario::named("three minor findings")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise_with(vec![
            finding(ReviewSeverity::Minor, "The comment could be shorter."),
            finding(ReviewSeverity::Minor, "The import order is unusual."),
            finding(ReviewSeverity::Minor, "The name could be plainer."),
        ]))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_ready("Every remark is answered."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_turns(2, 2);
}

// ---------------------------------------------------------------------------
// 5 — a work turn that reports a failure
// ---------------------------------------------------------------------------

// GTE-FR-XKGB / GRL-FR-DNKA: an agent-reported failure is material rather than
// an outcome. The turn completed, so the run reaches review with the failure as
// context, and the review's verdict decides what happens next.
#[test]
fn an_agent_reported_failure_reaches_the_review_that_judges_what_it_wrote() {
    Scenario::named("agent failure, review says revise")
        .work_turn(
            Script::reports_failure("The panel module does not exist to add a state to.")
                .writes("src/notes.md", "what I found\n"),
        )
        .review_turn(Script::answers_revise(
            ReviewSeverity::Critical,
            "Create the panel module first.",
        ))
        .work_turn(Script::reports_success().writes("src/panel.ts", "export const panel = 1;\n"))
        .review_turn(Script::answers_ready("The module is there now."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_turns(2, 2)
        .expect_committed_paths(&["src/notes.md", "src/panel.ts"])
        .expect_dispatch(1, |d| {
            d.part("review")
                .field_contains(
                    "agent_account",
                    "The panel module does not exist to add a state to.",
                )
                .changed_paths(&["src/notes.md"]);
        });
}

// GTE-FR-TCUW / GRL-FR-DNKA: the review's verdict is what decides the next state, so the same
// reported failure under a `ready` verdict completes the run.
#[test]
fn an_agent_reported_failure_the_review_passes_still_completes_the_run() {
    Scenario::named("agent failure, review says ready")
        .work_turn(
            Script::reports_failure("The formatter would not run, so the file is unformatted.")
                .writes("src/panel.ts", "export const panel=1;\n"),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_agent_account_contains("The formatter would not run")
        .expect_commits(1)
        .expect_stream_released();
}

// ---------------------------------------------------------------------------
// 6 — an escalation, and the answer that continues it
// ---------------------------------------------------------------------------

// GTE-FR-TCUW, GTE-FR-PFEO / GRL-FR-VBCL, GXD-FR-HGSU, GXD-FR-BJYT,
// GXD-FR-XPUR: an escalation ends the turn without a review, persists the
// ordered questions, releases the stream, and the answers reach the next turn
// of the same pass.
#[test]
fn an_escalation_rests_the_run_and_its_answers_continue_the_same_pass() {
    Scenario::named("escalation and answer")
        .work_turn(
            Script::escalates(&[
                "Which panel is meant, the editor's or the library's?",
                "Should the empty state be translated?",
            ])
            .leaves_a_session(),
        )
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_pass(1)
        .expect_dispatch_count(1)
        .expect_escalation_questions(&[
            "Which panel is meant, the editor's or the library's?",
            "Should the empty state be translated?",
        ])
        .expect_stream_released()
        .expect_commits(0)
        .answered(
            &["The editor's.", "No."],
            vec![
                Script::reports_success().writes("src/panel.ts", "export const panel = 1;\n"),
                Script::answers_ready("The work answers the prompt."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        // GXD-FR-XPUR: the pass does not advance for an answered escalation.
        .expect_pass(1)
        .expect_no_escalation()
        .expect_committed_paths(&["src/panel.ts"])
        .expect_dispatch(0, |d| {
            d.part("work")
                .purpose("escalation_answer")
                .pass(1)
                .escalation_answers(&["The editor's.", "No."]);
        });
}

// GTE-FR-TCUW, GTE-FR-PFEO / GRL-FR-VBCL: a review turn escalates on the same
// terms a work turn does.
#[test]
fn a_review_turn_escalates_on_the_same_terms_a_work_turn_does() {
    Scenario::named("review escalation")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::escalates(&["Is the panel's own test suite the one to run?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_escalation_questions(&["Is the panel's own test suite the one to run?"])
        .expect_stream_released()
        .expect_commits(0);
}

// ---------------------------------------------------------------------------
// 7 — an unreadable verdict
// ---------------------------------------------------------------------------

// GTE-FR-XKGB / GRL-FR-TVXI: a verdict the application cannot read asks the
// review again **once**, carrying the correction, and composes no work turn. A
// second unreadable verdict blocks the run with the code the spec names.
#[test]
fn a_second_unreadable_verdict_blocks_the_run_with_the_named_code() {
    Scenario::named("invalid verdict twice")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("review_verdict_invalid")
        // No work turn is composed between the two reviews: the work did not
        // change, so there is nothing for one to do.
        .expect_dispatch_order(&["work", "review", "review"])
        .expect_turns(1, 2)
        .expect_commits(0)
        // GRD-FR-BNTC: a run resting `blocked` keeps its stream, because the
        // work it left standing is what the author clears the condition
        // against.
        .expect_stream_held()
        .expect_dispatch(1, |d| d.has_no_field("correction"))
        .expect_dispatch(2, |d| {
            d.part("review")
                .field_contains("correction", "could not be read");
        });
}

// GTE-FR-XKGB / GRL-FR-TVXI: one unreadable verdict followed by a readable one is
// a run that
// completes, and the refusal count is spent rather than remembered.
#[test]
fn one_unreadable_verdict_is_asked_again_and_the_run_goes_on() {
    Scenario::named("one invalid verdict")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_turns(1, 2)
        .expect_commits(1)
        .expect_stream_released();
}

// ---------------------------------------------------------------------------
// 8 — cancellation, timeout, and a turn that could not be launched
// ---------------------------------------------------------------------------

// GTE-FR-RSQD, GTE-FR-PFEO / GRL-FR-ZDKP, GRD-FR-SWOJ, GXD-FR-QGYA,
// GXD-FR-NRWD, GRD-FR-PUXO, GLG-FR-RLQZ, GXD-FR-WJOW: a turn
// stopped where it stood leaves the run interrupted, commits what it wrote as
// an abandoned turn, keeps the checkpoint, and releases the stream. The
// interruption is produced by the harness rather than waited for.
#[test]
fn a_cancelled_turn_commits_its_work_as_abandoned_and_releases_the_stream() {
    Scenario::named("cancellation")
        .work_turn(Script::is_cancelled().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("retryable_failure")
        .expect_interruption_detail_contains("committed as an abandoned turn")
        .expect_interrupted_record(
            "retryable_failure",
            "error",
            &[("process_outcome", serde_json::json!("cancelled"))],
        )
        .expect_dispatch_count(1)
        .expect_commits(1)
        .expect_committed_paths(&["src/half-done.ts"])
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_stream_released()
        // GXD-FR-QGYA: the checkpoint is written before the run is reported as
        // stopped, so what the turn wrote is readable from the store rather
        // than only from the commit.
        .expect_checkpoint_paths(&["src/half-done.ts"])
        .expect_pass_window(1, 2);
}

// GTE-FR-RSQD, GTE-FR-KAZX / GRD-FR-SWOJ, GXD-FR-XEUX, GXD-FR-UZHX,
// GXD-FR-WJOW, GLG-FR-RLQZ: a turn that ran out of its execution timeout rests
// the run rather than failing it, and the run says it was the time limit.
#[test]
fn a_timed_out_turn_rests_the_run_rather_than_failing_it() {
    Scenario::named("timeout")
        .with_setting(Setting::ExecutionTimeoutMs, 1_000)
        .work_turn(Script::times_out().writes("src/half-done.ts", "as far as it got\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("execution_timeout")
        .expect_interruption_detail_contains("reached the execution time limit of 1 s")
        .expect_interruption_detail_contains("committed as an abandoned turn")
        .expect_interrupted_record(
            "execution_timeout",
            "error",
            &[
                ("process_outcome", serde_json::json!("timeout")),
                ("limit_ms", serde_json::json!(1_000)),
                ("duration_ms", serde_json::json!(0)),
            ],
        )
        .expect_interrupted_record_lacks("exit_code")
        .expect_commits(1)
        .expect_commit_message_contains(0, "Abandoned graduation turn")
        .expect_checkpoint_paths(&["src/half-done.ts"])
        .expect_stream_released()
        // GXD-FR-MTVR: the turn ran under the timeout the project configured.
        .expect_dispatch(0, |d| d.execution_timeout_ms(1_000));
}

// GTE-FR-TCUW / GXD-FR-XEUX, GXD-FR-UZHX, GXD-FR-WJOW, GLG-FR-RLQZ: **the seam
// ends no run.** A pre-launch error a retry cannot get past rests the run
// rather than making it terminal, and the author reads one sentence for it.
#[test]
fn a_turn_that_could_not_be_launched_rests_the_run_and_ends_nothing() {
    Scenario::named("unlaunchable turn")
        .work_turn(Script::cannot_launch(
            AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Inaccessible),
        ))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_interruption("launch_failed")
        .expect_interruption_detail("The run's working directory could not be used.")
        .expect_interrupted_record("launch_failed", "error", &[])
        .expect_interrupted_record_lacks("process_outcome")
        .expect_dispatch_count(1)
        .expect_stream_released()
        .expect_commits(0);
}

// ---------------------------------------------------------------------------
// 9 — resuming from the checkpoint
// ---------------------------------------------------------------------------

// GTE-FR-TCUW / GRL-FR-ISIL, GXD-FR-PWYD, GRD-FR-CYIB: a stopped run continues
// from its checkpoint. It repeats no completed pass, keeps the change set it
// already holds, and dispatches with the resume purpose.
#[test]
fn a_stopped_run_resumes_from_its_checkpoint_without_repeating_a_pass() {
    Scenario::named("resume from checkpoint")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise(
            ReviewSeverity::Major,
            "The empty state is not there.",
        ))
        // The second pass is stopped where it stands.
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "2\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .expect_pass(2)
        .expect_loop_instruction_contains("The empty state is not there.")
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "3\n"),
            Script::answers_ready("The empty state is there now."),
        ])
        .expect_state(GraduationRunState::Completed)
        // The pass the run resumed at is the one it was in, not the one after.
        .expect_pass(2)
        .expect_dispatch_order(&["work", "review"])
        .expect_dispatch(0, |d| {
            d.part("work")
                // The turn is dispatched for the revision it was stopped in the
                // middle of, which is what the checkpoint's own instruction
                // says it is for.
                .purpose("revise")
                .pass(2)
                // The scripted context survives the interruption: the findings
                // the first review gave still reach the resumed turn.
                .field_contains("loop_instruction", "The empty state is not there.");
        });

    // GRL-FR-ISIL: a run interrupted before any review has no instruction to
    // answer, so the turn it is handed back with is the resume purpose itself.
    Scenario::named("resume with no instruction")
        .work_turn(Script::is_cancelled().writes("src/panel.ts", "1\n"))
        .run()
        .expect_state(GraduationRunState::Interrupted)
        .continued(vec![
            Script::reports_success().writes("src/panel.ts", "2\n"),
            Script::answers_ready("The work answers the prompt."),
        ])
        .expect_state(GraduationRunState::Completed)
        .expect_pass(1)
        .expect_dispatch(0, |d| d.part("work").purpose("resume").pass(1));
}

// GTE-FR-XKGB / GXD-FR-TJRV: a run that stopped in the **review** resumes there
// and composes
// no work turn, because the work did not change while the run rested.
#[test]
fn a_run_that_blocked_in_the_review_resumes_in_the_review() {
    Scenario::named("resume at the review")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("review_verdict_invalid")
        .continued(vec![Script::answers_ready("The work answers the prompt.")])
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["review"])
        .expect_pass(1);
}
