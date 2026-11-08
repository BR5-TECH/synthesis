//! What Continue and an accepted answer do with a run's rest and its check
//! gate (`GRD-graduation.md` GRD-FR-OERK, `../ai/GRL-graduation-loop.md`
//! GRL-FR-SPRE, GRL-FR-LNIZ).
//!
//! Split out of `commands.rs`, which holds the operations themselves.

use super::*;

/// GRL-FR-SPRE / GRD-FR-OERK: clear the rest of a run that Continue moves on.
///
/// A rest is cleared by every Continue, so no rest outlives the stop it was
/// recorded for. Continue from `total_time_limit` resets the used total time
/// alone. Continue from `checks_not_passed` clears the check gate and asks the
/// next work turn for the check correction of the rest's cause, and resets
/// neither the used total time nor the pass budget. Either way the pass budget
/// and the rest of the checkpoint are carried through untouched, so no
/// completed pass is made again.
pub(super) fn clear_rest<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &mut GraduationRun) {
    let Some(rest) = run.rest.take() else {
        return;
    };
    if run.state != GraduationRunState::AwaitingAuthor {
        return;
    }
    if rest.is_checks_not_passed() {
        // GRD-FR-OERK / GRL-FR-LAVF: the turn Continue dispatches carries the
        // check correction of the rest's cause and starts a new gate episode.
        let cause = rest
            .cause
            .or_else(|| run.checkpoint.check_gate.as_ref().map(|gate| gate.cause))
            .unwrap_or(GraduationCheckCause::RecordMissing);
        run.checkpoint.check_gate = None;
        run.checkpoint.continue_check_correction = Some(cause);
        logging::log_info(
            app,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "graduation continued a run whose checks did not pass",
            log_fields! {
                "run_id" => run.id.clone(),
                "cause" => cause.as_str(),
                "used_ms" => run.turn_time_used_ms as i64,
                "pass" => run.pass() as i64,
            },
        );
        return;
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation reset a spent total time",
        log_fields! {
            "run_id" => run.id.clone(),
            "limit_ms" => rest.limit_ms.unwrap_or_default() as i64,
            "used_ms" => run.turn_time_used_ms as i64,
        },
    );
    run.turn_time_used_ms = 0;
}

/// GRL-FR-LNIZ: an accepted set of answers closes the check gate. The turn
/// that takes the answers starts a new gate episode (GRL-FR-LAVF).
pub(super) fn close_gate_for_answers<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) {
    let open = run.checkpoint.check_gate.take();
    let continued = run.checkpoint.continue_check_correction.take();
    if let Some(gate) = open {
        logging::log_debug(
            app,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "graduation closed a check gate for the author's answers",
            log_fields! {
                "run_id" => run.id.clone(),
                "cause" => gate.cause.as_str(),
                "step" => gate.step.as_str(),
                "continued" => continued.is_some(),
            },
        );
    }
}

/// GRD-FR-CYIB / GRL-FR-XBUE: Continue against a run whose pass budget is spent
/// resets **that budget alone**. The window's floor moves to the next pass and
/// the run stands at that pass, so the passes it already made are not made
/// again and the findings it holds reach the next work turn. Every other
/// checkpoint value — the change set, the instruction, the phase to resume at
/// — is carried through untouched.
pub(super) fn reset_spent_budget<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    settings: &driver::LoopSettings,
) {
    let next = run.pass().saturating_add(1);
    run.checkpoint.pass = next;
    run.checkpoint.pass_floor = next;
    // Written now rather than left at nothing, so a run that waits in its
    // stream's queue between Continue and its next dispatch still reads
    // back as a run with a bound. The next dispatch recomputes it from the
    // settings then held (GRL-FR-KWNP).
    run.checkpoint.pass_limit = settings.pass_limit_from(run, next);
    crate::logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Ai, crate::logging::Domain::Backend],
        "graduation reset a spent pass budget",
        crate::log_fields! {
            "run_id" => run.id.clone(),
            "pass" => next as i64,
            "pass_budget" => if run.is_merge() { MERGE_PASS_BUDGET as i64 } else { settings.pass_budget as i64 },
        },
    );
}
