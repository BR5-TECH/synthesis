//! The project-wide scheduler (`GRD-graduation.md` GRD-FR-KKKN, GRD-FR-NYSH,
//! GRD-FR-IJKV, GRD-FR-KPET, GRD-FR-GRHC).
//!
//! The scenarios bind a starter that claims a queue and a slot exactly as the
//! loop does, and drives no turn. What a scenario observes is which run the
//! scheduler chose, and in what order.

use super::*;
use crate::project_settings::{GraduationConcurrency, ProjectConfig};
use crate::tools::agent_exec::runtime::CancellationToken;

/// A fixture whose loop does not start by itself, so a run saved here waits for
/// the scheduler call the scenario makes.
fn quiet_fixture() -> Fixture {
    let fx = Fixture::new();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    fx
}

fn set_limit(fx: &Fixture, limit: GraduationConcurrency) {
    crate::project_settings::save_project_config_to(
        &crate::fs::RootFs::for_root(fx.root()),
        ProjectConfig {
            graduation_concurrency_limit: Some(limit),
            ..Default::default()
        },
    )
    .expect("the limit is saved");
}

/// One scheduler pass. Every start claims through the state, so the slots it
/// takes are counted by the next pass. Answers the runs started, in order.
fn pass(fx: &Fixture) -> Vec<String> {
    let mut started = Vec::new();
    let app = fx.app.clone();
    crate::graduation::scheduler::dispatch_with(&app, |run_id, key| {
        let run = crate::graduation::load_run(&app, run_id).expect("the run");
        let limit = crate::graduation::scheduler::concurrency_limit(&app);
        let took = app.state::<GraduationState>().claim(
            key,
            run_id,
            run.is_direct(),
            limit,
            CancellationToken::new(),
        );
        if took {
            started.push(run_id.to_string());
        }
        took
    });
    started
}

/// What the run's end does: its queue and its slot are released and its record
/// leaves `queued`.
fn finish(fx: &Fixture, run: &GraduationRun) {
    fx.app.state::<GraduationState>().release(&run.queue_key());
    let mut stored = fx.reload(&run.id);
    stored.state = GraduationRunState::Completed;
    crate::graduation::save_run(&fx.app, &mut stored).expect("saved");
}

/// A run of the stream that stands in `working`, holding the stream.
fn work(fx: &Fixture, run: &GraduationRun) {
    let mut stored = fx.reload(&run.id);
    stored.state = GraduationRunState::Working;
    crate::graduation::save_run(&fx.app, &mut stored).expect("saved");
    assert!(fx.app.state::<GraduationState>().claim(
        &run.queue_key(),
        &run.id,
        run.is_direct(),
        GraduationConcurrency::Unlimited,
        CancellationToken::new(),
    ));
}

/// The same, for a run that rests `blocked`: it holds its queue and a slot
/// with no loop behind it (GRD-FR-BNTC).
fn block(fx: &Fixture, run: &GraduationRun) {
    let mut stored = fx.reload(&run.id);
    stored.state = GraduationRunState::Blocked;
    crate::graduation::save_run(&fx.app, &mut stored).expect("saved");
    assert!(fx.app.state::<GraduationState>().claim(
        &run.queue_key(),
        &run.id,
        run.is_direct(),
        GraduationConcurrency::Unlimited,
        CancellationToken::new(),
    ));
}

fn ids(runs: &[&GraduationRun]) -> Vec<String> {
    runs.iter().map(|run| run.id.clone()).collect()
}

// GRD-FR-NYSH, GRD-FR-VLFO: a free slot goes to the oldest eligible run of the
// whole project, whichever queue it waits in, and the run behind it in its own
// queue does not overtake the run of another queue.
#[test]
fn a_free_slot_goes_to_the_oldest_eligible_run_across_queues() {
    let fx = quiet_fixture();
    let (a, b, c) = (fx.stream("a"), fx.stream("b"), fx.stream("c"));
    let a1 = fx.enqueue_for(&a, "a1", "d1");
    let b1 = fx.enqueue_for(&b, "b1", "d2");
    let a2 = fx.enqueue_for(&a, "a2", "d3");
    let c1 = fx.enqueue_for(&c, "c1", "d4");

    assert_eq!(pass(&fx), vec![a1.id.clone()], "the limit of one holds");
    assert!(pass(&fx).is_empty(), "no slot is free while a1 works");

    finish(&fx, &a1);
    assert_eq!(pass(&fx), vec![b1.id.clone()], "b1 is older than a2 and c1");
    finish(&fx, &b1);
    assert_eq!(pass(&fx), vec![a2.id.clone()]);
    finish(&fx, &a2);
    assert_eq!(pass(&fx), vec![c1.id.clone()]);
}

// GRD-FR-NYSH, GRD-FR-TKUR: a run that is not eligible is skipped, and it blocks
// no run of another queue.
#[test]
fn an_ineligible_run_blocks_no_run_of_another_queue() {
    let fx = quiet_fixture();
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let a1 = fx.enqueue_for(&a, "a1", "d1");
    let b1 = fx.enqueue_for(&b, "b1", "d2");
    crate::graduation::set_graduation_auto_start(fx.app.clone(), a1.id.clone(), false)
        .expect("paused");

    assert_eq!(pass(&fx), vec![b1.id.clone()]);
}

// GRD-FR-NYSH, GRD-FR-XRDY: a direct run held for its branch is skipped, and the
// oldest eligible run behind it takes the slot.
#[test]
fn a_direct_run_held_for_its_branch_is_skipped() {
    let fx = quiet_fixture();
    fx.other_branch("elsewhere");
    let held = fx.direct("held", "d1");
    fx.check_out("elsewhere");
    let stream = fx.stream("a");
    let behind = fx.enqueue_for(&stream, "behind", "d2");

    assert_eq!(pass(&fx), vec![behind.id.clone()]);
    assert_eq!(fx.reload(&held.id).state, GraduationRunState::Queued);
}

// GRD-FR-KKKN: stream runs and direct runs share one count of slots.
#[test]
fn stream_runs_and_direct_runs_count_together() {
    let fx = quiet_fixture();
    let stream = fx.stream("a");
    let first = fx.enqueue_for(&stream, "stream run", "d1");
    let direct = fx.direct("direct run", "d2");

    assert_eq!(pass(&fx), vec![first.id.clone()], "the stream run takes the slot");
    assert!(pass(&fx).is_empty(), "the direct run waits for a slot");
    assert_eq!(crate::graduation::graduation_capacity_of(&fx.app).in_use, 1);

    set_limit(&fx, GraduationConcurrency::limited(2));
    assert_eq!(pass(&fx), vec![direct.id.clone()], "a second slot is taken by the direct run");
    assert_eq!(crate::graduation::graduation_capacity_of(&fx.app).in_use, 2);
}

// GRD-FR-BNTC, GRD-FR-KPET: under `unlimited` every queue works at once, and a
// queue still works one run at a time.
#[test]
fn unlimited_removes_the_cap_and_keeps_one_run_per_queue() {
    let fx = quiet_fixture();
    set_limit(&fx, GraduationConcurrency::Unlimited);
    let (a, b, c) = (fx.stream("a"), fx.stream("b"), fx.stream("c"));
    let a1 = fx.enqueue_for(&a, "a1", "d1");
    let _a2 = fx.enqueue_for(&a, "a2", "d2");
    let b1 = fx.enqueue_for(&b, "b1", "d3");
    let c1 = fx.enqueue_for(&c, "c1", "d4");

    assert_eq!(pass(&fx), vec![a1.id.clone(), b1.id.clone(), c1.id.clone()]);
    assert!(pass(&fx).is_empty(), "a2 waits for its own stream");
}

// GRD-FR-BNTC: the one run each queue works at a time holds under a wide limit,
// so two runs never write to one stream or one worktree together.
#[test]
fn a_wide_limit_never_starts_two_runs_of_one_queue() {
    let fx = quiet_fixture();
    set_limit(&fx, GraduationConcurrency::limited(8));
    let stream = fx.stream("a");
    let first = fx.enqueue_for(&stream, "first", "d1");
    let _second = fx.enqueue_for(&stream, "second", "d2");
    let d1 = fx.direct("one", "d3");
    let _d2 = fx.direct("two", "d4");

    assert_eq!(pass(&fx), vec![first.id.clone(), d1.id.clone()]);
}

// GRD-FR-IJKV: a limit lowered below the slots held stops no run and starts no
// run until the slots held are fewer than the limit.
#[test]
fn a_limit_lowered_below_usage_stops_nothing_and_starts_nothing() {
    let fx = quiet_fixture();
    set_limit(&fx, GraduationConcurrency::limited(3));
    let streams: Vec<_> = (0..4).map(|n| fx.stream(&format!("s{n}"))).collect();
    let runs: Vec<_> = streams
        .iter()
        .enumerate()
        .map(|(n, s)| fx.enqueue_for(s, "run", &format!("d{n}")))
        .collect();

    let first = pass(&fx);
    assert_eq!(first, ids(&runs[..3].iter().collect::<Vec<_>>()));

    set_limit(&fx, GraduationConcurrency::limited(1));
    let state = fx.app.state::<GraduationState>();
    assert!(pass(&fx).is_empty());
    assert_eq!(state.working_count(), 3, "no running run was stopped");

    finish(&fx, &runs[0]);
    assert!(pass(&fx).is_empty(), "two slots are still held");
    finish(&fx, &runs[1]);
    assert!(pass(&fx).is_empty(), "one slot is held and the limit is one");
    finish(&fx, &runs[2]);
    assert_eq!(pass(&fx), vec![runs[3].id.clone()]);
}

// GRD-FR-GRHC: the capacity names the limit, the slots held, and the queued
// runs that wait for a slot alone.
#[test]
fn the_capacity_lists_the_runs_that_wait_for_a_slot_alone() {
    let fx = quiet_fixture();
    let (a, b, c, d) = (fx.stream("a"), fx.stream("b"), fx.stream("c"), fx.stream("d"));
    let working = fx.enqueue_for(&a, "working", "d1");
    let behind_it = fx.enqueue_for(&a, "behind", "d2");
    let b1 = fx.enqueue_for(&b, "b1", "d3");
    let _b2 = fx.enqueue_for(&b, "b2", "d4");
    let paused = fx.enqueue_for(&c, "paused", "d5");
    let d1 = fx.enqueue_for(&d, "d1", "d6");
    crate::graduation::set_graduation_auto_start(fx.app.clone(), paused.id.clone(), false)
        .expect("paused");
    work(&fx, &working);

    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.limit, GraduationConcurrency::limited(1));
    assert_eq!(capacity.in_use, 1);
    assert_eq!(
        capacity.waiting_for_slot,
        vec![b1.id.clone(), d1.id.clone()],
        "the earliest eligible run of each free queue, and no run that waits for its own queue",
    );
    assert!(!capacity.waiting_for_slot.contains(&behind_it.id));
    assert!(!capacity.waiting_for_slot.contains(&paused.id));
}

// GRD-FR-GRHC, GRD-FR-KPET: no run waits for a slot while a slot is free, and
// none ever does under `unlimited`.
#[test]
fn no_run_waits_for_a_slot_while_one_is_free() {
    let fx = quiet_fixture();
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let working = fx.enqueue_for(&a, "working", "d1");
    let _waiting = fx.enqueue_for(&b, "waiting", "d2");
    work(&fx, &working);

    set_limit(&fx, GraduationConcurrency::limited(2));
    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert!(capacity.waiting_for_slot.is_empty());

    set_limit(&fx, GraduationConcurrency::Unlimited);
    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.limit, GraduationConcurrency::Unlimited);
    assert!(capacity.waiting_for_slot.is_empty());
}

// GRD-FR-GRHC: a direct run held for its branch is not eligible and is never
// listed as waiting for a slot.
#[test]
fn a_held_direct_run_does_not_wait_for_a_slot() {
    let fx = quiet_fixture();
    fx.other_branch("elsewhere");
    let held = fx.direct("held", "d1");
    fx.check_out("elsewhere");
    let stream = fx.stream("a");
    let working = fx.enqueue_for(&stream, "working", "d2");
    work(&fx, &working);
    assert_eq!(fx.reload(&held.id).state, GraduationRunState::Queued);

    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert!(capacity.waiting_for_slot.is_empty(), "{capacity:?}");
}

// PSS-FR-ZVSD, GRD-FR-IJKV: a saved limit that is higher than the stored one
// offers every queue a dispatch at once, and one that is not higher offers none
// beyond what the slots allow.
#[test]
fn raising_the_saved_limit_offers_the_queues_a_dispatch() {
    let fx = Fixture::new();
    fx.app
        .manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let tripwire = || {
        fx.app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take()
    };
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let _held = fx.enqueue_for(&a, "held", "d1");
    fx.hold_stream(&a.id);
    let waiting = fx.enqueue_for(&b, "waiting", "d2");
    tripwire();

    let save = |limit: GraduationConcurrency| {
        crate::project_settings::save_project_config(
            ProjectConfig {
                graduation_concurrency_limit: Some(limit),
                ..Default::default()
            },
            fx.app.state::<crate::project::ProjectState>(),
            fx.app.clone(),
        )
        .expect("saved");
    };
    save(GraduationConcurrency::limited(1));
    assert!(tripwire().is_empty(), "no slot is free at one");

    save(GraduationConcurrency::limited(2));
    let attempts = tripwire();
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert_eq!(attempts[0].0, waiting.id);
}

// GRD-FR-GRHC: a run claimed a moment ago still reads `queued` until its first
// turn is recorded, and is not listed as waiting for a slot.
#[test]
fn a_run_that_was_just_claimed_does_not_wait_for_a_slot() {
    let fx = quiet_fixture();
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let starting = fx.enqueue_for(&a, "starting", "d1");
    let waiting = fx.enqueue_for(&b, "waiting", "d2");
    assert_eq!(pass(&fx), vec![starting.id.clone()]);
    assert_eq!(fx.reload(&starting.id).state, GraduationRunState::Queued);

    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.waiting_for_slot, vec![waiting.id.clone()]);
}

// GRD-FR-KKKN: a run resting `blocked` holds its slot until it stops holding
// its stream.
#[test]
fn a_blocked_run_keeps_its_slot() {
    let fx = quiet_fixture();
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let blocked = fx.enqueue_for(&a, "blocked", "d1");
    let waiting = fx.enqueue_for(&b, "waiting", "d2");
    block(&fx, &blocked);

    assert!(pass(&fx).is_empty(), "the blocked run holds the only slot");
    let capacity = crate::graduation::graduation_capacity_of(&fx.app);
    assert_eq!(capacity.in_use, 1);
    assert_eq!(capacity.waiting_for_slot, vec![waiting.id.clone()]);
}

// GRD-FR-NYSH, GRD-FR-KKKN: discarding a blocked run frees its slot, and the
// oldest eligible run of another queue is offered it at once.
#[test]
fn discarding_a_blocked_run_offers_its_slot_to_another_queue() {
    let fx = Fixture::new();
    fx.app
        .manage(crate::graduation::driver::drive::SpawnTripwire::default());
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    let blocked = fx.enqueue_for(&a, "blocked", "d1");
    let waiting = fx.enqueue_for(&b, "waiting", "d2");
    block(&fx, &blocked);
    let tripwire = || {
        fx.app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take()
    };
    tripwire();

    crate::graduation::discard_graduation_run(fx.app.clone(), blocked.id.clone())
        .expect("discarded");
    let attempts = tripwire();
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert_eq!(attempts[0].0, waiting.id);
}

// GRD-FR-IJKV, PSS-FR-ZVSD: a saved limit that is lower than the stored one
// offers no dispatch, and stops no run.
#[test]
fn lowering_the_saved_limit_dispatches_nothing() {
    let fx = Fixture::new();
    fx.app
        .manage(crate::graduation::driver::drive::SpawnTripwire::default());
    set_limit(&fx, GraduationConcurrency::limited(3));
    let (a, b) = (fx.stream("a"), fx.stream("b"));
    fx.enqueue_for(&a, "held", "d1");
    fx.hold_stream(&a.id);
    fx.enqueue_for(&b, "waiting", "d2");
    fx.app
        .state::<crate::graduation::driver::drive::SpawnTripwire>()
        .take();

    crate::project_settings::save_project_config(
        ProjectConfig {
            graduation_concurrency_limit: Some(GraduationConcurrency::limited(1)),
            ..Default::default()
        },
        fx.app.state::<crate::project::ProjectState>(),
        fx.app.clone(),
    )
    .expect("saved");
    let attempts = fx
        .app
        .state::<crate::graduation::driver::drive::SpawnTripwire>()
        .take();
    assert!(attempts.is_empty(), "{attempts:?}");
    assert_eq!(fx.app.state::<GraduationState>().working_count(), 1);
}
