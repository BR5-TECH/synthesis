//! Journey 34 of the coverage matrix: the apply step of a merge run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! Only a `ready` review leads to the apply. It writes the base branch on the
//! publication the author chose. A side that is not clean, a held repository
//! update guard and a failing write each rest the run `blocked` at the apply
//! phase, and Continue retries the apply alone.

use super::merge_arrange::{as_one_commit, conflicted_merge};
use super::scenario::TurnScript as Script;
use crate::graduation::GraduationRunState;
use crate::streams::StreamMergePublication;

fn settling() -> Script {
    Script::reports_success().writes("README.md", "both sides\n")
}

// GTE-FR-SFTU, GTE-FR-MDVQ / GRD-FR-AQNW, GRD-FR-BHCS, WKS-FR-HLGN: with `commit`
// the base branch holds one new merge commit that has the pinned base tip and
// the pinned stream tip as its two parents and carries the author's message. The
// run is `completed`, records the commit among its own and in `merge.result`, and
// no earlier step wrote the base branch.
#[test]
fn a_commit_publication_makes_one_merge_commit_with_both_pinned_tips_as_parents() {
    conflicted_merge("apply, commit")
        .with_merge(as_one_commit())
        .merge_work_turn(settling().checking_the_live_trees_stand())
        .merge_review_turn(Script::answers_ready("Both intents stand.").checking_the_live_trees_stand())
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(1)
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_merge_result("commit", true, &["README.md", "stream-only.txt"])
        .expect_base_working_copy_holds("README.md", "both sides\n", false)
        .expect_base_working_copy_holds("stream-only.txt", "kept\n", false)
        .expect_base_working_copy_clean()
        .expect_stream_branch_unchanged()
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_reclaimed()
        .expect_stream_row_merge_run(GraduationRunState::Completed);
}

// GTE-FR-SFTU, GTE-FR-MDVQ / GRD-FR-AQNW, GRD-FR-ARLT, WKS-FR-TVBM: with
// `uncommitted` the result stands unstaged in the base working copy and the base
// branch head is where it was. The run is `completed` with no commit of its own,
// and `merge.result` names the publication and the merged paths.
#[test]
fn an_uncommitted_publication_leaves_the_result_unstaged_in_the_base_working_copy() {
    conflicted_merge("apply, uncommitted")
        .with_merge(StreamMergePublication::Uncommitted)
        .merge_work_turn(settling().checking_the_live_trees_stand())
        .merge_review_turn(Script::answers_ready("Both intents stand."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .expect_commits(0)
        .expect_base_branch_at_the_pinned_tip()
        .expect_merge_result("uncommitted", false, &["README.md", "stream-only.txt"])
        .expect_base_working_copy_holds("README.md", "both sides\n", true)
        .expect_base_working_copy_holds("stream-only.txt", "kept\n", true)
        .expect_stream_released()
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-CZPM, GTE-FR-TPLD, GTE-FR-RZKC / GRD-FR-JSBE, GXD-FR-GMDI, GXD-FR-TJRV:
// the author leaves uncommitted work in the base working copy while
// the run reviews. The apply is refused with `merge_dirty_side`, the run rests
// `blocked` at the phase `apply` and holds its stream and its slot, and the base
// branch and working copy hold no part of the merge. Continue, once the base is
// clean, retries the apply alone: no turn is dispatched and no pass is spent.
#[test]
fn a_dirty_base_blocks_the_apply_and_continue_retries_it_alone() {
    conflicted_merge("apply, dirty base")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(
            Script::answers_ready("Both intents stand.").dirtying_the_base_meanwhile("scratch.txt", "mine\n"),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("merge_dirty_side")
        .expect_resume_phase("apply")
        .expect_stream_held()
        .expect_slots_in_use(1)
        .expect_dispatch_count(2)
        .expect_pass(1)
        .expect_turns(1, 1)
        .expect_no_merge_result()
        .expect_commits(0)
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_holds("README.md", "base side\n")
        .expect_merge_workspace_standing()
        .refusing_pause("run_state_not_permitted")
        .cleaning_the_base("scratch.txt")
        .continued(Vec::new())
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(0)
        .expect_pass(1)
        .expect_turns(1, 1)
        .expect_base_holds_the_merge_commit("Merge the feature stream")
        .expect_merge_result("commit", true, &["README.md", "stream-only.txt"])
        .expect_stream_released()
        .expect_slots_in_use(0)
        .expect_merge_workspace_reclaimed();
}

// GTE-FR-CZPM, GTE-FR-TPLD / GRD-FR-JSBE, WKS-FR-UZHT: the stream's working copy is
// a side too. Uncommitted work in it blocks the apply with `merge_dirty_side`, and
// Continue retries once it is clean.
#[test]
fn a_dirty_stream_blocks_the_apply_and_continue_retries_it() {
    conflicted_merge("apply, dirty stream")
        .with_merge(StreamMergePublication::Uncommitted)
        .merge_work_turn(settling())
        .merge_review_turn(
            Script::answers_ready("Both intents stand.").dirtying_the_stream_meanwhile("scratch.txt", "mine\n"),
        )
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("merge_dirty_side")
        .expect_resume_phase("apply")
        .expect_base_working_copy_clean()
        .expect_base_branch_at_the_pinned_tip()
        .cleaning_the_stream("scratch.txt")
        .continued(Vec::new())
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(0)
        .expect_merge_result("uncommitted", false, &["README.md", "stream-only.txt"]);
}

// GTE-FR-CZPM, GTE-FR-TPLD / GRD-FR-JSBE, WKS-FR-TSOA, WKS-FR-HLGN: an update that
// holds the repository update guard when the review ends blocks the apply with
// `merge_guard_held`. Nothing is written, and Continue applies once the guard is
// free.
#[test]
fn a_held_update_guard_blocks_the_apply_and_continue_retries_it() {
    conflicted_merge("apply, guard held")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_ready("Both intents stand.").holding_the_update_guard())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("merge_guard_held")
        .expect_resume_phase("apply")
        .expect_stream_held()
        .expect_no_merge_result()
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_working_copy_clean()
        .releasing_the_guard()
        .continued(Vec::new())
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(0)
        .expect_turns(1, 1)
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}

// GTE-FR-CZPM, GTE-FR-TPLD / GRD-FR-JSBE, WKS-FR-GKPX: a write Git refuses blocks
// the apply with `merge_apply_failed` and leaves no part of the result on the base
// branch or in its working copy. Continue retries once the obstruction is gone.
#[test]
fn a_failing_write_blocks_the_apply_and_continue_retries_it() {
    conflicted_merge("apply, write fails")
        .with_merge(as_one_commit())
        .merge_work_turn(settling())
        .merge_review_turn(Script::answers_ready("Both intents stand.").holding_the_base_index())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .expect_blocker("merge_apply_failed")
        .expect_resume_phase("apply")
        .expect_no_merge_result()
        .expect_base_branch_at_the_pinned_tip()
        .expect_base_holds("README.md", "base side\n")
        .expect_base_working_copy_clean()
        .releasing_the_base_index()
        .continued(Vec::new())
        .expect_state(GraduationRunState::Completed)
        .expect_dispatch_count(0)
        .expect_base_holds_the_merge_commit("Merge the feature stream");
}
