//! Journeys 22 and 25 of the coverage matrix: what the change set holds and
//! what the review is told about it, and the verdicts the loop cannot read
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{finding, Scenario, TurnScript as Script};
use crate::graduation::{GraduationRunState, ReviewSeverity};

// ---------------------------------------------------------------------------
// 22 — the change set
// ---------------------------------------------------------------------------

// GTE-FR-HNSA, GTE-FR-MDVQ / GRL-FR-CKBL, GRD-FR-ARLT, GXD-FR-GMDI: output the
// repository's ignore rules hide is not committed, stays in the working copy,
// and is named to the review as what the rules kept out.
#[test]
fn ignored_output_is_not_committed_and_is_named_to_the_review() {
    Scenario::named("ignored output")
        .work_turn(
            Script::reports_success()
                .writes("src/panel.ts", "1\n")
                .writes("target/build.out", "built\n"),
        )
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(1, |d| d.changed_paths(&["src/panel.ts"]).hidden_paths(&["target/"]))
        .expect_hidden_paths(&["target/"])
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .worktree_holds("target/build.out", "built\n");
}

// GTE-FR-MDVQ / GRL-FR-YKRI, GRL-FR-XNQU: what a review turn writes in its
// throwaway checkout reaches neither the stream's working copy nor the commit.
#[test]
fn what_a_review_turn_writes_reaches_neither_the_stream_nor_the_commit() {
    Scenario::named("review writes")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(
            Script::answers_ready("The work answers the prompt.")
                .writes("src/panel.ts", "what the reviewer tried\n")
                .writes("review-notes.md", "notes\n"),
        )
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .expect_committed_paths(&["src/panel.ts"])
        .worktree_holds("src/panel.ts", "1\n")
        .expect_worktree_lacks("review-notes.md")
        .expect_review_checkout_reclaimed();
}

// ---------------------------------------------------------------------------
// 25 — verdicts the loop cannot read
// ---------------------------------------------------------------------------

fn ready_carrying_findings() -> Script {
    Script::answers_ready_with(vec![finding(ReviewSeverity::Minor, "The comment could be shorter.")])
}

// GTE-FR-XKGB / GRL-FR-VIAT, GRL-FR-TVXI: a `ready` verdict carrying findings, a
// `revise` verdict carrying none, and a verdict with a blank rationale are
// each refused whole, and the review is asked once more with the correction.
#[test]
fn a_verdict_that_breaks_its_own_shape_is_refused_and_asked_again() {
    let verdicts: [(&'static str, fn() -> Script); 3] = [
        ("ready carrying findings", ready_carrying_findings),
        ("revise carrying no finding", Script::answers_revise_without_findings),
        ("blank rationale", Script::answers_with_blank_rationale),
    ];
    for (name, verdict) in verdicts {
        Scenario::named(name)
            .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
            .review_turn(verdict())
            .review_turn(Script::answers_ready("The work answers the prompt."))
            .run()
            .expect_state(GraduationRunState::Completed)
            .expect_turns(1, 2)
            .expect_dispatch_order(&["work", "review", "review"])
            .expect_dispatch(2, |d| d.field_contains("correction", "could not be read"))
            .expect_commits(1)
            .expect_committed_paths(&["src/panel.ts"]);
    }
}

// GTE-FR-XKGB / GRL-FR-UQNV, GRL-FR-GQAB: a minor finding beside a major one is
// not an advisory remark, so the verdict starts a further pass.
#[test]
fn one_minor_and_one_major_finding_are_a_revision() {
    Scenario::named("mixed severities")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_revise_with(vec![
            finding(ReviewSeverity::Minor, "The comment could be shorter."),
            finding(ReviewSeverity::Major, "The empty state is not there."),
        ]))
        .work_turn(Script::reports_success().writes("src/panel.ts", "2\n"))
        .review_turn(Script::answers_ready("The empty state is there now."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_pass(2)
        .expect_turns(2, 2)
        .expect_dispatch(2, |d| {
            d.purpose("revise")
                .field_contains("loop_instruction", "The comment could be shorter.")
                .field_contains("loop_instruction", "The empty state is not there.")
        })
        .expect_commits(1)
        .worktree_holds("src/panel.ts", "2\n");
}
