//! What a run that stopped before it completed does to its queue and to the
//! project's slots, and what the discard of such a run removes
//! (`GRD-graduation.md` GRD-FR-BNTC, GRD-FR-EGWS, GRD-FR-KKKN, GRD-FR-BSWV,
//! GRD-FR-RKWQ, GRD-FR-QCZF, GRD-FR-EPWW).

use super::merge_handoff::*;
use super::*;
use crate::project_settings::{GraduationConcurrency, ProjectConfig};
use crate::streams::StreamMergePublication;

/// A fixture whose loop does not start by itself, so nothing is dispatched
/// behind a scenario's back.
fn quiet_fixture() -> Fixture {
    let fx = Fixture::new();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    fx
}

fn set_limit(fx: &Fixture, limit: GraduationConcurrency) {
    crate::project_settings::save_project_config_to(
        &crate::fs::RootFs::for_root(fx.root()),
        ProjectConfig { graduation_concurrency_limit: Some(limit), ..Default::default() },
    )
    .expect("the limit is saved");
}

/// Put a stored run in a state, as the loop or the author leaves it there.
fn rest(fx: &Fixture, run: &GraduationRun, state: GraduationRunState) -> GraduationRun {
    let mut stored = fx.reload(&run.id);
    stored.state = state;
    stored.dispatched = true;
    crate::graduation::save_run(&fx.app, &mut stored).expect("saved");
    stored
}

/// The runs one scheduler pass offers to start, in order. The starter claims
/// nothing, so the pass shows the offers and not their consequences.
fn offers(fx: &Fixture) -> Vec<String> {
    let mut offered = Vec::new();
    crate::graduation::scheduler::dispatch_with(&fx.app, |run_id, _| {
        offered.push(run_id.to_string());
        false
    });
    offered
}

// GRD-FR-BNTC, GRD-FR-EGWS: a queue that a run holds offers no other run, in
// every state that holds it, and a `failed` run holds it too.
#[test]
fn a_queue_that_a_run_holds_offers_no_other_run() {
    use GraduationRunState as S;
    for state in [S::Blocked, S::AwaitingAuthor, S::Interrupted, S::Failed] {
        let fx = quiet_fixture();
        let stream = fx.stream("editor");
        let held = fx.enqueue_for(&stream, "held", "d1");
        let behind = fx.enqueue_for(&stream, "behind", "d2");
        assert_eq!(offers(&fx), vec![held.id.clone()], "before it rests");
        rest(&fx, &held, state);
        assert!(offers(&fx).is_empty(), "{} holds the queue", state.as_str());
        assert_eq!(fx.reload(&behind.id).state, S::Queued);
    }
}

// GRD-FR-BNTC: the queue is held wherever the holding run stands in the order.
// A run ahead of it in the order is not offered while it rests, and Continue
// offers the holder alone.
#[test]
fn a_run_that_holds_the_queue_is_offered_ahead_of_an_earlier_member() {
    let fx = quiet_fixture();
    let stream = fx.stream("editor");
    let earlier = fx.enqueue_for(&stream, "earlier", "d1");
    let holder = fx.enqueue_for(&stream, "holder", "d2");
    rest(&fx, &holder, GraduationRunState::Interrupted);
    assert!(offers(&fx).is_empty(), "the earlier member waits for the holder");

    let continued = fx.continue_run(&fx.reload(&holder.id));
    assert_eq!(continued.state, GraduationRunState::Queued);
    assert!(continued.holds_queue());
    assert_eq!(offers(&fx), vec![holder.id.clone()], "the holder alone is offered");
    assert_eq!(fx.reload(&earlier.id).state, GraduationRunState::Queued);
}

// GRD-FR-KKKN, GRD-FR-GRHC: a run at rest holds one slot, a `failed` run holds
// none, and a held run is never listed as waiting for a slot.
#[test]
fn a_run_at_rest_holds_a_slot_and_a_failed_run_holds_none() {
    let fx = quiet_fixture();
    set_limit(&fx, GraduationConcurrency::limited(2));
    let (a, b, c) = (fx.stream("a"), fx.stream("b"), fx.stream("c"));
    let resting = fx.enqueue_for(&a, "resting", "d1");
    let failed = fx.enqueue_for(&b, "failed", "d2");
    let waiting = fx.enqueue_for(&c, "waiting", "d3");
    rest(&fx, &resting, GraduationRunState::AwaitingAuthor);
    rest(&fx, &failed, GraduationRunState::Failed);

    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.in_use, 1, "only the resting run holds a slot");
    assert!(capacity.waiting_for_slot.is_empty(), "a slot is free");

    set_limit(&fx, GraduationConcurrency::limited(1));
    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.in_use, 1);
    assert_eq!(
        capacity.waiting_for_slot,
        vec![waiting.id.clone()],
        "the run of a free queue waits for a slot alone; the held queues are not listed"
    );
}

// GRD-FR-KKKN, GRD-FR-IJKV, GRD-FR-CYIB: a run that holds its slot takes no new
// claim on the limit, so Continue dispatches it at once under a full limit,
// while a run that holds no slot starts nothing.
#[test]
fn a_held_run_is_offered_under_a_full_limit_and_another_run_is_not() {
    let fx = quiet_fixture();
    set_limit(&fx, GraduationConcurrency::limited(1));
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let held = fx.enqueue_for(&a, "held", "d1");
    let other = fx.enqueue_for(&b, "other", "d2");
    rest(&fx, &held, GraduationRunState::Interrupted);
    assert!(offers(&fx).is_empty(), "the only slot is held by a run at rest");

    let continued = fx.continue_run(&fx.reload(&held.id));
    assert_eq!(continued.state, GraduationRunState::Queued);
    assert_eq!(offers(&fx), vec![held.id.clone()]);
    assert_eq!(fx.reload(&other.id).state, GraduationRunState::Queued);
}

// GRD-FR-BSWV: the slots are read from the stored runs, so a relaunch, which
// leaves no claim in memory, frees none of them.
#[test]
fn a_relaunch_frees_no_slot() {
    let fx = quiet_fixture();
    let stream = fx.stream("editor");
    let held = fx.enqueue_for(&stream, "held", "d1");
    rest(&fx, &held, GraduationRunState::AwaitingAuthor);
    assert_eq!(fx.app.state::<GraduationState>().working_count(), 0, "no claim stands");
    assert_eq!(crate::graduation::graduation_capacity_of(&fx.app).in_use, 1);
}

// GRD-FR-QCZF, GRD-FR-EPWW: the discard of a held run removes every run queued
// behind it in its queue, whether its auto-start is off or on, whether it is a
// stream run or a direct run on the stream's working copy. A run of another
// queue and a run ahead stay, and each removed run's draft is unlocked.
#[test]
fn a_discard_removes_the_runs_queued_behind_in_its_own_queue_alone() {
    let fx = quiet_fixture();
    let stream = fx.stream("editor");
    let other = fx.stream("header");
    let ahead = fx.enqueue_for(&stream, "ahead", "d-ahead");
    let held = fx.enqueue_for(&stream, "held", "d-held");
    let off = fx.enqueue_for(&stream, "off", "d-off");
    let on = fx.enqueue_for(&stream, "on", "d-on");
    let direct =
        fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "direct", "d-direct");
    let elsewhere = fx.enqueue_for(&other, "elsewhere", "d-else");
    crate::graduation::set_graduation_auto_start(fx.app.clone(), off.id.clone(), false)
        .expect("auto-start off");
    rest(&fx, &ahead, GraduationRunState::Completed);
    rest(&fx, &held, GraduationRunState::AwaitingAuthor);

    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), held.id.clone())
        .expect("discarded");
    assert_eq!(discarded.state, GraduationRunState::Discarded);

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    let remaining: Vec<&str> = queue.runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(remaining, vec![ahead.id.as_str(), held.id.as_str(), elsewhere.id.as_str()]);
    for gone in [&off, &on, &direct] {
        assert!(crate::graduation::load_run(&fx.app, &gone.id).is_err(), "the record is deleted");
        assert!(
            crate::graduation::draft_graduation(&queue, &gone.input.draft_id).is_none(),
            "the draft carries no run-state tag"
        );
        assert!(crate::graduation::require_unlocked_draft(&fx.app, &gone.input.draft_id).is_ok());
    }
    assert_eq!(fx.reload(&elsewhere.id).state, GraduationRunState::Queued);
    assert_eq!(fx.reload(&ahead.id).state, GraduationRunState::Completed);
    let record = crate::streams::stream_of(&fx.app, &stream.id).expect("the stream");
    assert_eq!(record.busy_run_id, None, "the discard released the stream");
}

// GRD-FR-BNTC, GRD-FR-EGWS, GRD-FR-KKKN, GRD-FR-ZVNO: a direct run on an
// ordinary worktree holds the worktree's queue and one slot while it rests in
// `awaiting_author` or `interrupted`, and the run behind it is not dispatched.
#[test]
fn a_resting_direct_run_holds_the_queue_of_an_ordinary_worktree() {
    use GraduationRunState as S;
    for state in [S::AwaitingAuthor, S::Interrupted] {
        let fx = quiet_fixture();
        let held = fx.direct("held", "d1");
        let behind = fx.direct("behind", "d2");
        assert!(held.stream_id.is_empty(), "an ordinary worktree has no stream");
        assert!(crate::graduation::is_worktree_queue_key(&held.queue_key()));
        assert_eq!(held.queue_key(), behind.queue_key());
        assert_eq!(offers(&fx), vec![held.id.clone()], "before it rests");

        let held = rest(&fx, &held, state);
        assert!(held.holds_queue() && held.holds_slot(), "{} holds both", state.as_str());
        assert!(offers(&fx).is_empty(), "{} holds the queue", state.as_str());
        assert_eq!(fx.reload(&behind.id).state, S::Queued);
        assert_eq!(crate::graduation::graduation_capacity_of(&fx.app).in_use, 1);
    }
}

// GRD-FR-QCZF, GRD-FR-EPWW, GRD-FR-ZVNO: the discard of a resting direct run on
// an ordinary worktree removes the runs queued behind it, and the draft of each
// removed run is an active draft again. A run ahead of it stays.
#[test]
fn a_discard_removes_the_runs_behind_a_direct_run_on_an_ordinary_worktree() {
    let fx = quiet_fixture();
    let ahead = fx.direct("ahead", "d-ahead");
    let held = fx.direct("held", "d-held");
    let off = fx.direct("off", "d-off");
    let on = fx.direct("on", "d-on");
    crate::graduation::set_graduation_auto_start(fx.app.clone(), off.id.clone(), false)
        .expect("auto-start off");
    rest(&fx, &ahead, GraduationRunState::Completed);
    rest(&fx, &held, GraduationRunState::AwaitingAuthor);

    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), held.id.clone())
        .expect("discarded");
    assert_eq!(discarded.state, GraduationRunState::Discarded);

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    let remaining: Vec<&str> = queue.runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(remaining, vec![ahead.id.as_str(), held.id.as_str()]);
    for gone in [&off, &on] {
        assert!(crate::graduation::load_run(&fx.app, &gone.id).is_err(), "the record is deleted");
        assert!(
            crate::graduation::draft_graduation(&queue, &gone.input.draft_id).is_none(),
            "the draft carries no run-state tag"
        );
        assert!(crate::graduation::require_unlocked_draft(&fx.app, &gone.input.draft_id).is_ok());
    }
    assert_eq!(fx.reload(&ahead.id).state, GraduationRunState::Completed);
    assert_eq!(crate::graduation::graduation_capacity_of(&fx.app).in_use, 0);
}

// GRD-FR-QCZF: the discard of a run that never held its queue removes nothing
// else, and a run that is `queued` after a Continue does hold it.
#[test]
fn the_discard_of_a_run_that_never_held_its_queue_removes_nothing() {
    let fx = quiet_fixture();
    let stream = fx.stream("editor");
    let first = fx.enqueue_for(&stream, "first", "d1");
    let second = fx.enqueue_for(&stream, "second", "d2");
    crate::graduation::discard_graduation_run(fx.app.clone(), first.id.clone()).expect("discarded");
    assert_eq!(fx.reload(&second.id).state, GraduationRunState::Queued);

    let held = fx.enqueue_for(&stream, "held", "d3");
    let behind = fx.enqueue_for(&stream, "behind", "d4");
    rest(&fx, &held, GraduationRunState::Interrupted);
    let continued = fx.continue_run(&fx.reload(&held.id));
    assert_eq!(continued.state, GraduationRunState::Queued);
    crate::graduation::discard_graduation_run(fx.app.clone(), held.id.clone()).expect("discarded");
    assert!(
        crate::graduation::load_run(&fx.app, &behind.id).is_err(),
        "a continued run held the queue, so its discard removed the run behind it"
    );
    assert_eq!(
        fx.reload(&second.id).state,
        GraduationRunState::Queued,
        "the second run is ahead of the held one and stays"
    );
}

// GRD-FR-RKWQ, GRD-FR-QCZF: the discard of a `failed` run is accepted, ends it
// as `discarded` and removes the run behind it. A restart of the failed run is
// refused first.
#[test]
fn a_failed_run_holds_its_queue_until_it_is_discarded() {
    let fx = quiet_fixture();
    let stream = fx.stream("editor");
    let failed = fx.enqueue_for(&stream, "failed", "d1");
    let behind = fx.enqueue_for(&stream, "behind", "d2");
    rest(&fx, &failed, GraduationRunState::Failed);
    assert!(offers(&fx).is_empty(), "the run behind a failed run waits");

    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        failed.id.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect_err("a failed run is not restarted");
    assert!(refused.starts_with("run_state_not_permitted"), "{refused}");

    let discarded = crate::graduation::discard_graduation_run(fx.app.clone(), failed.id.clone())
        .expect("discarded");
    assert_eq!(discarded.state, GraduationRunState::Discarded);
    assert!(crate::graduation::load_run(&fx.app, &behind.id).is_err());
}

// GRD-FR-QCZF, GRD-FR-EPWW, GRD-FR-KZPT: a merge run queued behind a discarded
// run is removed with what it owns. The snapshot ref that kept its merge
// reachable goes, and its stream's row no longer names it.
#[test]
fn a_merge_run_queued_behind_a_discarded_run_is_removed_with_what_it_owns() {
    let fx = quiet_fixture();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (merge_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    assert!(snapshot_ref_stands(&fx, &merge_id));
    let held = fx.enqueue_for(&stream, "held", "d1");
    // The draft run is moved ahead of the merge run, and it rests.
    crate::graduation::reorder_graduation_run(fx.app.clone(), held.id.clone(), 1, 0)
        .expect("reordered");
    rest(&fx, &held, GraduationRunState::AwaitingAuthor);

    crate::graduation::discard_graduation_run(fx.app.clone(), held.id.clone()).expect("discarded");

    assert!(crate::graduation::load_run(&fx.app, &merge_id).is_err(), "the merge run is removed");
    assert!(!snapshot_ref_stands(&fx, &merge_id), "its snapshot ref is removed");
    assert!(
        index_entries(&fx).iter().all(|entry| entry.run_id != merge_id),
        "it left the run order"
    );
}
