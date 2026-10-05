//! Journey 29 of the coverage matrix: the handoff of a merge Git cannot settle
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).
//!
//! A conflict creates one `queued` merge run, pinned to the tips and the
//! snapshot, and writes neither branch nor either working copy. The queue is
//! offered the run only after the repository update guard is free.

use super::merge_acts::MachineSetup;
use super::merge_arrange::{as_one_commit, bare_conflict, conflicted_merge as conflicted};
use crate::graduation::GraduationRunState;
use crate::streams::StreamMergePublication;

// GTE-FR-LMXV, GTE-FR-YAEB / GRD-FR-MRNQ, GRD-FR-VCTH, GRD-FR-KZPT, GRD-FR-BHCS,
// GRD-FR-OYPY, WKS-FR-GKPX, WKS-FR-KHJS: a conflict makes one run, named for the stream, in
// state `queued`. It pins both tips, the merge base and a snapshot commit, and
// records the changed paths, the unresolved paths and the publication. It holds
// no draft. Both branches and both working copies are what they were, and the
// queue is offered a dispatch of the run once the guard is free.
#[test]
fn a_conflict_hands_off_to_one_queued_merge_run_and_writes_nothing() {
    conflicted("handoff")
        .with_merge(as_one_commit())
        .run()
        .expect_conflicted(&["README.md"])
        .expect_state(GraduationRunState::Queued)
        .expect_merge_run_named("Merge feature")
        .expect_merge_paths(&["README.md", "stream-only.txt"], &["README.md"])
        .expect_publication(&as_one_commit())
        .expect_merge_snapshot(&["README.md"])
        .expect_no_merge_result()
        .expect_live_state_unchanged()
        .expect_branches_unchanged()
        .expect_runs_in_the_queue(1)
        .expect_queue_index_names_the_merge_run()
        .expect_listings_carry_the_merge_run()
        .expect_stream_row_merge_run(GraduationRunState::Queued)
        .expect_dispatch_count(0)
        .expect_stream_released()
        .expect_queue_offered_the_merge_run()
        .expect_the_guard_free();
}

// GTE-FR-LMXV / GRD-FR-KZPT, WKS-FR-GKPX: the publication the author chose is
// the one the run records, so an `uncommitted` request reaches the apply as one.
#[test]
fn the_handoff_records_an_uncommitted_publication() {
    conflicted("handoff, uncommitted")
        .with_merge(StreamMergePublication::Uncommitted)
        .run()
        .expect_conflicted(&["README.md"])
        .expect_publication(&StreamMergePublication::Uncommitted)
        .expect_live_state_unchanged()
        .expect_queue_offered_the_merge_run();
}

// GTE-FR-LMXV, GTE-FR-RZKC / GSU-FR-GLVQ, WKS-FR-GKPX: a machine that cannot
// execute an agent is refused by the image preflight before anything is
// captured. Each refusal makes no run, no snapshot ref and no worktree, and
// writes neither branch nor either working copy. A complete machine hands off.
#[test]
fn a_refused_image_preflight_creates_no_run_and_writes_nothing() {
    bare_conflict("handoff preflight")
        .without_a_draft_run()
        .run()
        .setting_the_machine_up_to(MachineSetup::Nothing)
        .merging(as_one_commit())
        .expect_merge_refused("vendor_execution_unsupported")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .expect_queue_offered_nothing()
        .setting_the_machine_up_to(MachineSetup::Vendor)
        .merging(as_one_commit())
        .expect_merge_refused("vendor_image_unconfigured")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .setting_the_machine_up_to(MachineSetup::Image)
        .merging(as_one_commit())
        .expect_merge_refused("docker_backend_unverified")
        .expect_no_merge_run()
        .expect_live_state_unchanged()
        .expect_stream_released()
        .setting_the_machine_up_to(MachineSetup::Complete)
        .merging(as_one_commit())
        .expect_conflicted(&["README.md"])
        .expect_state(GraduationRunState::Queued)
        .expect_queue_offered_the_merge_run();
}
