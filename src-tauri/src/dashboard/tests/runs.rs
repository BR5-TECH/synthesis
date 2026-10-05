//! Recent agent runs (PST-FR-33).
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// Recent agent runs (PST-FR-33)
// ------------------------------------------------------------------

fn queued_run(id: &str, draft: &str, updated_at: &str) -> crate::graduation::GraduationRun {
    let mut run = crate::graduation::GraduationRun::new_for_test(id, draft, updated_at);
    run.state = crate::graduation::GraduationRunState::Queued;
    run.observability.current_stage =
        crate::graduation::observability::GraduationVisualStage::Queued;
    run.observability.stage_condition =
        crate::graduation::observability::GraduationStageCondition::Waiting;
    run
}

fn running_run(id: &str, draft: &str, updated_at: &str) -> crate::graduation::GraduationRun {
    let mut run = crate::graduation::GraduationRun::new_for_test(id, draft, updated_at);
    run.state = crate::graduation::GraduationRunState::Working;
    run.observability.current_stage =
        crate::graduation::observability::GraduationVisualStage::Working;
    run.observability.stage_condition =
        crate::graduation::observability::GraduationStageCondition::Active;
    run
}

#[test]
fn agent_runs_order_by_updated_at_most_recent_first_and_cap_at_five() {
    // PST-FR-33: six runs of which two are `queued` -> five items, the
    // queued ones among them where their instants place them rather than
    // bunched at either end.
    let runs = vec![
        running_run("r-a", "d1", "2026-05-15T08:00:00Z"),
        queued_run("r-b", "d2", "2026-05-15T12:00:00Z"),
        running_run("r-c", "d3", "2026-05-15T09:00:00Z"),
        running_run("r-d", "d4", "2026-05-15T11:00:00Z"),
        queued_run("r-e", "d5", "2026-05-15T10:00:00Z"),
        running_run("r-f", "d6", "2026-05-15T07:00:00Z"),
    ];
    let items = agent_runs_from(&runs, &HashMap::new());

    assert_eq!(items.len(), DASHBOARD_ITEM_CAP);
    assert_eq!(
        items.iter().map(|i| i.run_id.as_str()).collect::<Vec<_>>(),
        vec!["r-b", "r-d", "r-e", "r-c", "r-a"],
        "descending by updated_at, with the queued runs where their instants put them"
    );
    // Each row carries the run's persisted state, stage and condition —
    // nothing here derives any of them.
    assert_eq!(items[0].state, crate::graduation::GraduationRunState::Queued);
    assert_eq!(
        items[0].stage,
        crate::graduation::observability::GraduationVisualStage::Queued
    );
    assert_eq!(
        items[0].stage_condition,
        crate::graduation::observability::GraduationStageCondition::Waiting
    );
    assert_eq!(
        items[1].state,
        crate::graduation::GraduationRunState::Working
    );
}

#[test]
fn agent_runs_break_a_tie_on_the_run_id_ascending() {
    let runs = vec![
        running_run("r-b", "d1", "2026-05-15T08:00:00Z"),
        running_run("r-a", "d2", "2026-05-15T08:00:00Z"),
    ];
    let first = agent_runs_from(&runs, &HashMap::new());
    let second = agent_runs_from(&runs, &HashMap::new());
    assert_eq!(first, second, "the order is total, so it repeats");
    assert_eq!(
        first.iter().map(|i| i.run_id.as_str()).collect::<Vec<_>>(),
        vec!["r-a", "r-b"],
    );
}

// PST-FR-33: a position is a position in ONE stream's queue. Runs of two
// streams stand in one project order and wait in queues of their own, so
// counting over the whole order would name a place no run holds.
#[test]
fn a_queue_place_is_counted_within_the_runs_own_stream() {
    let mut a1 = queued_run("r-a1", "d1", "2026-05-15T08:00:00Z");
    a1.stream_id = "w-a".into();
    a1.stream_name = "editor work".into();
    let mut b1 = queued_run("r-b1", "d2", "2026-05-15T09:00:00Z");
    b1.stream_id = "w-b".into();
    b1.stream_name = "server work".into();
    let mut a2 = queued_run("r-a2", "d3", "2026-05-15T10:00:00Z");
    a2.stream_id = "w-a".into();
    a2.stream_name = "editor work".into();
    let items = agent_runs_from(&vec![a1, b1, a2], &HashMap::new());

    let item = |id: &str| items.iter().find(|i| i.run_id == id).unwrap();
    assert_eq!(item("r-a1").queue_position, Some(0));
    assert_eq!(
        item("r-b1").queue_position,
        Some(0),
        "the first run of its own stream, whatever stands ahead of it in the project order",
    );
    assert_eq!(item("r-a2").queue_position, Some(1));
    assert_eq!(item("r-b1").stream_name, "server work");
}

// PST-FR-33, GRD-FR-ZVNO: a direct run has no stream id, so its queue is its
// worktree's. Runs on two worktrees wait in queues of their own, and each row
// names the worktree the run pinned.
#[test]
fn a_direct_run_is_counted_within_its_own_worktrees_queue() {
    let direct = |id: &str, draft: &str, at: &str, worktree: &str| {
        let mut run = queued_run(id, draft, at);
        run.stream_id = String::new();
        run.stream_name = String::new();
        run.direct_target = Some(crate::graduation::DirectTarget {
            worktree_path: format!("/work/{worktree}"),
            worktree_name: worktree.into(),
            branch: format!("{worktree}-branch"),
        });
        run
    };
    let runs = vec![
        direct("r-a1", "d1", "2026-05-15T08:00:00Z", "alpha"),
        direct("r-b1", "d2", "2026-05-15T09:00:00Z", "beta"),
        direct("r-a2", "d3", "2026-05-15T10:00:00Z", "alpha"),
    ];
    let items = agent_runs_from(&runs, &HashMap::new());

    let item = |id: &str| items.iter().find(|i| i.run_id == id).unwrap();
    assert_eq!(item("r-a1").queue_position, Some(0));
    assert_eq!(
        item("r-b1").queue_position,
        Some(0),
        "the first run of its own worktree, whatever stands ahead of it in the project order",
    );
    assert_eq!(item("r-a2").queue_position, Some(1));
    assert_eq!(item("r-a1").stream_name, "alpha");
    assert_eq!(item("r-b1").stream_name, "beta");
}

#[test]
fn only_a_queued_run_carries_its_place_in_the_queue() {
    // PST-FR-33: `queue_position` is the run's index in its own stream's
    // queue, counted in the project's own run order rather than in the
    // sorted result, and it is null for a run that has left `queued`. A run
    // that is not waiting takes no place, so it is not counted either.
    let runs = vec![
        running_run("r-a", "d1", "2026-05-15T12:00:00Z"),
        queued_run("r-b", "d2", "2026-05-15T08:00:00Z"),
        queued_run("r-c", "d3", "2026-05-15T09:00:00Z"),
    ];
    let items = agent_runs_from(&runs, &HashMap::new());

    let position = |id: &str| {
        items.iter().find(|i| i.run_id == id).unwrap().queue_position
    };
    assert_eq!(position("r-a"), None, "a running run holds no queue place");
    assert_eq!(position("r-b"), Some(0), "its index in the queue, not in this list");
    assert_eq!(position("r-c"), Some(1));
    assert_eq!(
        items.iter().map(|i| i.run_id.as_str()).collect::<Vec<_>>(),
        vec!["r-a", "r-c", "r-b"],
        "precondition: the sorted order differs from the queue order, or the \
             assertion above could not tell the two apart"
    );
}

#[test]
fn a_run_whose_draft_no_longer_resolves_is_still_returned() {
    // PST-FR-33: it keeps its `draft_id` and carries no name.
    let runs = vec![running_run("r-a", "d-gone", "2026-05-15T12:00:00Z")];
    let items = agent_runs_from(&runs, &HashMap::new());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].draft_id, "d-gone");
    assert_eq!(items[0].draft_name, None);

    let mut names = HashMap::new();
    names.insert("d-gone".to_string(), "checkout-v2".to_string());
    assert_eq!(
        agent_runs_from(&runs, &names)[0].draft_name.as_deref(),
        Some("checkout-v2"),
        "and a draft that does resolve is named by its stored record"
    );
}
