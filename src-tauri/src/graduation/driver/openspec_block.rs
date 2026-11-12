//! How a CLI step of a run rests it on an OpenSpec failure
//! (`OFP-openspec-failure-policy.md` OFP-FR-TBHX, OFP-FR-UVPV, OFP-FR-NQPY,
//! OFP-FR-ITUD; `GRD-graduation.md` GRD-FR-UJII).
//!
//! The step dispatches no turn. The run rests `blocked` with its queue, its
//! stream, and its project slot held, and its blocker holds the code, the
//! cause, and the diagnostics. Continue clears a kept `failed` CLI check and
//! runs the step again.

use super::turns::note_blocked;
use super::*;
use crate::graduation::transitions::BlockerOpenSpec;
use crate::logging::BUFFER;
use crate::openspec::{OpenSpec, OpenSpecReadError};

/// GRD-FR-UJII, OFP-FR-XBNJ: the blocker code of an image probe that fails.
pub const BLOCKER_IMAGE_CLI_UNAVAILABLE: &str =
    crate::openspec::failure::CODE_IMAGE_CLI_UNAVAILABLE;

/// GRD-FR-UJII: the OpenSpec blocker codes of every run.
pub fn is_openspec_blocker(code: &str) -> bool {
    code == openspec_review::BLOCKER_VALIDATION_UNAVAILABLE || code == BLOCKER_IMAGE_CLI_UNAVAILABLE
}

/// OFP-FR-ITUD, OFP-FR-TBHX: Continue of a run that rests on an OpenSpec
/// failure clears a kept `failed` CLI check before the step runs again. A
/// run that rests on anything else clears nothing.
pub fn clear_failed_check_for<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    let openspec = run.blocker.as_ref().is_some_and(|blocker| is_openspec_blocker(&blocker.code));
    if !openspec {
        return;
    }
    if let Some(seam) = app.try_state::<OpenSpec>() {
        seam.clear_failed_checks(app, &BUFFER);
    }
}

/// GRL-FR-KDYL, GRL-FR-AGOA, OFP-FR-TBHX, OFP-FR-UVPV: a gate read that
/// failed rests the run `blocked` with `openspec_validation_unavailable`, the
/// read's code as its cause, and the read's diagnostics. The review is the
/// step Continue repeats.
pub(super) fn block_on_read<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    error: &OpenSpecReadError,
    message: String,
    clears_by: &str,
) {
    let code = openspec_review::BLOCKER_VALIDATION_UNAVAILABLE;
    note_blocked(app, run, code);
    let _ = crate::graduation::transitions::block_with(
        app,
        run,
        code,
        message,
        clears_by,
        phases::PART_REVIEW,
        BlockerOpenSpec {
            cause: Some(error.code().to_string()),
            detail: error.detail(),
            diagnostics: error.diagnostics().to_vec(),
        },
    );
}
