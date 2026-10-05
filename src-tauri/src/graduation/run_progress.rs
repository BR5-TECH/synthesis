//! A run that is executing, as an in-flight operation
//! (`specifications/core/GRD-graduation.md` GRD-FR-ZHNV).
//!
//! A run is executing while it is `working` or `reviewing`. A queued run waits,
//! and a run that is `blocked` or `awaiting_author` waits on the author, so none
//! of the three is work in flight. Every state change goes through
//! [`super::transition`], which calls [`sync`], so the operation starts and ends
//! at the same place the state changes.

use tauri::Manager;

use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};
use crate::progress::{self, Activation, OperationState, ProgressRegistry};

use super::{GraduationRun, GraduationRunState, GraduationState};

/// The producing module of a run's operation.
pub const PROGRESS_KIND_GRADUATION: &str = "graduation";

/// GRD-FR-ZHNV: the label names the run's title — the draft's name for a draft
/// run and `Merge <stream>` for a merge run.
pub(super) fn label_of(run: &GraduationRun) -> String {
    match run.merge.as_ref() {
        Some(merge) => format!("{}…", merge.name),
        None => format!("Graduating {}…", run.input.draft_name),
    }
}

/// GRD-FR-ZHNV: whether a state is one the run is executing in.
fn is_executing(state: GraduationRunState) -> bool {
    matches!(
        state,
        GraduationRunState::Working | GraduationRunState::Reviewing
    )
}

/// GRD-FR-ZHNV: how an operation ends for the state the run entered.
fn end_state_of(state: GraduationRunState) -> OperationState {
    match state {
        GraduationRunState::Completed => OperationState::Finished,
        GraduationRunState::Failed => OperationState::Failed,
        _ => OperationState::Cancelled,
    }
}

/// Make the run's operation match its state: registered while it executes and
/// ended in every other state. A run that enters `working` and then `reviewing`
/// keeps the one operation.
pub(super) fn sync<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    let (Some(state), Some(registry)) = (
        app.try_state::<GraduationState>(),
        app.try_state::<ProgressRegistry>(),
    ) else {
        return;
    };
    if !is_executing(run.state) {
        if let Some(operation) = state.take_operation(&run.id) {
            progress::terminate_and_publish(app, &registry, operation, end_state_of(run.state));
            logging::log_debug(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation run left the in-flight set",
                log_fields! { "run_id" => run.id.clone(), "state" => format!("{:?}", run.state) },
            );
        }
        return;
    }
    let registered = state.ensure_operation(&run.id, || {
        // A pause or a discard that saved a resting state after this change was
        // saved has already run its own sync. Registering now would report a
        // resting run as working, so the stored state decides.
        if !super::runs::load_run(app, &run.id).is_ok_and(|stored| is_executing(stored.state)) {
            return None;
        }
        Some(progress::register_and_publish_with_activation(
            app,
            &registry,
            PROGRESS_KIND_GRADUATION,
            &label_of(run),
            None,
            None,
            Some(Activation::GraduationRun {
                run_id: run.id.clone(),
            }),
        ))
    });
    if registered {
        logging::log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation run entered the in-flight set",
            log_fields! { "run_id" => run.id.clone() },
        );
    }
}

/// GRD-FR-ZHNV: the loop of a run has returned, so no operation of the run
/// outlives it.
pub fn end_with_loop<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run_id: &str) {
    let (Some(state), Some(registry)) = (
        app.try_state::<GraduationState>(),
        app.try_state::<ProgressRegistry>(),
    ) else {
        return;
    };
    if let Some(operation) = state.take_operation(run_id) {
        progress::terminate_and_publish(app, &registry, operation, OperationState::Cancelled);
    }
}
