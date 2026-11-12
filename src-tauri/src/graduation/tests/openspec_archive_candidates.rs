//! The changes a stream merge archives, and their order
//! (`OAR-openspec-archive.md` OAR-FR-KGDP, OAR-FR-SURB), with the OpenSpec
//! CLI faked.

use super::archive_support::*;
use super::*;
use crate::graduation::openspec_start::PHASE_PROPOSE;
use crate::graduation::{PHASE_APPLY, PHASE_REWORK};
use crate::streams::StreamMergeResult;

/// The change names a merge of the stream archived, in archive order.
fn archived_names(fx: &Fixture, stream: &crate::streams::WorkStream) -> Vec<String> {
    let result = merge(fx, stream, commit_publication()).expect("merged");
    let StreamMergeResult::Merged { archived, .. } = result else { panic!("{result:?}") };
    archived.into_iter().map(|a| a.change).collect()
}

// OAR-FR-KGDP, OAR-FR-LZLW: only a completed apply run of the stream names a
// change to archive: a completed propose run, a completed rework run, a failed
// and a discarded apply run, and a completed apply run of another stream name
// none, with each change folder present, so no CLI process starts.
#[test]
fn only_a_completed_apply_run_of_the_stream_names_a_change() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let names = ["proposed", "reworked", "failed", "discarded", "elsewhere"];
    let stream = stream_with_changes(&fx, "editor", &names);
    let other = fx.stream("billing");
    let at = "2026-11-10T10:00:00Z";
    ended_run(&fx, &stream, PHASE_PROPOSE, "proposed", at, GraduationRunState::Completed);
    ended_run(&fx, &stream, PHASE_REWORK, "reworked", at, GraduationRunState::Completed);
    ended_run(&fx, &stream, PHASE_APPLY, "failed", at, GraduationRunState::Failed);
    ended_run(&fx, &stream, PHASE_APPLY, "discarded", at, GraduationRunState::Discarded);
    ended_run(&fx, &other, PHASE_APPLY, "elsewhere", at, GraduationRunState::Completed);
    let before = runner.inner.calls().len();

    assert!(archived_names(&fx, &stream).is_empty());
    assert_eq!(runner.inner.calls().len(), before, "no CLI process");
    for name in names {
        assert!(stream.worktree().join("openspec/changes").join(name).is_dir(), "{name}");
    }
}

// OAR-FR-KGDP: a revert on another branch than the stream's and a landing on
// another branch than the base do not stop the archive of a change.
#[test]
fn a_revert_or_a_landing_on_another_branch_does_not_stop_the_archive() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["reverted-elsewhere", "landed-elsewhere"]);
    let elsewhere =
        |at: &str| Landing { branch: "other".into(), commit: "0".repeat(40), at: at.into() };
    let mut reverted = completed_apply(&fx, &stream, "reverted-elsewhere", "2026-11-10T09:00:00Z");
    reverted.reverts.push(elsewhere("2026-11-10T09:30:00Z"));
    save_run(&fx.app, &mut reverted).unwrap();
    let mut landed = completed_apply(&fx, &stream, "landed-elsewhere", "2026-11-10T10:00:00Z");
    landed.landings.push(elsewhere("2026-11-10T10:30:00Z"));
    save_run(&fx.app, &mut landed).unwrap();

    assert_eq!(archived_names(&fx, &stream), ["reverted-elsewhere", "landed-elsewhere"]);
}

// OAR-FR-SURB: the completion times are compared as instants, so a time with
// a fraction of a second sorts after the same second with none; two runs of
// one moment keep the order in which they were made.
#[test]
fn the_archive_order_compares_instants_and_keeps_the_run_order_on_a_tie() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["fraction", "whole", "zeta", "alpha"]);
    completed_apply(&fx, &stream, "fraction", "2026-11-10T09:00:00.500Z");
    completed_apply(&fx, &stream, "whole", "2026-11-10T09:00:00Z");
    completed_apply(&fx, &stream, "zeta", "2026-11-10T10:00:00Z");
    completed_apply(&fx, &stream, "alpha", "2026-11-10T10:00:00Z");

    assert_eq!(archived_names(&fx, &stream), ["whole", "fraction", "zeta", "alpha"]);
}

// OAR-FR-SURB: a completed run whose record holds no transition to `done`
// completed at its last write, so it sorts between a run that completed
// before that write and one whose time is after it.
#[test]
fn a_run_with_no_done_transition_completed_at_its_last_write() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["future", "unstaged", "past"]);
    completed_apply(&fx, &stream, "future", "2999-01-01T00:00:00Z");
    let mut unstaged = completed_apply(&fx, &stream, "unstaged", "2999-01-01T00:00:00Z");
    unstaged.observability.stage_history.clear();
    save_run(&fx.app, &mut unstaged).unwrap();
    completed_apply(&fx, &stream, "past", "2000-01-01T00:00:00Z");

    assert_eq!(archived_names(&fx, &stream), ["past", "unstaged", "future"]);
}
