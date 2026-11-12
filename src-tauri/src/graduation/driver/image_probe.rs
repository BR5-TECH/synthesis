//! The image probe of an OpenSpec run (`OFP-openspec-failure-policy.md`
//! OFP-FR-BZDP, OFP-FR-XBNJ, OFP-FR-RKMB, OFP-FR-WACS, OFP-FR-JBSO,
//! OFP-FR-LXYP, OFP-FR-NQPY).
//!
//! The agent of a propose, apply, or rework run calls the OpenSpec CLI
//! itself, so before the first work turn the application checks that the
//! run's image holds a usable CLI: `openspec --version` in one container of
//! that image, through the dispatch seam's `probe_agent_image`. The probe is
//! not a turn: it spends no pass, no turn time, and no stage time, starts no
//! agent CLI, mounts no credential, and emits no agent activity.

use std::time::Instant;

use super::*;
use crate::graduation::transitions::BlockerOpenSpec;
use crate::graduation::OpenSpecImageCli;
use crate::logging::BUFFER;
use crate::openspec::failure::{CODE_FAILED, CODE_MODE_INACTIVE, CODE_TIMED_OUT, RETRY_DELAY_MS};
use crate::openspec::OpenSpec;
use crate::tools::agent_exec::image_probe::{ImageProbeOutcome, ImageProbeRequest, ProbeCommand};
use crate::tools::agent_exec::runtime::RunEnd;

/// OFP-FR-XBNJ: the CLI states of an image that holds no usable CLI.
pub const STATE_NOT_FOUND: &str = "not_found";
pub const STATE_UNSUPPORTED: &str = "unsupported";
pub const STATE_FAILED: &str = "failed";

/// OFP-FR-RKMB: the code of the record of a skipped probe.
pub const CODE_PROBE_SKIPPED: &str = "image_probe_skipped";

/// The exit status a container runtime gives where the entrypoint program
/// does not exist in the image.
const EXIT_NOT_FOUND: i32 = 127;

/// What one probe found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// OFP-FR-XBNJ: exit 0 and a version of major number 1 or more.
    Usable(String),
    /// OFP-FR-XBNJ: the state of an image that holds no usable CLI.
    Unusable(&'static str),
    /// OFP-FR-XBNJ, OFP-FR-PQNS: an outcome that runs once more, with the
    /// code of that outcome for the record of the retry (OFP-FR-ZQNU):
    /// `cli_timed_out` for a timeout, and `cli_failed` for an end by a signal
    /// and for a container that does not start.
    Transient(&'static str),
    /// The run was cancelled during the probe.
    Cancelled,
    /// OFP-FR-XBNJ: an error of the runtime or of the image setting, with its
    /// kind. It is not a probe outcome: the probe records nothing, and the
    /// dispatch states the error on its own terms.
    NotAnOutcome(&'static str),
    /// OFP-FR-RKMB: the isolation of the run cannot run the probe.
    Skipped,
}

/// OFP-FR-XBNJ, OFP-FR-RKMB, OFP-FR-PQNS: what the outcome of one probe
/// means. A timeout, an end by a signal, and a container that does not start
/// are transient. An exit status 127 is `not_found`, a version below 1 is
/// `unsupported`, and every other outcome of the CLI is `failed`.
pub fn verdict_of(outcome: &Result<ImageProbeOutcome, AgentExecutionError>) -> ProbeVerdict {
    let probe = match outcome {
        Ok(ImageProbeOutcome::Ran(probe)) => probe,
        Ok(ImageProbeOutcome::Skipped) => return ProbeVerdict::Skipped,
        Err(AgentExecutionError::LaunchFailed(_)) => return ProbeVerdict::Transient(CODE_FAILED),
        Err(error) => return ProbeVerdict::NotAnOutcome(error.kind()),
    };
    match (probe.end, probe.exit_status) {
        (RunEnd::Cancelled, _) => ProbeVerdict::Cancelled,
        (RunEnd::TimedOut, _) => ProbeVerdict::Transient(CODE_TIMED_OUT),
        (RunEnd::Terminated, _) => ProbeVerdict::Transient(CODE_FAILED),
        // An exit with no status is an end by a signal.
        (RunEnd::Exited, None) | (RunEnd::EndedAfterFinalEvent, None) => {
            ProbeVerdict::Transient(CODE_FAILED)
        }
        (_, Some(0)) => {
            match crate::openspec::cli::parse_version(&String::from_utf8_lossy(&probe.stdout)) {
                Some((version, major)) if major >= crate::openspec::cli::MIN_MAJOR => {
                    ProbeVerdict::Usable(version)
                }
                Some(_) => ProbeVerdict::Unusable(STATE_UNSUPPORTED),
                None => ProbeVerdict::Unusable(STATE_FAILED),
            }
        }
        (_, Some(EXIT_NOT_FOUND)) => ProbeVerdict::Unusable(STATE_NOT_FOUND),
        (_, Some(_)) => ProbeVerdict::Unusable(STATE_FAILED),
    }
}

/// OFP-FR-BZDP: whether `run` is a propose, an apply, or a rework run.
fn probes(run: &GraduationRun) -> bool {
    run.openspec.is_some() && !run.is_merge() && !run.is_pr_summary()
}

/// OFP-FR-BZDP, OFP-FR-WACS, OFP-FR-NQPY: the probe before the first work
/// turn. It answers whether the run goes on to its turns; where it does not,
/// the run rests `blocked` with `openspec_image_cli_unavailable`.
pub(super) async fn probe_before_dispatch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> bool {
    if !probes(run) || run.checkpoint.openspec_image_cli.is_some() {
        return true;
    }
    let resume_part =
        run.checkpoint.resume_part.clone().unwrap_or_else(|| phases::PART_WORK.to_string());
    // OFP-FR-NQPY: a mode that went off rests the run; the probe is not
    // skipped.
    let active =
        app.try_state::<crate::project::ProjectState>().and_then(|s| s.require_root().ok());
    if !OpenSpec::mode_active(active.as_ref()) {
        rest(app, run, &resume_part, None, Some(CODE_MODE_INACTIVE));
        return false;
    }
    let started = Instant::now();
    let mut verdict = probe_once(app, run, cancellation, dispatch).await;
    if let ProbeVerdict::Transient(first_code) = verdict {
        pause(app, first_code).await;
        verdict = probe_once(app, run, cancellation, dispatch).await;
    }
    let millis = started.elapsed().as_millis() as u64;
    match verdict {
        ProbeVerdict::Usable(version) => {
            logging::log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec image probe passed",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "version" => version.clone(),
                    "durationMs" => millis,
                },
            );
            run.checkpoint.openspec_image_cli = Some(OpenSpecImageCli { version });
            let _ = crate::graduation::save_run(app, run);
            true
        }
        // The loop finds the cancellation and stops the run.
        ProbeVerdict::Cancelled => true,
        // OFP-FR-XBNJ: the dispatch goes on and states the error itself.
        ProbeVerdict::NotAnOutcome(kind) => {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec image probe not run",
                log_fields! { "run_id" => run.id.clone(), "code" => kind },
            );
            true
        }
        // OFP-FR-RKMB: the dispatch goes on, and the run records nothing.
        ProbeVerdict::Skipped => {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec image probe skipped",
                log_fields! { "run_id" => run.id.clone(), "code" => CODE_PROBE_SKIPPED },
            );
            true
        }
        ProbeVerdict::Unusable(state) => {
            rest(app, run, &resume_part, Some(state), None);
            false
        }
        ProbeVerdict::Transient(_) => {
            rest(app, run, &resume_part, Some(STATE_FAILED), None);
            false
        }
    }
}

/// One call of the probe seam.
async fn probe_once<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    cancellation: &CancellationToken,
    dispatch: &dyn seams::GraduationDispatch<R>,
) -> ProbeVerdict {
    let request = ImageProbeRequest {
        command: ProbeCommand::OpenSpecVersion,
        cancellation: cancellation.clone(),
    };
    verdict_of(&dispatch.probe_graduation_image(app, &run.project_key, request).await)
}

/// OFP-FR-PQNS, OFP-FR-MJZU, OFP-FR-ZQNU: the record and the wait of the one
/// more try, through the wait seam of the OpenSpec seam, off the async
/// thread. The record names `first_code`, the code of the first outcome.
async fn pause<R: tauri::Runtime>(app: &tauri::AppHandle<R>, first_code: &'static str) {
    let owned = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || match owned.try_state::<OpenSpec>() {
        Some(seam) => seam.pause_before_retry(&owned, &BUFFER, "image_probe", first_code),
        None => std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)),
    })
    .await;
}

/// OFP-FR-XBNJ, OFP-FR-TBHX, OFP-FR-LXYP: the run rests `blocked` with
/// `openspec_image_cli_unavailable`, the CLI state as its detail, or the
/// cause `openspec_mode_inactive`. One `WARN` record names the run, the code,
/// and the state, and no probe output.
fn rest<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    resume_part: &str,
    state: Option<&'static str>,
    cause: Option<&str>,
) {
    let code = super::openspec_block::BLOCKER_IMAGE_CLI_UNAVAILABLE;
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "openspec image probe failed",
        log_fields! {
            "run_id" => run.id.clone(),
            "code" => cause.unwrap_or(code).to_string(),
            "state" => state.unwrap_or_default(),
        },
    );
    super::turns::note_blocked(app, run, code);
    let (message, clears_by) = match state {
        Some(state) => (
            format!("The agent image of this project holds no usable OpenSpec CLI ({state})."),
            "Make the OpenSpec CLI 1.0 or later available in the agent image, then continue the run.",
        ),
        None => (
            "OpenSpec mode is off for this project.".to_string(),
            "Turn OpenSpec mode on for this project, then continue the run.",
        ),
    };
    let _ = crate::graduation::transitions::block_with(
        app,
        run,
        code,
        message,
        clears_by,
        resume_part,
        BlockerOpenSpec {
            cause: cause.map(str::to_string),
            detail: state.map(str::to_string),
            diagnostics: Vec::new(),
        },
    );
}
