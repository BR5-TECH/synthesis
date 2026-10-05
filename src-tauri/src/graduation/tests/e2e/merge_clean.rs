//! Journey 28 of the coverage matrix: a merge that needs no run
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A merge Git settles on its own, and a stream that holds nothing its base does
//! not, finish in the request. They leave no run, hold no stream and no project
//! slot, and write the base branch only as the publication says. A merge the
//! request refuses writes nothing at all.

use super::scenario::Scenario;
use crate::graduation::GraduationRunState;
use crate::streams::StreamMergePublication;

fn as_one_commit() -> StreamMergePublication {
    StreamMergePublication::Commit {
        message: "Merge the editor stream".to_string(),
    }
}

// GTE-FR-TNZF, GTE-FR-QVHM, GTE-FR-MDVQ / WKS-FR-GKPX, WKS-FR-TVBM, WKS-FR-PZKD,
// GRD-FR-MRNQ: a stream Git merges on its own lands on the base branch as one
// commit under the author's message. No run is made, no turn is dispatched, and
// neither the stream nor a project slot is held afterwards.
#[test]
fn a_clean_merge_commits_once_and_leaves_no_run() {
    Scenario::named("clean merge, commit")
        .with_stream("editor")
        .with_stream_commit("src/panel.ts", "1\n")
        .with_merge(as_one_commit())
        .run()
        .expect_merged(&["src/panel.ts"], true)
        .expect_no_merge_run()
        .expect_dispatch_count(0)
        .expect_queue_offered_nothing()
        .expect_stream_released()
        .expect_stream_row_without_a_merge_run()
        .expect_base_head_message_contains("Merge the editor stream")
        .expect_base_holds("src/panel.ts", "1\n")
        .expect_base_working_copy_holds("src/panel.ts", "1\n", false)
        .expect_stream_branch_unchanged()
        .worktree_holds("src/panel.ts", "1\n");
}

// GTE-FR-TNZF, GTE-FR-MDVQ / WKS-FR-TVBM, WKS-FR-PZKD: an `uncommitted`
// publication leaves the result unstaged in the base working copy and moves
// neither branch. The merge made no run, and the next merge finds the base
// working copy dirty and is refused, writing nothing.
#[test]
fn a_clean_uncommitted_merge_leaves_the_result_unstaged_and_no_run() {
    Scenario::named("clean merge, uncommitted")
        .with_stream_commit("src/panel.ts", "1\n")
        .with_merge(StreamMergePublication::Uncommitted)
        .run()
        .expect_merged(&["src/panel.ts"], false)
        .expect_no_merge_run()
        .expect_dispatch_count(0)
        .expect_branches_unchanged()
        .expect_base_working_copy_holds("src/panel.ts", "1\n", true)
        .expect_stream_released()
        .merging(as_one_commit())
        .expect_merge_refused("base_dirty")
        .expect_no_merge_run()
        .expect_branches_unchanged();
}

// GTE-FR-TNZF, GTE-FR-QVHM / WKS-FR-FOTC, WKS-FR-PZKD: a stream that holds
// nothing its base does not hold reports `nothing_to_merge`, writes nothing, and
// makes no run, whatever the base gained meanwhile.
#[test]
fn a_stream_with_nothing_to_merge_reports_it_and_writes_nothing() {
    Scenario::named("nothing to merge")
        .with_base_commit("src/base.ts", "1\n")
        .without_a_draft_run()
        .run()
        .merging(as_one_commit())
        .expect_nothing_to_merge()
        .expect_no_merge_run()
        .expect_dispatch_count(0)
        .expect_queue_offered_nothing()
        .expect_live_state_unchanged()
        .expect_stream_released()
        .merging(StreamMergePublication::Uncommitted)
        .expect_nothing_to_merge()
        .expect_live_state_unchanged();
}

// GTE-FR-TNZF, GTE-FR-RZKC / WKS-FR-GKPX, WKS-FR-UZHT: a side that holds
// uncommitted work is refused with that side's code, and nothing is written. Both
// refusals are typed, and the merge goes through once the side is clean.
#[test]
fn a_merge_refused_for_a_dirty_side_writes_nothing() {
    Scenario::named("dirty sides")
        .with_stream_commit("src/panel.ts", "1\n")
        .without_a_draft_run()
        .run()
        .dirtying_the_stream("scratch.txt", "mine\n")
        .merging(as_one_commit())
        .expect_merge_refused("stream_dirty")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .expect_branches_unchanged()
        .cleaning_the_stream("scratch.txt")
        .dirtying_the_base("scratch.txt", "mine\n")
        .merging(as_one_commit())
        .expect_merge_refused("base_dirty")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .cleaning_the_base("scratch.txt")
        .merging(as_one_commit())
        .expect_merged(&["src/panel.ts"], true)
        .expect_no_merge_run();
}

// GTE-FR-TNZF, GTE-FR-YCQW, GTE-FR-RZKC / WKS-FR-GKPX, GRD-FR-DLWB: a stream that
// holds a run that has not ended refuses Merge with `stream_busy`, and nothing
// is written. The run is the stream's own, queued behind nothing.
#[test]
fn a_merge_refused_while_the_stream_holds_a_run_writes_nothing() {
    Scenario::named("stream busy")
        .with_stream("editor")
        .with_stream_commit("src/panel.ts", "1\n")
        .with_run_on_stream("waiting", "editor", "Write the header.")
        .without_a_draft_run()
        .run()
        .merging(as_one_commit())
        .expect_merge_refused("stream_busy")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .expect_queue_offered_nothing()
        .expect_state_of("waiting", GraduationRunState::Queued);
}

// GTE-FR-TNZF, GTE-FR-RZKC / WKS-FR-TSOA, WKS-FR-HLGN: a merge made while an
// update or another merge holds the repository update guard is refused with the
// code of the holder, and changes nothing. Once the guard is free it goes through.
#[test]
fn a_merge_refused_while_the_update_guard_is_held_writes_nothing() {
    Scenario::named("guard held")
        .with_stream_commit("src/panel.ts", "1\n")
        .without_a_draft_run()
        .run()
        .holding_the_update_guard_now()
        .merging(as_one_commit())
        .expect_merge_refused("update_in_progress")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .releasing_the_guard()
        .holding_another_merge_now()
        .merging(as_one_commit())
        .expect_merge_refused("merge_in_progress")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .releasing_the_guard()
        .merging(as_one_commit())
        .expect_merged(&["src/panel.ts"], true);
}

// GTE-FR-TNZF, GTE-FR-RZKC / WKS-FR-GKPX, WKS-FR-ETKW, WKS-FR-ZBHL: a stream
// whose working copy is gone, a stream nobody knows, a base branch no working
// copy holds and a project with no Git repository are each refused with their own
// typed code, and nothing is written.
#[test]
fn a_merge_refused_for_what_it_cannot_reach_writes_nothing() {
    Scenario::named("unreachable sides")
        .with_stream_commit("src/panel.ts", "1\n")
        .with_branch("elsewhere")
        .without_a_draft_run()
        .run()
        .merging_stream("no-such-stream", as_one_commit())
        .expect_merge_refused("unknown_stream")
        .expect_no_merge_run()
        .merging_without_a_repository(as_one_commit())
        .expect_merge_refused("not_a_git_repository")
        .expect_no_merge_run()
        .checking_out("elsewhere")
        .merging(StreamMergePublication::Uncommitted)
        .expect_merge_refused("base_not_checked_out")
        .expect_no_merge_run()
        .expect_branches_unchanged()
        .removing_the_working_copy()
        .merging(as_one_commit())
        .expect_merge_refused("stream_missing")
        .expect_no_merge_run();
}

// GTE-FR-TNZF, GTE-FR-YAEB / GRD-FR-MRNQ, WKS-FR-PZKD: a merge that needs no run
// never reaches the image preflight, so a machine that cannot execute an agent
// still merges what Git settles. The scripted seam saw no turn.
#[test]
fn a_clean_merge_needs_no_agent_and_no_image() {
    Scenario::named("clean merge, no agent")
        .with_stream_commit("src/panel.ts", "1\n")
        .without_a_draft_run()
        .run()
        .setting_the_machine_up_to(super::merge_acts::MachineSetup::Nothing)
        .merging(as_one_commit())
        .expect_merged(&["src/panel.ts"], true)
        .expect_no_merge_run()
        .expect_dispatch_count(0);
}
