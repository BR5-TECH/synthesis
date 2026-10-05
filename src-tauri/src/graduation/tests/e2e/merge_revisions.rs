//! Journey 30 of the coverage matrix: the passes of a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! One pass is a `merge_work` turn then a fresh `merge_review` turn. The review's
//! findings go whole into the next work turn, every `revise` of a merge review
//! counts, and a `ready` review is followed by the apply and by no further turn.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::{GraduationRunState, ReviewSeverity};
use crate::streams::StreamMergePublication;

fn settling() -> Script {
    Script::reports_success()
        .writes("README.md", "both sides\n")
        .checking_the_live_trees_stand()
}

// GTE-FR-RDPE, GTE-FR-XKGB, GTE-FR-QVHM / GRL-FR-MWPQ, GRL-FR-TXEB, GRL-FR-SLQF,
// GRL-FR-MRVK, GRL-FR-GADT, GXD-FR-XQJR, GXD-FR-KXXB, GXD-FR-MKTZ, GRD-FR-AQNW,
// GRD-FR-KZPT, GRD-FR-YBUM: one pass of a merge run is a `merge_work` turn in the run's merge
// worktree and a `merge_review` turn in a review checkout. Each carries the merge
// context, its own instruction and the part and turn kind of a merge. A `ready`
// review is followed by the apply and by no further turn, and both branches and
// both working copies are untouched at each dispatch.
#[test]
fn a_ready_merge_review_applies_the_merge_after_one_pass_and_no_further_turn() {
    let outcome = conflicted_merge("one merge pass")
        .with_merge(as_one_commit())
        .merge_work_turn(settling().writes("src/glue.ts", "glue\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand.").checking_the_live_trees_stand())
        .run();
    let (worktree, checkout, stream_worktree) = (
        outcome.merge_worktree_path(),
        outcome.review_checkout_path(),
        outcome.worktree().to_path_buf(),
    );
    let data = outcome.merge_data();
    outcome
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(2)
        .expect_dispatch_order(&["merge_work", "merge_review"])
        .expect_dispatch(0, |d| {
            d.part("merge_work")
                .purpose("generate")
                .pass(1)
                .turn_kind(crate::tools::agent_exec::TurnKind::MergeWork)
                .directory(&worktree)
                .merge_work_instruction()
                .no_supplementary_mount()
                .merge_field("stream_branch", &data.stream_branch)
                .merge_field("base_branch", &data.base_branch)
                .merge_field("base_tip", &data.base_tip)
                .merge_field("stream_tip", &data.stream_tip)
                .merge_field("merge_base", &data.merge_base)
                .merge_field("snapshot_commit", &data.snapshot_commit)
                .merge_paths("unresolved_paths", &["README.md"])
                .merge_paths("changed_paths", &["README.md", "stream-only.txt"])
                .merge_conflict("README.md", "updated", "updated")
                .merge_has_no("reconciled_paths")
                .field("draft_name", "")
                .field_contains("prompt", "feature")
                .resumes_no_session()
        })
        .expect_dispatch(1, |d| {
            d.part("merge_review")
                .purpose("generate")
                .pass(1)
                .turn_kind(crate::tools::agent_exec::TurnKind::MergeReview)
                .directory(&checkout)
                .directory_is_not(&worktree)
                .directory_is_not(&stream_worktree)
                .merge_review_instruction()
                .no_supplementary_mount()
                .changed_paths(&["README.md", "src/glue.ts"])
                .merge_paths("unresolved_paths", &["README.md"])
                .merge_paths("changed_paths", &["README.md", "stream-only.txt"])
                .merge_paths("reconciled_paths", &["src/glue.ts"])
                .merge_field("snapshot_commit", &data.snapshot_commit)
        })
        .expect_pass(1)
        .expect_turns(1, 1)
        .expect_base_is_the_snapshot()
        .expect_review_checkout_reclaimed()
        .expect_merge_result("commit", true, &["README.md", "src/glue.ts", "stream-only.txt"])
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_base_holds("README.md", "both sides\n")
        .expect_base_holds("src/glue.ts", "glue\n")
        .expect_stream_released()
        .expect_merge_workspace_reclaimed()
        .expect_stream_row_merge_run(GraduationRunState::Completed);
}

// GTE-FR-RDPE, GTE-FR-XKGB / GRL-FR-MWPQ, GRL-FR-GQAB, GRL-FR-TXEB, GRD-FR-AQNW:
// a `revise` review sends its findings whole, in the order it returned them,
// into the next `merge_work` turn. The second pass is numbered 2 and carries the
// revision purpose, and its `ready` review applies the merge once.
#[test]
fn a_revise_merge_review_sends_its_findings_whole_into_the_next_merge_work_turn() {
    use super::scenario::finding;
    conflicted_merge("merge revision")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_revise_with(vec![
            finding(ReviewSeverity::Critical, "The limit of README.md lost the base side."),
            finding(ReviewSeverity::Major, "The stream-only file is not mentioned."),
        ]))
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides, kept\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review", "merge_work", "merge_review"])
        .expect_dispatch(2, |d| {
            d.part("merge_work")
                .purpose("revise")
                .pass(2)
                .field_contains("loop_instruction", "[critical] The limit of README.md lost the base side.")
                .field_contains("loop_instruction", "[major] The stream-only file is not mentioned.")
                .merge_paths("unresolved_paths", &["README.md"])
                .merge_has_no("reconciled_paths")
        })
        .expect_dispatch(3, |d| d.part("merge_review").pass(2).merge_review_instruction())
        .expect_pass(2)
        .expect_turns(2, 2)
        .expect_base_holds("README.md", "both sides, kept\n")
        .expect_merge_result("commit", true, &["README.md", "stream-only.txt"]);
}

// GTE-FR-HJGA, GTE-FR-RDPE / GRL-FR-MRVK, GRL-FR-UQNV, GRL-FR-MWPQ: a `revise`
// review of one minor finding is not advisory for a merge review. It starts a
// second pass, and a second one rests the run rather than applying. A draft run
// completes on the same verdict (journey 4).
#[test]
fn a_single_minor_finding_of_a_merge_review_still_starts_another_pass() {
    conflicted_merge("merge minor finding")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_revise(ReviewSeverity::Minor, "Name the glue file."))
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides, named\n"))
        .merge_review_turn(Script::answers_revise(ReviewSeverity::Minor, "Name it again."))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_dispatch_count(4)
        .expect_dispatch(2, |d| d.part("merge_work").pass(2).field_contains("loop_instruction", "Name the glue file."))
        .expect_pass(2)
        .expect_no_merge_result()
        .expect_commits(0)
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_working_copy_clean()
        .expect_stream_released();
}

// GTE-FR-CZPM, GTE-FR-RDPE / GRL-FR-NDHW, GRD-FR-AQNW, GRL-FR-MRVK: a `ready`
// review of a result that still holds a conflict marker in a path Git could not
// merge is not applied. The verdict is replaced by a `revise` of one `critical`
// finding that names the path, and the next pass carries it.
#[test]
fn a_ready_review_over_a_result_with_a_marker_is_replaced_by_a_critical_finding() {
    conflicted_merge("merge marker left")
        .with_merge(StreamMergePublication::Uncommitted)
        .merge_work_turn(Script::reports_success().writes("src/glue.ts", "glue\n"))
        .merge_review_turn(Script::answers_ready("It reads well."))
        .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_order(&["merge_work", "merge_review", "merge_work", "merge_review"])
        .expect_dispatch(2, |d| {
            d.part("merge_work")
                .purpose("revise")
                .pass(2)
                .field_contains("loop_instruction", "[critical]")
                .field_contains("loop_instruction", "README.md")
        })
        .expect_pass(2)
        .expect_turns(2, 2)
        .expect_merge_result("uncommitted", false, &["README.md", "src/glue.ts", "stream-only.txt"])
        .expect_base_working_copy_holds("README.md", "both sides\n", true)
        .expect_base_branch_at_the_pinned_tip();
}

// GTE-FR-RDPE, GTE-FR-XKGB / GRL-FR-TXEB, GRL-FR-GADT, GXD-FR-XQJR: a run that is
// not a merge run carries no `merge` context in any turn, its parts are `work` and
// `review`, and its instructions are `work.md` and `review.md` and never the
// instructions of a merge.
#[test]
fn an_ordinary_run_carries_no_merge_context_and_no_merge_instruction() {
    use crate::graduation::driver::prompts::{REVIEW_PROMPT, WORK_PROMPT};
    use crate::tools::agent_exec::TurnKind;

    super::scenario::Scenario::named("ordinary run")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch(0, |d| {
            d.part("work")
                .turn_kind(TurnKind::Work)
                .instruction(WORK_PROMPT.as_str())
                .has_no_merge()
        })
        .expect_dispatch(1, |d| {
            d.part("review")
                .turn_kind(TurnKind::Review)
                .instruction(REVIEW_PROMPT.as_str())
                .has_no_merge()
        });
}
