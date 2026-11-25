//! A stopped loop whose run a later command already moved on (GRD-FR-HRXA).
//!
//! A command that stops a turn rests the run at once, and the loop of that
//! turn returns later. In that time the author can Continue the run, or answer
//! it, and the store then holds a state the loop's own copy knows nothing of.
//! The loop then adds the commit of its stopped turn and writes no state.

use super::*;
use crate::project_scope::ProjectStores;

/// The stored run, where a command moved it on since the loop's copy was
/// taken, and `None` where the loop may record its own stop.
///
/// The rest a stopping command wrote (`interrupted`) is not moving on: the
/// loop completes that rest with what became of the turn's work. A terminal
/// run is the business of [`super::turns::discarded_meanwhile`].
pub(super) fn moved_on<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Option<GraduationRun> {
    let stored = match crate::graduation::load_run(app, &run.id) {
        Ok(stored) => stored,
        Err(reason) => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not read a stopped loop's run again",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            return None;
        }
    };
    let own_or_stop = stored.state == run.state
        || stored.state.is_terminal()
        || matches!(
            stored.state,
            GraduationRunState::Interrupted
                | GraduationRunState::Working
                | GraduationRunState::Reviewing
        );
    (!own_or_stop).then_some(stored)
}

/// GRD-FR-HRXA: add what the stopped turn committed to the stored record, and
/// write nothing else.
///
/// The record is read again under the store's hold, so a command that writes
/// at the same time is not lost.
pub(super) fn record_commit_only<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    committed: Option<bool>,
) {
    let state = app.try_project_store::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let mut fresh = match crate::graduation::load_run(app, &run.id) {
        Ok(fresh) => fresh,
        Err(reason) => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not record a stopped turn's commit",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            return;
        }
    };
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation recorded a stopped turn's commit and no state: a command moved the run on",
        log_fields! {
            "run_id" => run.id.clone(),
            "stored_state" => fresh.state.as_str().to_string(),
            "committed" => committed == Some(true),
        },
    );
    if committed != Some(true) {
        return;
    }
    for revision in &run.commits {
        if !fresh.commits.contains(revision) {
            fresh.commits.push(revision.clone());
        }
    }
    fresh.checkpoint.changed_paths = run.checkpoint.changed_paths.clone();
    if let Err(reason) = crate::graduation::save_run(app, &mut fresh) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not record a stopped turn's commit",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        );
    }
}

/// GRD-FR-PDNY: the blocker code of a run whose stopped turn left its
/// container running.
pub(super) const AGENT_CONTAINER_RUNNING: &str = "agent_container_running";

/// GRD-FR-PDNY: a turn whose container the executor could not confirm removed
/// rests the run `blocked`, over whatever state a later command wrote. Nothing
/// of the turn is committed: the container can still write in the working copy.
///
/// The record is read again under the store's hold, and the loop's copy is
/// replaced with what was written, so the loop goes on from the stored rest.
pub(super) fn rest_on_running_container<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
) {
    let resume_part = if run.state == GraduationRunState::Reviewing {
        phases::PART_REVIEW
    } else {
        phases::PART_WORK
    };
    logging::log_error(
        app,
        &crate::logging::BUFFER,
        &[Domain::Ai, Domain::Backend],
        "graduation rested a run: the stopped turn's container may still run",
        log_fields! { "run_id" => run.id.clone(), "resume_part" => resume_part },
    );
    let state = app.try_project_store::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let mut fresh = match crate::graduation::load_run(app, &run.id) {
        Ok(fresh) => fresh,
        Err(reason) => {
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "graduation could not rest a run on its running container",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            );
            return;
        }
    };
    if fresh.state.is_terminal() {
        return;
    }
    // GRD-FR-UTTX: the time the stopped turn ran, and the turns it counted,
    // are the loop's alone until now, so they go onto the stored record.
    fresh.turn_time_used_ms = fresh.turn_time_used_ms.max(run.turn_time_used_ms);
    fresh.work_turns = fresh.work_turns.max(run.work_turns);
    fresh.review_turns = fresh.review_turns.max(run.review_turns);
    super::turns::note_blocked(app, &mut fresh, AGENT_CONTAINER_RUNNING);
    let _ = transitions::block(
        app,
        &mut fresh,
        AGENT_CONTAINER_RUNNING,
        "The container of the stopped turn could not be removed, so it may still write in the working copy.",
        "Stop the agent container the application log names, then continue the run.",
        resume_part,
    );
    *run = fresh;
}
