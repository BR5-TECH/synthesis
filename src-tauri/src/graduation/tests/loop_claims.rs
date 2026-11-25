//! One loop in a run and in a queue, from its claim until it returns
//! (GRD-FR-QWLP, GRD-FR-KSMB, GRD-FR-XVUD, GRD-FR-HRXA).

use super::*;
use crate::project_settings::GraduationConcurrency;
use crate::tools::agent_exec::runtime::CancellationToken;
use std::collections::BTreeSet;

fn claim_loop(state: &GraduationState, queue: &str, run: &str) -> Option<u64> {
    state.claim_loop(
        queue,
        run,
        false,
        GraduationConcurrency::limited(4),
        &BTreeSet::new(),
        CancellationToken::new(),
    )
}

fn claim_plain(state: &GraduationState, queue: &str, run: &str) -> bool {
    state.claim(queue, run, false, GraduationConcurrency::limited(4), CancellationToken::new())
}

// GRD-FR-QWLP: a loop whose claim a command released still stands in its run
// and its queue. No claim is taken there until the loop ends, and the end of
// an older loop touches neither the claim nor the entry of a later one.
#[test]
fn a_loop_holds_its_run_and_queue_until_it_ends_and_its_end_touches_no_later_loop() {
    let state = GraduationState::default();
    let first = claim_loop(&state, "w1", "g1").expect("the first loop claims");
    assert_ne!(first, state::NO_LOOP);

    // A pause or a discard releases the claim while the loop still stops.
    state.release_loop_claim("w1", "g1", first);
    assert!(!state.is_driving("g1"));
    assert!(state.is_looping("g1") && state.is_loop_in_queue("w1"));
    assert_eq!(claim_loop(&state, "w1", "g1"), None, "the same run");
    assert_eq!(claim_loop(&state, "w1", "g2"), None, "another run of the queue");
    assert!(!claim_plain(&state, "w1", "g2"), "a plain claim in the queue");
    assert!(!claim_plain(&state, "w2", "g1"), "the run on another queue");

    state.end_loop("g1", first);
    let second = claim_loop(&state, "w1", "g1").expect("the run claims once the loop ended");
    assert_ne!(second, first);
    assert_ne!(second, state::NO_LOOP);

    // The old loop's late release and end act on nothing of the new loop.
    state.release_loop_claim("w1", "g1", first);
    state.end_loop("g1", first);
    assert!(state.is_driving("g1"));
    assert!(state.is_looping("g1") && state.is_loop_in_queue("w1"));
    assert_eq!(state.holder_of("w1").as_deref(), Some("g1"));

    state.release_loop_claim("w1", "g1", second);
    state.end_loop("g1", second);
    assert!(!state.is_driving("g1") && !state.is_looping("g1") && !state.is_loop_in_queue("w1"));
}

// GRD-FR-QWLP: a claim that drives no loop is not released by a loop's
// release, and it registers no loop.
#[test]
fn a_plain_claim_is_not_released_by_a_loop_release() {
    let state = GraduationState::default();
    assert!(claim_plain(&state, "w1", "g1"));
    assert!(!state.is_looping("g1"));
    state.release_loop_claim("w1", "g1", 1);
    assert_eq!(state.holder_of("w1").as_deref(), Some("g1"));
}

// GRD-FR-QWLP: the claim and the loop's entry are taken in one step, so two
// claims that race for one run take one loop between them, also where they
// name two queues.
#[test]
fn two_claims_that_race_take_one_loop() {
    for queues in [("w1", "w1"), ("w1", "w2")] {
        for _ in 0..100 {
            let state = Arc::new(GraduationState::default());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let racers: Vec<_> = [queues.0, queues.1]
                .into_iter()
                .map(|queue| {
                    let (state, barrier) = (state.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        claim_loop(&state, queue, "g1")
                    })
                })
                .collect();
            let won: Vec<u64> =
                racers.into_iter().filter_map(|racer| racer.join().expect("joined")).collect();
            assert_eq!(won.len(), 1, "one claim wins on {queues:?}");
            state.end_loop("g1", won[0]);
            assert!(!state.is_looping("g1"), "one loop entry stood");
        }
    }
}

// GRD-FR-HRXA, GRL-FR-WCZE: the reason a command stopped the loop survives the
// release of the claim, for both cancellation paths.
#[test]
fn the_stop_reason_survives_the_release_of_the_claim() {
    let state = GraduationState::default();
    let id = claim_loop(&state, "w1", "g1").expect("claimed");
    assert!(state.cancel_run("g1", GraduationInterruptionReason::AuthorPause));
    state.release_loop_claim("w1", "g1", id);
    assert_eq!(state.cancellation_reason("g1"), Some(GraduationInterruptionReason::AuthorPause));
    assert!(state.loop_stopped("g1"));
    state.end_loop("g1", id);
    assert_eq!(state.cancellation_reason("g1"), None);
    assert!(!state.loop_stopped("g1"));

    let a = claim_loop(&state, "w1", "g1").expect("claimed");
    let b = claim_loop(&state, "w2", "g2").expect("claimed");
    state.cancel_all(GraduationInterruptionReason::ApplicationShutdown);
    state.release_loop_claim("w1", "g1", a);
    state.release_loop_claim("w2", "g2", b);
    for run in ["g1", "g2"] {
        assert_eq!(
            state.cancellation_reason(run),
            Some(GraduationInterruptionReason::ApplicationShutdown)
        );
    }
    assert!(!state.cancel_run("g3", GraduationInterruptionReason::AuthorPause), "no claim");
}

// GRD-FR-KSMB: agent work is entered from the queue or from agent work alone.
#[test]
fn agent_work_is_entered_from_the_queue_or_from_agent_work_alone() {
    use GraduationRunState::*;
    let all = [
        Queued,
        Working,
        Reviewing,
        AwaitingAuthor,
        Blocked,
        Interrupted,
        Completed,
        Discarded,
        Failed,
    ];
    for stored in all {
        for next in all {
            let expected = !next.is_agent_work() || matches!(stored, Queued | Working | Reviewing);
            assert_eq!(
                transitions::enters_agent_work_from(stored, next),
                expected,
                "{stored:?} -> {next:?}"
            );
        }
    }
}

// GRD-FR-KSMB: a save that puts a run at rest or ended into agent work is
// refused and writes nothing, and so is a move through `transition`.
#[test]
fn a_run_at_rest_is_not_saved_or_moved_into_agent_work() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    for rest in [
        GraduationRunState::Interrupted,
        GraduationRunState::Blocked,
        GraduationRunState::AwaitingAuthor,
        GraduationRunState::Failed,
        GraduationRunState::Completed,
        GraduationRunState::Discarded,
    ] {
        let mut run = fx.enqueue(&stream, "Add the empty state.");
        run.state = rest;
        crate::graduation::save_run(&fx.app, &mut run).expect("the rest is saved");
        let before = fx.reload(&run.id);

        let mut copy = before.clone();
        copy.state = GraduationRunState::Working;
        let refusal = crate::graduation::save_run(&fx.app, &mut copy).expect_err("refused");
        assert!(refusal.starts_with(ERR_RUN_STATE_NOT_PERMITTED), "{rest:?}: {refusal}");
        assert!(refusal.contains(rest.as_str()), "{rest:?}: {refusal}");
        let after = fx.reload(&run.id);
        assert_eq!(after.state, rest);
        assert_eq!(after.updated_at, before.updated_at, "{rest:?}: nothing is written");
        let listed = crate::graduation::project_queue(&fx.app).expect("the queue");
        let entry = listed.runs.iter().find(|r| r.id == run.id).expect("listed");
        assert_eq!(entry.state, rest, "{rest:?}: the index entry stands");
    }

    let mut paused = fx.enqueue(&stream, "Add the header.");
    transitions::transition(&fx.app, &mut paused, GraduationRunState::Queued).unwrap();
    paused.state = GraduationRunState::Interrupted;
    crate::graduation::save_run(&fx.app, &mut paused).unwrap();
    let mut copy = fx.reload(&paused.id);
    let refusal = transitions::transition(&fx.app, &mut copy, GraduationRunState::Working)
        .expect_err("refused");
    assert!(refusal.starts_with(ERR_RUN_STATE_NOT_PERMITTED), "{refusal}");
    assert_eq!(copy.state, GraduationRunState::Interrupted, "the copy did not move");
    assert_eq!(fx.reload(&paused.id).state, GraduationRunState::Interrupted);

    // An ended run is still left as it is, with no error.
    let mut ended = fx.enqueue(&stream, "Add the footer.");
    transitions::transition(&fx.app, &mut ended, GraduationRunState::Discarded).unwrap();
    let mut copy = fx.reload(&ended.id);
    // GRD-FR-EWTN: an ended run is refused silently, as every move of it is.
    transitions::transition(&fx.app, &mut copy, GraduationRunState::Working).expect("silent");
    assert_eq!(fx.reload(&ended.id).state, GraduationRunState::Discarded);
}

// GRD-FR-KSMB, GRL-FR-WCZE: a queued run moves into agent work, except under
// a loop that a command stopped.
#[test]
fn a_queued_run_is_not_moved_into_agent_work_by_a_stopped_loop() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state.");
    let state = fx.app.state::<GraduationState>();
    let queued = GraduationRunState::Queued;
    let working = GraduationRunState::Working;

    assert!(!transitions::agent_work_refused(&fx.app, queued, working, &run.id), "no loop");
    let id = claim_loop(&state, &run.queue_key(), &run.id).expect("claimed");
    assert!(!transitions::agent_work_refused(&fx.app, queued, working, &run.id), "a loop");
    state.cancel_run(&run.id, GraduationInterruptionReason::AuthorPause);
    state.release_loop_claim(&run.queue_key(), &run.id, id);
    assert!(transitions::agent_work_refused(&fx.app, queued, working, &run.id), "stopped");
    assert!(
        !transitions::agent_work_refused(&fx.app, queued, queued, &run.id),
        "a move that is not agent work"
    );
    state.end_loop(&run.id, id);
    assert!(!transitions::agent_work_refused(&fx.app, queued, working, &run.id), "ended");
}

// GRD-FR-XVUD, GRD-FR-QWLP: the sweep leaves a run a loop of this process
// still stands in, and only the first read of the queue after a launch sweeps.
#[test]
fn the_first_read_sweeps_and_leaves_a_run_a_loop_stands_in() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Add the empty state.");
    transitions::transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    let state = fx.app.state::<GraduationState>();
    let id = claim_loop(&state, &run.queue_key(), &run.id).expect("claimed");
    state.release_loop_claim(&run.queue_key(), &run.id, id);

    let read = || crate::graduation::list_graduation_queue(fx.app.clone(), Default::default());
    read().expect("the first read");
    assert_eq!(fx.reload(&run.id).state, GraduationRunState::Working, "a loop stands in it");

    state.end_loop(&run.id, id);
    read().expect("a later read");
    assert_eq!(fx.reload(&run.id).state, GraduationRunState::Working, "a later read sweeps none");

    state.relaunch_for_test();
    read().expect("the first read after a launch");
    let swept = fx.reload(&run.id);
    assert_eq!(swept.state, GraduationRunState::Interrupted);
    assert_eq!(
        swept.interruption.map(|i| i.reason),
        Some(GraduationInterruptionReason::ExecutionAbandoned)
    );
}

// GRD-FR-XVUD: the first read is answered once for each launch.
#[test]
fn the_first_read_is_answered_once_for_each_launch() {
    let state = GraduationState::default();
    assert!(state.take_first_read());
    assert!(!state.take_first_read());
    state.relaunch_for_test();
    assert!(state.take_first_read());
}

/// The whole stored record, for a check that nothing at all was written.
fn record_of(fx: &Fixture, run_id: &str) -> serde_json::Value {
    serde_json::to_value(fx.reload(run_id)).expect("a record")
}

// GRD-FR-KSMB, GRL-FR-WCZE: the save and the transition of a stopped loop's
// copy that still reads `working` are refused over a queued run, and the
// stored record and its index entry stand. Once the loop ended, they go through.
#[test]
fn a_stopped_loop_cannot_write_agent_work_over_a_queued_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state.");
    let state = fx.app.state::<GraduationState>();
    let id = claim_loop(&state, &run.queue_key(), &run.id).expect("claimed");
    state.cancel_run(&run.id, GraduationInterruptionReason::AuthorPause);
    state.release_loop_claim(&run.queue_key(), &run.id, id);
    let before = record_of(&fx, &run.id);

    let mut copy = fx.reload(&run.id);
    copy.state = GraduationRunState::Working;
    let refusal = crate::graduation::save_run(&fx.app, &mut copy).expect_err("refused");
    assert_eq!(refusal, format!("{ERR_RUN_STATE_NOT_PERMITTED}: queued"));
    let mut copy = fx.reload(&run.id);
    let refusal = transitions::transition(&fx.app, &mut copy, GraduationRunState::Working)
        .expect_err("refused");
    assert_eq!(refusal, format!("{ERR_RUN_STATE_NOT_PERMITTED}: queued"));
    assert_eq!(record_of(&fx, &run.id), before, "nothing is written");
    let listed = crate::graduation::project_queue(&fx.app).expect("the queue");
    let entry = listed.runs.iter().find(|r| r.id == run.id).expect("listed");
    assert_eq!(entry.state, GraduationRunState::Queued, "the index entry stands");

    state.end_loop(&run.id, id);
    let mut copy = fx.reload(&run.id);
    transitions::transition(&fx.app, &mut copy, GraduationRunState::Working).expect("moved");
    assert_eq!(fx.reload(&run.id).state, GraduationRunState::Working);
}
