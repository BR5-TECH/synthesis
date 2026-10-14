//! The total time a run may use (`GRL-graduation-loop.md` GRL-FR-EFTM,
//! GRL-FR-WZTE, GRL-FR-WOBK, GRL-FR-JNMI; `GRD-graduation.md` GRD-FR-UTTX,
//! GRD-FR-SKLO).
//!
//! Every turn adds its execution time to the run's used total time. A turn is
//! dispatched under the smaller of the stage time limit and what is left of
//! the total, and a run with nothing left rests for the author rather than
//! being interrupted: the author decides whether the run may use more.

use super::settings::{LoopSettings, TurnBound};
use super::stops;
use super::turns;
use super::*;
use crate::graduation::logs::{self, GraduationLogProducer, StructuredEvent};
use std::path::Path;
use std::time::Instant;

/// GRD-FR-UTTX: add the execution time of one turn to the run's used total
/// time.
///
/// The time is the longer of what this process measured and what the executor
/// reported. A turn stopped at its effective timeout adds at least that
/// timeout, so a run the total time stopped has no remaining time left.
///
/// Nothing is saved here: the run in hand may belong to a run the author
/// discarded meanwhile, and the code that settles the turn decides whether
/// the record is written.
pub(super) fn charge(
    run: &mut GraduationRun,
    started: Instant,
    outcome: &Result<AgentExecution, AgentExecutionError>,
    bound: TurnBound,
) {
    let measured = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut spent = measured;
    if let Ok(execution) = outcome {
        spent = spent.max(execution.duration_ms);
        if execution.process_outcome == ProcessOutcome::Timeout {
            spent = spent.max(bound.limit_ms);
        }
    }
    run.turn_time_used_ms = run.turn_time_used_ms.saturating_add(spent);
}

/// GRL-FR-WOBK: a run with no remaining total time dispatches no turn.
///
/// Answers whether the run came to rest.
pub(super) fn rest_if_spent<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    settings: &LoopSettings,
    part: &str,
) -> bool {
    if settings.remaining_total_ms(run.turn_time_used_ms) > 0 {
        return false;
    }
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation dispatched no turn: the run has no total time left",
        log_fields! {
            "run_id" => run.id.clone(),
            "part" => part.to_string(),
            "limit_ms" => settings.total_time_limit_ms as i64,
            "used_ms" => run.turn_time_used_ms as i64,
        },
    );
    // GRL-FR-JNMI: the turn that did not run is the turn Continue starts with.
    if part == phases::PART_REVIEW {
        run.checkpoint.resume_part = Some(phases::PART_REVIEW.to_string());
    }
    rest(app, run, settings.total_time_limit_ms);
    true
}

/// GRL-FR-WZTE / GRL-FR-JNMI: a turn the total time stopped.
///
/// What the turn wrote is committed as for any stopped turn (GRD-FR-SWOJ), and
/// a turn stopped in the review resumes at the review. The run then rests for
/// the author, holding its queue, its project slot and its checkpoint.
pub(super) fn rest_on_total_time<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    execution: &AgentExecution,
    bound: TurnBound,
) {
    if run.state == GraduationRunState::Reviewing {
        run.checkpoint.resume_part = Some(phases::PART_REVIEW.to_string());
    }
    let committed = turns::commit_stopped_turn(app, run, worktree);
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation stopped a turn at the run's total time limit",
        log_fields! {
            "run_id" => run.id.clone(),
            "duration_ms" => execution.duration_ms as i64,
            "limit_ms" => bound.total_limit_ms as i64,
            "used_ms" => run.turn_time_used_ms as i64,
            "committed" => stops::committed_sentence(committed),
        },
    );
    rest(app, run, bound.total_limit_ms);
}

/// GRD-FR-SKLO: record the rest and move the run to `awaiting_author`.
fn rest<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun, limit_ms: u64) {
    let event =
        StructuredEvent::new(GraduationLogProducer::StageTransition, "run rested for the author")
            .leaving(run.observability.current_stage)
            .with("reason", crate::graduation::REST_TOTAL_TIME_LIMIT)
            .with("limit_ms", limit_ms)
            .with("used_ms", run.turn_time_used_ms);
    // GRD-FR-IKVE: a record that could not be stored has already rested the
    // run on `log_persistence_failed`, which is the stop the author sees first.
    // The rest is set only after the record is stored, so a run that rests on
    // `log_persistence_failed` instead carries no rest that a later Continue
    // could misread.
    if !logs::emit(app, run, event) {
        return;
    }
    run.rest = Some(crate::graduation::GraduationRest {
        reason: crate::graduation::REST_TOTAL_TIME_LIMIT.to_string(),
        at: crate::notes::now_rfc3339(),
        limit_ms,
        used_ms: run.turn_time_used_ms,
    });
    let _ = transitions::transition(app, run, GraduationRunState::AwaitingAuthor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::agent_exec::runtime::CapturedStream;

    fn execution(outcome: ProcessOutcome, duration_ms: u64) -> AgentExecution {
        AgentExecution {
            process_outcome: outcome,
            exit_code: None,
            response: None,
            stdout: CapturedStream::default(),
            stderr: CapturedStream::default(),
            duration_ms,
            resume_unavailable: false,
            session: None,
            container_removed: true,
        }
    }

    const BOUND: TurnBound =
        TurnBound { limit_ms: 1_800_000, total_governs: true, total_limit_ms: 1_800_000 };

    // GRD-FR-UTTX: every turn adds its execution time, and the time is summed
    // across turns.
    #[test]
    fn each_turn_adds_its_execution_time_to_the_used_total() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-10-14T09:00:00Z");
        charge(&mut run, Instant::now(), &Ok(execution(ProcessOutcome::Completed, 600_000)), BOUND);
        charge(&mut run, Instant::now(), &Ok(execution(ProcessOutcome::Completed, 300_000)), BOUND);
        assert!(run.turn_time_used_ms >= 900_000);
        assert!(run.turn_time_used_ms < 900_000 + 60_000, "{}", run.turn_time_used_ms);
    }

    // GRD-FR-UTTX: a turn stopped at its effective timeout adds at least that
    // timeout, so no remaining time is left where the total ended it.
    #[test]
    fn a_turn_stopped_at_its_timeout_adds_at_least_that_timeout() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-10-14T09:00:00Z");
        charge(&mut run, Instant::now(), &Ok(execution(ProcessOutcome::Timeout, 0)), BOUND);
        assert!(run.turn_time_used_ms >= BOUND.limit_ms);
    }

    // GRD-FR-SKLO: a record with a rest reads back with it, and one written
    // before the fields existed reads as no rest and no time used.
    #[test]
    fn the_used_time_and_the_rest_round_trip_through_the_record() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-10-14T09:00:00Z");
        run.turn_time_used_ms = 42;
        run.rest = Some(crate::graduation::GraduationRest {
            reason: "total_time_limit".to_string(),
            at: "2026-10-14T09:00:00Z".to_string(),
            limit_ms: 1_800_000,
            used_ms: 42,
        });
        let value = serde_json::to_value(&run).unwrap();
        assert_eq!(value["turnTimeUsedMs"], 42);
        assert_eq!(value["rest"]["reason"], "total_time_limit");
        assert_eq!(value["rest"]["limitMs"], 1_800_000);
        let back: GraduationRun = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back.rest, run.rest);

        let mut old = value;
        let map = old.as_object_mut().unwrap();
        map.remove("turnTimeUsedMs");
        map.remove("rest");
        let back: GraduationRun = serde_json::from_value(old).unwrap();
        assert_eq!(back.turn_time_used_ms, 0);
        assert!(back.rest.is_none());
    }
}
