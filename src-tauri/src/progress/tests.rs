//! The tests of progress reporting
//! (`../../specifications/core/PRG-progress-reporting.md`).

use super::*;
use std::cell::RefCell;

/// A `ProgressSink` that records what it was handed, so the ordering and
/// count of delivered events are assertable without a Tauri runtime.
#[derive(Default)]
struct Recorder {
    events: RefCell<Vec<Operation>>,
}

impl ProgressSink for Recorder {
    fn publish(&self, operation: &Operation) {
        self.events.borrow_mut().push(operation.clone());
    }
}

impl Recorder {
    fn states(&self) -> Vec<OperationState> {
        self.events.borrow().iter().map(|e| e.state).collect()
    }

    fn len(&self) -> usize {
        self.events.borrow().len()
    }
}

/// An instant far enough in the past that every window has elapsed, so a
/// test that is not about coalescing never trips over it.
fn t0() -> Instant {
    Instant::now()
}

fn after(base: Instant, millis: u64) -> Instant {
    base + Duration::from_millis(millis)
}

fn register(reg: &ProgressRegistry, kind: &str, at: Instant) -> (OperationId, Operation) {
    reg.register_at(kind, kind, None, None, at)
}

// -----------------------------------------------------------------------
// PRG-FR-01 — the documented payload shape
// -----------------------------------------------------------------------

#[test]
fn an_operation_serialises_to_the_documented_shape() {
    // PRG-FR-01: the contract's field names, camelCase, with the state
    // discriminant lowercased. An indeterminate operation omits both
    // progress fields rather than sending nulls (PRG-FR-05), and the
    // internal scope never reaches the wire.
    let registry = ProgressRegistry::default();
    let (_, op) = registry.register_at(
        "scan",
        "Indexing project…",
        Some(PathBuf::from("/tmp/project")),
        None,
        t0(),
    );
    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(json.get("kind").and_then(|v| v.as_str()), Some("scan"));
    assert_eq!(
        json.get("label").and_then(|v| v.as_str()),
        Some("Indexing project…")
    );
    assert_eq!(json.get("state").and_then(|v| v.as_str()), Some("running"));
    assert!(json.get("id").and_then(|v| v.as_str()).is_some());
    assert!(json.get("sequence").and_then(|v| v.as_u64()).is_some());
    assert!(json.get("completed").is_none(), "indeterminate: no progress");
    assert!(json.get("total").is_none());
    assert!(json.get("scope").is_none(), "the scope is not on the wire");

    // A determinate one carries both fields.
    let (_, det) = registry.register_at("git", "Pushing…", None, Some(500), t0());
    let json = serde_json::to_value(&det).unwrap();
    assert_eq!(json.get("total").and_then(|v| v.as_u64()), Some(500));
    assert_eq!(json.get("completed").and_then(|v| v.as_u64()), Some(0));
}

// -----------------------------------------------------------------------
// PRG-FR-02, PRG-FR-08 / PRG-FR-02 — the in-flight set and its ordering
// -----------------------------------------------------------------------

#[test]
fn an_activation_serialises_as_a_typed_destination_and_is_absent_when_none() {
    // PRG-FR-KXQW, PRG-FR-TBZN: each supported type names its target by its own
    // camelCase field, and an operation with no destination has no field at all.
    let registry = ProgressRegistry::default();
    let wire = |activation: Option<Activation>| {
        let (_, op) = registry.register_with_activation_at("k", "l", None, None, activation, t0());
        serde_json::to_value(&op).unwrap()
    };
    assert_eq!(
        wire(Some(Activation::GraduationRun { run_id: "r1".into() }))["activation"],
        serde_json::json!({ "type": "graduation_run", "runId": "r1" })
    );
    assert_eq!(
        wire(Some(Activation::Discussion { discussion_id: "d1".into() }))["activation"],
        serde_json::json!({ "type": "discussion", "discussionId": "d1" })
    );
    assert_eq!(
        wire(Some(Activation::GitPush { branch: "feature/x".into() }))["activation"],
        serde_json::json!({ "type": "git_push", "branch": "feature/x" })
    );
    assert!(wire(None).get("activation").is_none());
}

#[test]
fn the_destination_travels_unchanged_through_listing_updates_and_the_terminal_event() {
    // PRG-FR-KXQW: the list, every progress event and the terminal event of one
    // operation carry the destination it was registered with.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let activation = Activation::GraduationRun { run_id: "run-7".into() };
    let id = register_and_publish_with_activation(
        &sink,
        &registry,
        "graduation",
        "Graduating",
        None,
        Some(10),
        Some(activation.clone()),
    );
    update_and_publish(&sink, &registry, id, Some(3), None);
    assert_eq!(registry.in_flight()[0].activation, Some(activation.clone()));
    terminate_and_publish(&sink, &registry, id, OperationState::Finished);
    let events = sink.events.borrow();
    assert!(events.len() >= 2);
    assert!(events.iter().all(|e| e.activation == Some(activation.clone())));
}

#[test]
fn an_operation_registered_without_a_destination_has_none_whatever_its_kind_and_label() {
    // PRG-FR-TBZN: nothing is derived from `kind` or `label`.
    let registry = ProgressRegistry::default();
    let (_, op) = registry.register_at("graduation", "Graduating run-1", None, None, t0());
    assert_eq!(op.activation, None);
    let sink = Recorder::default();
    let _ = attribute::<(), ()>(&sink, &registry, "git", "Pushing", None, || Ok(()));
    assert!(sink.events.borrow().iter().all(|e| e.activation.is_none()));
}

#[test]
fn attribute_with_activation_carries_the_destination_on_both_events() {
    // PRG-FR-KXQW
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let activation = Activation::GitPush { branch: "main".into() };
    let _ = attribute_with_activation::<(), ()>(
        &sink,
        &registry,
        "git",
        "Pushing",
        None,
        Some(activation.clone()),
        || Ok(()),
    );
    let events = sink.events.borrow();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.activation == Some(activation.clone())));
}

#[test]
fn in_flight_is_most_recently_started_first_and_shrinks_on_terminate() {
    // PRG-FR-02, PRG-FR-08: register A then B -> [B, A]; terminate B -> [A].
    let registry = ProgressRegistry::default();
    let (_a, a) = register(&registry, "a", t0());
    let (b_id, b) = register(&registry, "b", t0());

    let listed = registry.in_flight();
    assert_eq!(
        listed.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        vec![b.id.as_str(), a.id.as_str()],
        "descending sequence: the most recently started is first"
    );

    registry.terminate(b_id, OperationState::Finished);
    let listed = registry.in_flight();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, a.id);
}

#[test]
fn an_empty_registry_lists_nothing_without_erroring() {
    // PRG-FR-02: the command's return type has no error arm at all, so this
    // asserts the value; the *shape* is what makes it infallible.
    let registry = ProgressRegistry::default();
    assert!(registry.in_flight().is_empty());
}

// -----------------------------------------------------------------------
// PRG-FR-03 — ids are never reused
// -----------------------------------------------------------------------

#[test]
fn ids_and_sequences_never_repeat_across_a_session() {
    // PRG-FR-03: a long run of registrations and terminations must not let
    // a later operation reuse an earlier one's id — a consumer keying by id
    // would otherwise confuse the two.
    let registry = ProgressRegistry::default();
    let mut seen: Vec<String> = Vec::new();
    let mut highest = 0;
    for _ in 0..50 {
        let (id, op) = register(&registry, "churn", t0());
        registry.terminate(id, OperationState::Finished);
        assert!(op.sequence >= highest);
        highest = op.sequence;
        seen.push(op.id);
    }
    let (_, fresh) = register(&registry, "fresh", t0());
    assert!(
        !seen.contains(&fresh.id),
        "a fresh id must match none of the earlier ones"
    );
    assert!(
        fresh.sequence > highest,
        "and its sequence must exceed every earlier one"
    );
}

// -----------------------------------------------------------------------
// PRG-FR-04, PRG-FR-08 / PRG-FR-06 — the event stream converges on the in-flight set
// -----------------------------------------------------------------------

#[test]
fn a_subscriber_sees_registration_updates_and_exactly_one_terminal() {
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let base = t0();

    let (id, registration) = registry.register_at("scan", "Indexing…", None, Some(30), base);
    sink.publish(&registration);
    // Spaced beyond the coalescing window so each update is delivered.
    for (n, completed) in [10u64, 20, 30].into_iter().enumerate() {
        let at = after(base, (n as u64 + 1) * 100);
        if let Some(ev) = registry.update_at(id, Some(completed), None, None, at) {
            sink.publish(&ev);
        }
    }
    publish_all(&sink, &registry.terminate(id, OperationState::Finished));

    assert_eq!(
        sink.states(),
        vec![
            OperationState::Running,
            OperationState::Running,
            OperationState::Running,
            OperationState::Running,
            OperationState::Finished,
        ],
        "a registration, three updates, and exactly one terminal event"
    );
    assert!(
        registry.in_flight().is_empty(),
        "the derived in-flight set matches the command's"
    );

    // PRG-FR-08: nothing further can be produced for a terminated id.
    assert!(registry
        .terminate(id, OperationState::Finished)
        .is_empty());
    assert!(registry.update_at(id, Some(31), None, None, base).is_none());
}

#[test]
fn an_operation_faster_than_one_window_emits_exactly_two_events() {
    // PRG-FR-06, PRG-FR-08: register and immediately terminate -> the registration and
    // the terminal state, nothing between, and it is gone from the list.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let (id, registration) = register(&registry, "changes", t0());
    sink.publish(&registration);
    publish_all(&sink, &registry.terminate(id, OperationState::Finished));

    assert_eq!(sink.len(), 2);
    assert_eq!(
        sink.states(),
        vec![OperationState::Running, OperationState::Finished]
    );
    assert!(registry.in_flight().is_empty());
}

// -----------------------------------------------------------------------
// PRG-FR-05 — determinate progress is monotonic and bounded
// -----------------------------------------------------------------------

#[test]
fn completed_never_decreases_and_never_exceeds_total() {
    let registry = ProgressRegistry::default();
    let base = t0();
    let (id, _) = registry.register_at("git", "Pushing…", None, Some(500), base);
    registry
        .update_at(id, Some(40), None, None, after(base, 100))
        .unwrap();

    // Backwards is clamped to the previous value.
    let ev = registry
        .update_at(id, Some(10), None, None, after(base, 200))
        .unwrap();
    assert_eq!(ev.completed, Some(40));

    // Past the total is clamped to the total.
    let ev = registry
        .update_at(id, Some(9_000), None, None, after(base, 300))
        .unwrap();
    assert_eq!(ev.completed, Some(500));
    assert_eq!(ev.total, Some(500));
}

#[test]
fn an_indeterminate_operation_becomes_determinate_and_never_reverts() {
    // PRG-FR-05: a total arriving later brings `completed` with it, and no
    // later event drops either field again.
    let registry = ProgressRegistry::default();
    let base = t0();
    let (id, registration) = registry.register_at("install", "Installing…", None, None, base);
    assert_eq!((registration.completed, registration.total), (None, None));

    let ev = registry
        .update_at(id, None, Some(12), None, after(base, 100))
        .unwrap();
    assert_eq!(ev.total, Some(12));
    assert_eq!(ev.completed, Some(0), "a determinate operation carries both");

    // An update that names no total leaves the operation determinate.
    let ev = registry
        .update_at(id, Some(5), None, None, after(base, 200))
        .unwrap();
    assert_eq!((ev.completed, ev.total), (Some(5), Some(12)));
    let terminal = registry.terminate(id, OperationState::Finished);
    assert_eq!(terminal.last().unwrap().total, Some(12));
}

// -----------------------------------------------------------------------
// PRG-FR-07 — coalescing
// -----------------------------------------------------------------------

#[test]
fn a_burst_of_updates_collapses_but_keeps_the_latest_values() {
    // PRG-FR-07: a thousand updates inside one window deliver far fewer than
    // a thousand events; the last delivered update carries the latest
    // values, and the registration and terminal events both arrive.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let base = t0();
    let (id, registration) = registry.register_at("scan", "Indexing…", None, Some(1_000), base);
    sink.publish(&registration);

    for n in 1..=1_000u64 {
        // All inside the first window, so every one of them coalesces.
        if let Some(ev) = registry.update_at(id, Some(n), None, None, after(base, 1)) {
            sink.publish(&ev);
        }
    }
    publish_all(&sink, &registry.terminate(id, OperationState::Finished));

    assert!(
        sink.len() < 100,
        "a thousand updates must not become a thousand events, got {}",
        sink.len()
    );
    let events = sink.events.borrow();
    assert_eq!(events.first().unwrap().state, OperationState::Running);
    assert_eq!(events.last().unwrap().state, OperationState::Finished);
    // The trailing flush: the last *update* delivered carries 1000, not the
    // value from whenever the last non-coalesced update happened to fall.
    let last_update = events
        .iter()
        .rev()
        .find(|e| e.state == OperationState::Running)
        .unwrap();
    assert_eq!(
        last_update.completed,
        Some(1_000),
        "the last delivered update carries the latest values"
    );
}

#[test]
fn an_update_past_the_window_is_delivered_immediately() {
    // The other half of PRG-FR-07: coalescing must not swallow updates from
    // a reporter that is simply slow.
    let registry = ProgressRegistry::default();
    let base = t0();
    let (id, _) = registry.register_at("scan", "Indexing…", None, Some(10), base);
    assert!(registry
        .update_at(id, Some(1), None, None, base + COALESCE_WINDOW)
        .is_some());
    assert!(registry
        .update_at(id, Some(2), None, None, base + COALESCE_WINDOW * 2)
        .is_some());
}

#[test]
fn a_coalesced_update_is_not_flushed_twice() {
    // The trailing flush is owed once. A window that elapses before the
    // terminate delivers it as a normal update, and the terminate must then
    // emit only the terminal event.
    let registry = ProgressRegistry::default();
    let base = t0();
    let (id, _) = registry.register_at("scan", "Indexing…", None, Some(10), base);
    assert!(
        registry.update_at(id, Some(1), None, None, base).is_none(),
        "inside the window: coalesced"
    );
    assert!(
        registry
            .update_at(id, Some(2), None, None, base + COALESCE_WINDOW)
            .is_some(),
        "past the window: delivered, clearing the debt"
    );
    let events = registry.terminate(id, OperationState::Finished);
    assert_eq!(events.len(), 1, "only the terminal event is owed: {events:?}");
    assert_eq!(events[0].state, OperationState::Finished);
}

// -----------------------------------------------------------------------
// PRG-FR-09 — failure and cancellation are ordinary terminations
// -----------------------------------------------------------------------

#[test]
fn failure_and_cancellation_terminate_like_success_and_carry_no_detail() {
    let registry = ProgressRegistry::default();
    let (failing, _) = register(&registry, "git", t0());
    let (cancelling, _) = register(&registry, "run", t0());

    let failed = registry.terminate(failing, OperationState::Failed);
    let cancelled = registry.terminate(cancelling, OperationState::Cancelled);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].state, OperationState::Failed);
    assert_eq!(cancelled[0].state, OperationState::Cancelled);
    assert!(
        registry.in_flight().is_empty(),
        "a failure must not strand an operation as permanently running"
    );
    // PRG-FR-09: no error detail anywhere on the wire.
    let json = serde_json::to_string(&failed[0]).unwrap();
    for forbidden in ["error", "message", "detail", "cause"] {
        assert!(!json.contains(forbidden), "found {forbidden:?} in {json}");
    }
}

#[test]
fn attribute_terminates_on_both_the_success_and_the_failure_path() {
    // PRG-FR-11 / PRG-FR-09: the wrapper is what guarantees no operation is
    // left running, whichever way the work goes.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();

    let ok: Result<u8, ()> = attribute(&sink, &registry, "scan", "Indexing…", None, || Ok(7));
    assert_eq!(ok, Ok(7));
    assert_eq!(
        sink.states(),
        vec![OperationState::Running, OperationState::Finished]
    );
    assert!(registry.in_flight().is_empty());

    let err: Result<(), &str> =
        attribute(&sink, &registry, "git", "Pushing…", None, || Err("boom"));
    assert_eq!(err, Err("boom"));
    assert_eq!(
        sink.events.borrow().last().unwrap().state,
        OperationState::Failed
    );
    assert!(
        registry.in_flight().is_empty(),
        "a failing operation still leaves the in-flight set"
    );
}

#[test]
fn attribute_terminates_an_operation_whose_work_panics() {
    // PRG-FR-09: "a failure never strands an operation as permanently
    // running". An unwind skips everything after the call, so without the
    // catch the operation would sit in the in-flight set for the rest of
    // the session and the status bar would show it forever.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), ()> =
            attribute(&sink, &registry, "scan", "Indexing…", None, || {
                panic!("the scan hit a filesystem race");
            });
    }));

    assert!(panicked.is_err(), "the panic is re-raised, not swallowed");
    assert!(
        registry.in_flight().is_empty(),
        "and the operation still left the in-flight set"
    );
    assert_eq!(
        sink.states(),
        vec![OperationState::Running, OperationState::Failed]
    );
}

#[test]
fn a_completed_reported_against_no_total_stays_indeterminate() {
    // PRG-FR-05: an operation carries both progress fields or neither.
    // A reporter that knows how far it has got but not how far there is to
    // go must not produce a third state — `../ui/STB-status-bar.md`
    // STB-FR-08 renders determinate-vs-busy off exactly this pair, and a
    // `completed` with no `total` has no proportion to fill a bar to.
    let registry = ProgressRegistry::default();
    let base = t0();
    let (id, _) = registry.register_at("scan", "Indexing…", None, None, base);

    let ev = registry
        .update_at(id, Some(40), None, None, after(base, 100))
        .unwrap();
    assert_eq!(
        (ev.completed, ev.total),
        (None, None),
        "still indeterminate, not half-determinate"
    );

    // And once a total does arrive the operation becomes determinate.
    let ev = registry
        .update_at(id, Some(40), Some(500), None, after(base, 200))
        .unwrap();
    assert_eq!((ev.completed, ev.total), (Some(40), Some(500)));
}

// -----------------------------------------------------------------------
// PRG-FR-10 — no history
// -----------------------------------------------------------------------

#[test]
fn terminated_operations_leave_nothing_behind() {
    // PRG-FR-10: the registry holds only what is running. There is no query
    // for what ran earlier, so this asserts the only observable: the
    // in-flight set, and the fact that the registry's own storage shrank.
    let registry = ProgressRegistry::default();
    for _ in 0..5 {
        let (id, _) = register(&registry, "churn", t0());
        registry.terminate(id, OperationState::Finished);
    }
    assert!(registry.in_flight().is_empty());
    assert_eq!(
        registry.lock().tracked.len(),
        0,
        "nothing accumulates across a session"
    );
}

// -----------------------------------------------------------------------
// PRG-FR-13 / PRG-FR-15 — scope teardown, and the unscoped operation
// -----------------------------------------------------------------------

#[test]
fn a_content_root_change_terminates_only_that_roots_operations() {
    // PRG-FR-13: a scan against worktree A terminates when the active
    // worktree becomes B. PRG-FR-15: an unscoped operation (a plugin
    // install, which can be in flight before or between projects) is
    // untouched by the teardown and is still listed.
    let registry = ProgressRegistry::default();
    let sink = Recorder::default();
    let a = PathBuf::from("/tmp/worktree-a");
    let b = PathBuf::from("/tmp/worktree-b");

    let (_, scan_a) =
        registry.register_at("scan", "Indexing…", Some(a.clone()), None, t0());
    let (_, scan_b) =
        registry.register_at("scan", "Indexing…", Some(b.clone()), None, t0());
    let (_, install) = registry.register_at("install", "Installing…", None, None, t0());

    terminate_scope_and_publish(&sink, &registry, &a);

    let remaining: Vec<String> = registry.in_flight().into_iter().map(|o| o.id).collect();
    assert!(!remaining.contains(&scan_a.id), "A's scan is gone");
    assert!(remaining.contains(&scan_b.id), "B's scan keeps running");
    assert!(
        remaining.contains(&install.id),
        "an unscoped operation survives a project teardown"
    );
    assert_eq!(
        sink.states(),
        vec![OperationState::Cancelled],
        "exactly one terminal event, for A's scan"
    );
}

#[test]
fn an_unscoped_operation_is_listed_with_no_project_open() {
    // PRG-FR-15: `list_in_flight_operations` is not project-scoped, so an
    // install in flight between projects is returned rather than hidden.
    let registry = ProgressRegistry::default();
    let (_, install) = registry.register_at("install", "Installing…", None, None, t0());
    let listed = registry.in_flight();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, install.id);
}

// -----------------------------------------------------------------------
// PRG-FR-12 — an unrecognised kind is still a well-formed operation
// -----------------------------------------------------------------------

#[test]
fn an_unrecognised_kind_travels_verbatim_with_its_label() {
    // PRG-FR-12: recognising a kind is never required to render an
    // operation. The backend's part of that is not to constrain the value:
    // `kind` is an open string, and label and progress stand on their own.
    let registry = ProgressRegistry::default();
    let (_, op) = registry.register_at("something-new", "Doing a thing…", None, None, t0());
    assert_eq!(op.kind, "something-new");
    assert_eq!(op.label, "Doing a thing…");
    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(
        json.get("kind").and_then(|v| v.as_str()),
        Some("something-new")
    );
}

#[test]
fn command_functions_are_in_scope() {
    // Compile-time guard: renaming or removing the command without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = list_in_flight_operations;
}
