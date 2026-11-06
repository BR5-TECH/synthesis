//! GRS-FR-EYNU: the damage a run log read met, kept on the run record.

use super::*;

/// Whether two read failures name the same damage.
///
/// Compared on what the damage **is** rather than on the whole record: a
/// failure carries the instant it was met, so two reads of one damaged file are
/// never equal and comparing them whole would rewrite the run record once per
/// scroll.
fn same_damage(held: &logs::GraduationLogFailure, met: &logs::GraduationLogFailure) -> bool {
    held.code == met.code
        && held.stream == met.stream
        && held.stopped_sequence == met.stopped_sequence
        && held.byte_offset == met.byte_offset
}

/// GRS-FR-EYNU: keep the damage a read met on the run record, and clear it once
/// a later read of that stream succeeds.
///
/// The run record is written only where the value actually changes, and under
/// the same write guard the run's own operations take, so a read never overtakes
/// the thread driving the run.
pub(super) fn record_read_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    stream: GraduationLogStream,
    met: Option<logs::GraduationLogFailure>,
) {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    // Read the record back inside the guard: the copy this command read before
    // the file may be behind what the run's own thread has since written.
    let Ok(mut run) = runs::load_run(app, run_id) else {
        return;
    };
    let held = run.logs.last_read_failure.clone();
    let settled = match (&held, &met) {
        (Some(held), Some(met)) if same_damage(held, met) => return,
        (None, None) => return,
        // A read of this stream that met nothing clears a failure of this
        // stream alone, and leaves one the other stream is still damaged by.
        (Some(held), None) if held.stream != stream => return,
        (_, met) => met.clone(),
    };
    run.logs.last_read_failure = settled;
    if let Err(reason) = runs::save_run(app, &mut run) {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not record what a run log read met",
            log_fields! { "run_id" => run_id.to_string(), "reason" => reason },
        );
    }
}
