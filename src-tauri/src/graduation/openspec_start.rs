//! The checks of a propose start (`GSU-graduation-start.md` GSU-FR-MIVF,
//! GSU-FR-MTDA, GSU-FR-CZEP, GSU-FR-UIDE, GSU-FR-QSNE) and the `openspec`
//! data a propose run carries (`GRD-graduation.md` GRD-FR-LKGC).

use tauri::Manager;

use super::*;

/// GSU-FR-MTDA.
pub const ERR_OPENSPEC_MODE_INACTIVE: &str = "openspec_mode_inactive";
/// GSU-FR-MTDA.
pub const ERR_OPENSPEC_CLI_UNAVAILABLE: &str = "openspec_cli_unavailable";
/// GSU-FR-CZEP.
pub const ERR_INVALID_CHANGE_NAME: &str = "invalid_change_name";
/// GSU-FR-UIDE.
pub const ERR_OPENSPEC_CHANGE_EXISTS: &str = "openspec_change_exists";

/// GRD-FR-LKGC: the phase of every propose run.
pub const PHASE_PROPOSE: &str = "propose";

/// GSU-FR-MTDA, GSU-FR-CZEP, GSU-FR-UIDE: whether a propose run of `name`
/// may start on `stream`. A restart passes `check_exists = false`
/// (GSU-FR-QSNE).
pub fn preflight<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    name: &str,
    check_exists: bool,
) -> Result<(), String> {
    let result = check(app, stream, name, check_exists);
    if let Err(code) = &result {
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "propose start refused",
            crate::log_fields! { "code" => code.clone() },
        );
    }
    result
}

fn check<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    name: &str,
    check_exists: bool,
) -> Result<(), String> {
    require_mode_and_cli(app)?;
    if !crate::openspec::change::valid_change_name(name) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    if check_exists && change_exists(app, stream, name) {
        return Err(ERR_OPENSPEC_CHANGE_EXISTS.to_string());
    }
    Ok(())
}

/// GSU-FR-MTDA, OCR-FR-GTOM: OpenSpec mode is active on the active worktree,
/// and the CLI is usable, in that order.
pub(super) fn require_mode_and_cli<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    let active = app.try_state::<ProjectState>().and_then(|state| state.require_root().ok());
    let seam = app.try_state::<crate::openspec::OpenSpec>();
    let (Some(active), Some(seam)) = (active, seam) else {
        return Err(ERR_OPENSPEC_MODE_INACTIVE.to_string());
    };
    if !crate::openspec::OpenSpec::mode_active(Some(&active)) {
        return Err(ERR_OPENSPEC_MODE_INACTIVE.to_string());
    }
    if !seam.cli_state_in(app, &crate::logging::BUFFER, Some(&active)).is_available() {
        return Err(ERR_OPENSPEC_CLI_UNAVAILABLE.to_string());
    }
    Ok(())
}

/// GSU-FR-UIDE: the folder stands in the stream's working copy, or a
/// non-terminal OpenSpec run of any phase on that stream names the same
/// change (OCR-FR-VHQW).
fn change_exists<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    name: &str,
) -> bool {
    let folder = change_folder_stands(app, stream, name);
    let queued = queue::project_queue(app).is_some_and(|queue| {
        queue.runs.iter().any(|run| {
            run.stream_id == stream.id
                && !run.state.is_terminal()
                && run.openspec.as_ref().is_some_and(|data| data.change == name)
        })
    });
    folder || queued
}

/// OCR-FR-GTOM: whether `openspec/changes/<name>/` is a folder in the stream's
/// working copy.
pub(super) fn change_folder_stands<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    name: &str,
) -> bool {
    app.try_state::<crate::fs::FsAccessState>()
        .and_then(|state| state.get())
        .map(|access| crate::fs::RootFs::new(stream.worktree(), access))
        .and_then(|copy| copy.file_info(copy.path().join("openspec/changes").join(name)).ok())
        .is_some_and(|info| info.kind == crate::fs::EntryKind::Dir)
}

/// GRD-FR-LKGC: the data a propose run of `name` carries.
pub fn propose_data(name: &str) -> RunOpenSpec {
    RunOpenSpec::of_phase(PHASE_PROPOSE, name)
}
