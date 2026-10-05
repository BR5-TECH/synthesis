//! A run that is executing, reported as an in-flight operation
//! (`GRD-graduation.md` GRD-FR-ZHNV, `PRG-progress-reporting.md` PRG-FR-KXQW).

use super::*;
use crate::progress::{Activation, Operation, ProgressRegistry};
use tauri::Listener;

type Seen = Arc<Mutex<Vec<Vec<Operation>>>>;

fn graduation_operations(fx: &Fixture) -> Vec<Operation> {
    fx.app
        .state::<ProgressRegistry>()
        .in_flight()
        .into_iter()
        .filter(|op| op.kind == PROGRESS_KIND_GRADUATION)
        .collect()
}

/// A turn side effect that records the graduation operations in flight at the
/// moment the turn runs.
fn watcher(fx: &Fixture, seen: &Seen) -> impl Fn() + Send + Sync + 'static {
    let app = fx.app.clone();
    let seen = seen.clone();
    move || {
        let now: Vec<Operation> = app
            .state::<ProgressRegistry>()
            .in_flight()
            .into_iter()
            .filter(|op| op.kind == PROGRESS_KIND_GRADUATION)
            .collect();
        seen.lock().unwrap().push(now);
    }
}

/// GRD-FR-ZHNV, PRG-FR-KXQW: while the work turn and the review turn run, the
/// run is one in-flight operation that names the run as its destination, and it
/// is the same operation in both turns. It is gone when the run completes.
#[test]
fn an_executing_run_is_one_operation_with_the_run_as_its_destination() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let seen: Seen = Arc::default();
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    {
        let events = events.clone();
        fx.app.listen(crate::progress::OPERATION_PROGRESS, move |event| {
            events.lock().unwrap().push(event.payload().to_string());
        });
    }

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().doing(watcher(&fx, &seen)),
        Turn::ready().doing(watcher(&fx, &seen)),
    ]);
    let run = fx.drive(&run, dispatch);
    assert_eq!(run.state, GraduationRunState::Completed);

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for during in seen.iter() {
        assert_eq!(during.len(), 1, "one operation while executing: {during:?}");
        assert_eq!(
            during[0].activation,
            Some(Activation::GraduationRun {
                run_id: run.id.clone()
            })
        );
        assert_eq!(during[0].label, "Graduating editor draft…");
        assert_eq!(during[0].scope, None);
    }
    assert_eq!(seen[0][0].id, seen[1][0].id, "working and reviewing share one");
    assert!(graduation_operations(&fx).is_empty(), "gone with the run");

    // Exactly one registration and one terminal event, and a completed run ends
    // as finished.
    let events = events.lock().unwrap();
    let mine: Vec<&String> = events
        .iter()
        .filter(|payload| payload.contains(&seen[0][0].id))
        .collect();
    assert_eq!(mine.len(), 2, "registration and terminal: {mine:?}");
    assert!(mine[0].contains("\"running\""));
    assert!(mine[1].contains("\"finished\""));
}

/// GRD-FR-ZHNV: a queued run is waiting and is not reported.
#[test]
fn a_queued_run_is_not_reported() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    assert_eq!(run.state, GraduationRunState::Queued);
    assert!(graduation_operations(&fx).is_empty());
}

/// GRD-FR-ZHNV: a run that rests for the author is not reported. It was
/// reported while its turns ran.
#[test]
fn a_run_awaiting_the_author_is_not_reported() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let seen: Seen = Arc::default();

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n").doing(watcher(&fx, &seen)),
        Turn::revise(ReviewSeverity::Major, "Still not there."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::revise(ReviewSeverity::Major, "Still not there."),
    ]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(seen.lock().unwrap()[0].len(), 1, "reported while it worked");
    assert!(graduation_operations(&fx).is_empty());
}

/// GRD-FR-ZHNV, PRG-FR-02: concurrent runs are separate operations, ordered
/// most-recently-started first, and ending one leaves the other.
#[test]
fn concurrent_runs_are_separate_operations_newest_first() {
    let fx = Fixture::new();
    let first_stream = fx.stream("one");
    let second_stream = fx.stream("two");
    let mut first = fx.enqueue(&first_stream, "First.");
    let mut second = fx.enqueue(&second_stream, "Second.");

    transition(&fx.app, &mut first, GraduationRunState::Working).unwrap();
    transition(&fx.app, &mut second, GraduationRunState::Working).unwrap();
    let both = graduation_operations(&fx);
    assert_eq!(both.len(), 2);
    assert_eq!(
        both[0].activation,
        Some(Activation::GraduationRun { run_id: second.id.clone() }),
        "the later run is first"
    );
    assert_eq!(
        both[1].activation,
        Some(Activation::GraduationRun { run_id: first.id.clone() })
    );

    // Moving on to reviewing registers no second operation.
    transition(&fx.app, &mut first, GraduationRunState::Reviewing).unwrap();
    assert_eq!(graduation_operations(&fx).len(), 2);

    // A blocked run waits on a condition, so it leaves; the other stays.
    transition(&fx.app, &mut second, GraduationRunState::Blocked).unwrap();
    let rest = graduation_operations(&fx);
    assert_eq!(rest.len(), 1);
    assert_eq!(
        rest[0].activation,
        Some(Activation::GraduationRun { run_id: first.id.clone() })
    );
}

/// GRD-FR-ZHNV: an interruption and a discard end the operation.
#[test]
fn an_interrupted_or_discarded_run_leaves_the_in_flight_set() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Add the empty state to the panel.");

    transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    assert_eq!(graduation_operations(&fx).len(), 1);
    transition(&fx.app, &mut run, GraduationRunState::Interrupted).unwrap();
    assert!(graduation_operations(&fx).is_empty());

    transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    assert_eq!(graduation_operations(&fx).len(), 1, "a continued run is reported again");
    transition(&fx.app, &mut run, GraduationRunState::Discarded).unwrap();
    assert!(graduation_operations(&fx).is_empty());
}

/// The states of the operation events of one run, in order.
fn end_states(events: &Arc<Mutex<Vec<String>>>, operation_id: &str) -> Vec<String> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|payload| payload.contains(&format!("\"{operation_id}\"")))
        .map(|payload| {
            let value: serde_json::Value = serde_json::from_str(payload).unwrap();
            value["state"].as_str().unwrap().to_string()
        })
        .collect()
}

fn listen_for_operations(fx: &Fixture) -> Arc<Mutex<Vec<String>>> {
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = events.clone();
    fx.app.listen(crate::progress::OPERATION_PROGRESS, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

/// GRD-FR-ZHNV: the operation ends as `finished` for `completed`, as `failed`
/// for `failed`, and as `cancelled` for every other state the run leaves
/// execution for.
#[test]
fn the_operation_ends_in_the_state_the_run_left_execution_for() {
    let fx = Fixture::new();
    let events = listen_for_operations(&fx);
    let stream = fx.stream("editor");
    let cases = [
        (GraduationRunState::Completed, "finished"),
        (GraduationRunState::Failed, "failed"),
        (GraduationRunState::Interrupted, "cancelled"),
        (GraduationRunState::Discarded, "cancelled"),
        (GraduationRunState::Blocked, "cancelled"),
        (GraduationRunState::AwaitingAuthor, "cancelled"),
        (GraduationRunState::Queued, "cancelled"),
    ];
    for (next, expected) in cases {
        let mut run = fx.enqueue(&stream, "Prompt.");
        transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
        let id = graduation_operations(&fx)
            .into_iter()
            .find(|op| op.activation == Some(Activation::GraduationRun { run_id: run.id.clone() }))
            .expect("the run is reported")
            .id;
        transition(&fx.app, &mut run, next).unwrap();
        assert_eq!(
            end_states(&events, &id),
            vec!["running".to_string(), expected.to_string()],
            "{next:?} ends the operation as {expected}"
        );
    }
}

/// GRD-FR-ZHNV: no operation of a run outlives its loop.
#[test]
fn an_operation_left_behind_is_ended_when_the_loop_returns() {
    let fx = Fixture::new();
    let events = listen_for_operations(&fx);
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Prompt.");
    transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    let id = graduation_operations(&fx)[0].id.clone();

    end_run_progress(&fx.app, &run.id);

    assert!(graduation_operations(&fx).is_empty());
    assert_eq!(
        end_states(&events, &id),
        vec!["running".to_string(), "cancelled".to_string()]
    );
    // A second end is a no-op: exactly one terminal event.
    end_run_progress(&fx.app, &run.id);
    assert_eq!(end_states(&events, &id).len(), 2);
}

/// GRD-FR-ZHNV: a change that was overtaken by a pause registers nothing. The
/// stored state says the run rests, so it is not reported as working.
#[test]
fn a_change_overtaken_by_a_pause_does_not_report_a_resting_run() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Prompt.");
    transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    // The loop's copy still says `working` while the pause has saved `interrupted`.
    let mut stale = run.clone();
    transition(&fx.app, &mut run, GraduationRunState::Interrupted).unwrap();
    assert!(graduation_operations(&fx).is_empty());

    stale.state = GraduationRunState::Working;
    crate::graduation::run_progress_sync_for_test(&fx.app, &stale);
    assert!(graduation_operations(&fx).is_empty(), "nothing is registered for a resting run");
}

/// GRD-FR-ZHNV: a merge run's label is its title, `Merge <stream>`, and names
/// no draft.
#[test]
fn a_merge_runs_label_names_its_title_and_no_draft() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Prompt.");
    run.merge = Some(GraduationMergeData {
        name: "Merge editor".into(),
        stream_branch: "synthesis/stream/editor".into(),
        base_branch: "main".into(),
        base_tip: "b".repeat(40),
        stream_tip: "c".repeat(40),
        merge_base: "a".repeat(40),
        snapshot_commit: "d".repeat(40),
        publication: crate::streams::StreamMergePublication::Uncommitted,
        changed_paths: Vec::new(),
        unresolved_paths: Vec::new(),
        conflicts: Vec::new(),
        result: None,
    });
    crate::graduation::save_run(&fx.app, &mut run).unwrap();
    transition(&fx.app, &mut run, GraduationRunState::Working).unwrap();
    let operations = graduation_operations(&fx);
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].label, "Merge editor…");
    assert!(!operations[0].label.contains("editor draft"));
}
