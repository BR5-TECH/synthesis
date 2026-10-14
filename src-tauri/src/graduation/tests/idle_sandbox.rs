//! When a queue is idle, and when the module asks the executor to stop the
//! queue's sandbox VM (`GRD-graduation.md` GRD-FR-UESX,
//! `../tools/EAC-execute-agent-cli.md` EAC-FR-LQRI).

use super::*;
use crate::graduation::idle_sandbox::{idle_now, queue_is_idle};
use GraduationRunState as S;

/// A fixture whose loop does not start by itself.
fn quiet() -> Fixture {
    let fx = Fixture::new();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    fx
}

/// Put a stored run in a state, as the loop or the author leaves it there.
fn put(fx: &Fixture, run: &GraduationRun, state: GraduationRunState) -> GraduationRun {
    let mut stored = fx.reload(&run.id);
    stored.state = state;
    stored.dispatched = true;
    crate::graduation::save_run(&fx.app, &mut stored).expect("saved");
    stored
}

/// Move a stored run through the one funnel of state changes.
fn move_to(fx: &Fixture, run: &GraduationRun, next: GraduationRunState) -> GraduationRun {
    let mut stored = fx.reload(&run.id);
    crate::graduation::transition(&fx.app, &mut stored, next).expect("moved");
    stored
}

/// The queues whose VM stop was asked for, in order.
fn stops(fx: &Fixture) -> Vec<String> {
    fx.app.state::<GraduationState>().idle_stops()
}

fn idle(fx: &Fixture, key: &str) -> bool {
    idle_now(&fx.app, &fx.project_key(), key)
}

// GRD-FR-UESX: a queue with no run, or with only completed and discarded runs,
// is idle. A run in a state that holds its queue, a dispatched run queued
// again, and a run queued and never dispatched each keep it from idle.
#[test]
fn a_queue_is_idle_only_with_no_holder_and_no_queued_member() {
    let fx = quiet();
    let stream = fx.stream("editor");
    assert!(idle(&fx, &stream.id), "a queue with no run");

    let run = fx.enqueue_for(&stream, "one", "d1");
    assert!(!idle(&fx, &stream.id), "a queued run that was never dispatched");
    for state in
        [S::Working, S::Reviewing, S::Blocked, S::AwaitingAuthor, S::Interrupted, S::Failed]
    {
        put(&fx, &run, state);
        assert!(!idle(&fx, &stream.id), "{} holds the queue", state.as_str());
    }
    put(&fx, &run, S::Queued);
    assert!(!idle(&fx, &stream.id), "a dispatched run queued again holds the queue");

    put(&fx, &run, S::Completed);
    let other = fx.enqueue_for(&stream, "two", "d2");
    put(&fx, &other, S::Discarded);
    assert!(idle(&fx, &stream.id), "completed and discarded runs only");
}

// GRD-FR-UESX: a claim of this process keeps a queue from idle, and work in
// one queue keeps no other queue from idle.
#[test]
fn a_claim_keeps_a_queue_from_idle_and_queues_are_apart() {
    let fx = quiet();
    let editor = fx.stream("editor");
    let header = fx.stream("header");
    let direct = fx.direct("direct", "d-direct");
    put(&fx, &fx.enqueue_for(&header, "held", "d1"), S::AwaitingAuthor);
    put(&fx, &direct, S::Working);
    assert!(idle(&fx, &editor.id), "the work of other queues");
    assert!(!idle(&fx, &header.id));
    assert!(!idle(&fx, &direct.queue_key()));

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    assert!(queue_is_idle(&queue, &editor.id, false));
    assert!(!queue_is_idle(&queue, &editor.id, true), "a claim");
    fx.hold_stream(&editor.id);
    assert!(!idle(&fx, &editor.id), "a claim of this process");
}

// GRD-FR-UESX, EAC-FR-LQRI, EAC-FR-DWBS: a run the loop drives to `completed`
// leaves its queue idle, and the module asks once for a stop of the VM its
// turns ran in: the name from the project key and the scope of each dispatch.
#[test]
fn the_last_run_that_completes_asks_for_a_stop_of_its_vm() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, S::Completed);
    assert_eq!(stops(&fx), vec![stream.id.clone()]);

    let seen = dispatch.seen();
    assert!(!seen.is_empty());
    for turn in &seen {
        let scope = crate::tools::agent_exec::sandbox::SandboxScope {
            queue_key: turn.sandbox_scope.clone().expect("a graduation turn has a scope"),
        };
        let launched = crate::tools::agent_exec::sandbox::vm_name(&turn.project_key, &scope);
        assert_eq!(fx.app.state::<GraduationState>().idle_stop_vms(), vec![launched]);
    }
}

// EAC-FR-LQRI, GRD-FR-UESX: the check the stop runs again once it holds the VM
// reads the queue at that time. A queue that has work again by then keeps its
// VM, and a queue that is idle again lets the stop go on.
#[test]
fn the_stop_checks_the_queue_again_when_it_holds_the_vm() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    put(&fx, &run, S::Working);
    move_to(&fx, &run, S::Completed);
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
    let state = fx.app.state::<GraduationState>();
    let recheck = state.take_idle_recheck(0).expect("the check of the stop");
    let next = fx.enqueue_for(&stream, "next", "d2");
    assert!(!recheck(), "a run queued since the stop was asked for");

    // A second stop, asked for once that run ended, finds the queue idle again.
    put(&fx, &next, S::Working);
    move_to(&fx, &next, S::Completed);
    let recheck = state.take_idle_recheck(1).expect("the check of the second stop");
    assert!(recheck(), "the queue is idle again");
}

// GRD-FR-UESX, GRD-FR-EWTN: the discard of a run that a loop still drives
// releases that run's own claim, so the queue is idle and one stop is asked.
// The loop's later move is refused and asks for no second stop.
#[test]
fn the_discard_of_a_driven_run_asks_for_one_stop() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    let mut stale = put(&fx, &run, S::Working);
    assert!(fx.app.state::<GraduationState>().claim(
        &stream.id,
        &run.id,
        false,
        crate::project_settings::GraduationConcurrency::limited(1),
        crate::tools::agent_exec::runtime::CancellationToken::new(),
    ));
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("discarded");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
    crate::graduation::transition(&fx.app, &mut stale, S::Completed).expect("refused quietly");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
}

// GRD-FR-UESX: a run at rest holds its queue, so no stop is asked. That holds
// for `failed` too, and the discard of the failed run asks for one.
#[test]
fn a_run_at_rest_asks_for_no_stop_and_its_discard_does() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    put(&fx, &run, S::Working);
    for state in [S::Blocked, S::AwaitingAuthor, S::Interrupted, S::Queued, S::Failed] {
        move_to(&fx, &run, state);
        assert!(stops(&fx).is_empty(), "{} asks for no stop", state.as_str());
    }
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("discarded");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
}

// GRD-FR-UESX, GRD-FR-QCZF: the discard of a held run removes the runs queued
// behind it first, so the queue is idle and a stop is asked.
#[test]
fn a_discard_that_removes_the_runs_behind_asks_for_a_stop() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let held = fx.enqueue_for(&stream, "held", "d1");
    let behind = fx.enqueue_for(&stream, "behind", "d2");
    put(&fx, &held, S::Interrupted);

    crate::graduation::discard_graduation_run(fx.app.clone(), held.id.clone()).expect("discarded");
    assert!(crate::graduation::load_run(&fx.app, &behind.id).is_err(), "the run behind is gone");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
}

// GRD-FR-UESX: a run that completes with a member queued behind it keeps the
// VM. The discard of that member, which never held the queue, asks for the
// stop.
#[test]
fn a_queued_member_keeps_the_vm_until_it_leaves() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let first = fx.enqueue_for(&stream, "first", "d1");
    let second = fx.enqueue_for(&stream, "second", "d2");
    put(&fx, &first, S::Working);
    move_to(&fx, &first, S::Completed);
    assert!(stops(&fx).is_empty(), "the second run waits in the queue");

    crate::graduation::discard_graduation_run(fx.app.clone(), second.id.clone())
        .expect("discarded");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
}

// GRD-FR-UESX: a claim of this process on the queue keeps the VM when a run
// completes.
#[test]
fn a_claim_on_the_queue_keeps_the_vm() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    put(&fx, &run, S::Working);
    fx.hold_stream(&stream.id);
    move_to(&fx, &run, S::Completed);
    assert!(stops(&fx).is_empty());
}

// GRD-FR-UESX: each queue is asked for on its own. A run that completes in one
// stream asks for that stream alone, while a run at rest holds another.
#[test]
fn each_queue_is_asked_for_on_its_own() {
    let fx = quiet();
    let editor = fx.stream("editor");
    let header = fx.stream("header");
    let done = fx.enqueue_for(&editor, "done", "d1");
    let resting = fx.enqueue_for(&header, "resting", "d2");
    put(&fx, &resting, S::AwaitingAuthor);
    put(&fx, &done, S::Working);
    move_to(&fx, &done, S::Completed);
    assert_eq!(stops(&fx), vec![editor.id.clone()]);

    crate::graduation::discard_graduation_run(fx.app.clone(), resting.id.clone())
        .expect("discarded");
    assert_eq!(stops(&fx), vec![editor.id.clone(), header.id.clone()]);
}

// GRD-FR-UESX, EAC-FR-LQRI: a direct run on an ordinary worktree asks for the
// stop of its worktree queue, the scope its turns ran under (GXD-FR-SHNW).
#[test]
fn a_direct_run_asks_for_its_worktree_queue() {
    let fx = quiet();
    let run = fx.direct("direct", "d1");
    assert!(crate::graduation::is_worktree_queue_key(&run.queue_key()));
    put(&fx, &run, S::Working);
    move_to(&fx, &run, S::Completed);
    assert_eq!(stops(&fx), vec![run.queue_key()]);
}

// GRD-FR-UESX, GRD-FR-EWTN: a move that the store refuses, because the run
// already ended, asks for no second stop.
#[test]
fn a_refused_move_asks_for_no_second_stop() {
    let fx = quiet();
    let stream = fx.stream("editor");
    let run = fx.enqueue_for(&stream, "work", "d1");
    let mut stale = put(&fx, &run, S::Working);
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone()).expect("discarded");
    crate::graduation::transition(&fx.app, &mut stale, S::Completed).expect("refused quietly");
    assert_eq!(stops(&fx), vec![stream.id.clone()]);
}
