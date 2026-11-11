//! The starts and the restart of an OpenSpec change run (OCR-FR-IDFC through
//! OCR-FR-TWDK, OCR-FR-SAVO, OCR-FR-NGGJ).
//!
//! Each start captures the input, runs every check of OCR-FR-GTOM in order,
//! and enqueues the run as one operation under the store hold, so a refusal
//! leaves no run record, no queue entry, and no dispatched turn.

use super::*;
use crate::comments::Participant;

/// OCR contract surface `ChangeReworkStart`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeReworkStart {
    pub run: GraduationRun,
    pub thread_id: String,
}

/// What the checks of one start are about.
enum Phase<'a> {
    Apply,
    Rework {
        instruction: &'a str,
        /// The request thread, where it stands before the start.
        thread_id: Option<&'a str>,
    },
}

impl Phase<'_> {
    fn name(&self) -> &'static str {
        match self {
            Phase::Apply => PHASE_APPLY,
            Phase::Rework { .. } => PHASE_REWORK,
        }
    }
}

/// OCR-FR-IDFC: `"start change apply"`. Off the main thread, because the
/// guard read can reach GitHub (GRD-FR-WJLO).
#[tauri::command(async)]
pub fn start_change_apply<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
) -> Result<GraduationRun, String> {
    start_change_apply_in_process(&app, &stream_id, &change)
}

/// OCR-FR-IDFC, OCR-FR-TWDK: the apply start, which the command and
/// `CAR-change-action-requests.md` call alike.
pub fn start_change_apply_in_process<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Result<GraduationRun, String> {
    prime_stream_write(app, stream_id);
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let outcome = checked(app, stream_id, change, &Phase::Apply, "start_change_apply")
        .and_then(|stream| enqueue(app, &stream, RunOpenSpec::of_phase(PHASE_APPLY, change), None));
    log_start(app, stream_id, change, PHASE_APPLY, &outcome);
    let run = outcome?;
    dispatch_pending(app);
    Ok(run)
}

/// OCR-FR-IDFC, OCR-FR-UAWO: `"start change rework"`. The start opens its
/// own request thread once every check has passed.
#[tauri::command(async)]
pub fn start_change_rework<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    instruction: String,
) -> Result<ChangeReworkStart, String> {
    prime_stream_write(&app, &stream_id);
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let phase = Phase::Rework { instruction: &instruction, thread_id: None };
    let stream = checked(&app, &stream_id, &change, &phase, "start_change_rework")
        .inspect_err(|code| log_refusal(&app, &stream_id, &change, PHASE_REWORK, code))?;
    // OCR-FR-SFDE: a discussion that cannot be opened refuses the start.
    let thread_id = open_request_thread(&app, &stream_id, &change, &instruction)
        .inspect_err(|code| log_refusal(&app, &stream_id, &change, PHASE_REWORK, code))?;
    let data =
        rework_data(&change, &instruction, &thread_id, crate::comments::Participant::synthesis());
    let outcome = enqueue(&app, &stream, data, None);
    log_start(&app, &stream_id, &change, PHASE_REWORK, &outcome);
    // OCR-FR-SFDE: an enqueue that fails leaves the thread, and names it
    // after the failure's own code and detail.
    let run = outcome.map_err(|failure| with_thread(&failure, &thread_id))?;
    dispatch_pending(&app);
    Ok(ChangeReworkStart { run, thread_id })
}

/// OCR-FR-SFDE: a failure after the request thread opened, as
/// `<code>[: <detail>]; thread: <thread-id>`.
pub fn with_thread(failure: &str, thread_id: &str) -> String {
    format!("{failure}; thread: {thread_id}")
}

/// OCR-FR-TWDK: a rework run in an existing request thread, for
/// `CAR-change-action-requests.md` alone. It opens no discussion.
pub fn start_change_rework_in_thread<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    instruction: &str,
    thread_id: &str,
    requester: &Participant,
) -> Result<GraduationRun, String> {
    prime_stream_write(app, stream_id);
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let phase = Phase::Rework { instruction, thread_id: Some(thread_id) };
    let outcome =
        checked(app, stream_id, change, &phase, "start_change_rework").and_then(|stream| {
            let data = rework_data(change, instruction, thread_id, requester.clone());
            enqueue(app, &stream, data, None)
        });
    log_start(app, stream_id, change, PHASE_REWORK, &outcome);
    let run = outcome?;
    dispatch_pending(app);
    Ok(run)
}

/// OCR-FR-SAVO, OCR-FR-NGGJ: a new run of the same phase, change, and stream
/// over the discarded run's captured input. The caller holds the store hold
/// and has checked that the run is discarded.
pub(crate) fn restart_change_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    discarded: &GraduationRun,
) -> Result<GraduationRun, String> {
    let data = discarded.openspec.clone().ok_or_else(|| ERR_RUN_STATE_NOT_PERMITTED.to_string())?;
    let phase = if data.phase == PHASE_REWORK {
        Phase::Rework {
            instruction: data.instruction.as_deref().unwrap_or_default(),
            thread_id: Some(data.thread_id.as_deref().unwrap_or_default()),
        }
    } else {
        Phase::Apply
    };
    let outcome =
        checked(app, &discarded.stream_id, &data.change, &phase, "restart_graduation_run")
            .and_then(|stream| {
                let fresh = RunOpenSpec { proposal_id: None, ..data.clone() };
                enqueue(app, &stream, fresh, Some(discarded))
            });
    log_start(app, &discarded.stream_id, &data.change, phase.name(), &outcome);
    let run = outcome?;
    dispatch_pending(app);
    Ok(run)
}

/// The data a rework run records (OCR-FR-TBGA).
fn rework_data(
    change: &str,
    instruction: &str,
    thread_id: &str,
    requester: Participant,
) -> RunOpenSpec {
    RunOpenSpec {
        instruction: Some(instruction.to_string()),
        thread_id: Some(thread_id.to_string()),
        requester: Some(requester),
        ..RunOpenSpec::of_phase(PHASE_REWORK, change)
    }
}

/// OCR-FR-GTOM: every check of a start, in order. The first that fails
/// refuses the start. Answers the stream the run belongs to.
fn checked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    phase: &Phase<'_>,
    operation: &str,
) -> Result<crate::streams::WorkStream, String> {
    if queue::project_key(app).is_empty() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    let stream =
        crate::streams::stream_of(app, stream_id).ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    openspec_start::require_mode_and_cli(app)?;
    if !crate::openspec::change::valid_change_name(change) {
        return Err(openspec_start::ERR_INVALID_CHANGE_NAME.to_string());
    }
    if !openspec_start::change_folder_stands(app, &stream, change) {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    // OCR-FR-MGPU: a restart's own run is discarded, so it is not counted.
    let queue = queue::project_queue(app).ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    if let Some(run_id) = active_openspec_run(&queue, &stream.id, change) {
        return Err(format!("{ERR_OPENSPEC_RUN_ACTIVE}: {run_id}"));
    }
    match phase {
        Phase::Apply => check_apply(app, &stream, change)?,
        Phase::Rework { instruction, thread_id } => {
            check_rework(app, &stream, change, instruction, *thread_id)?
        }
    }
    // OCR-FR-QBIF: the image preflight, then the write check on the stream
    // branch (GRD-FR-WJLO).
    commands::image_preflight(app)?;
    crate::git::write_guard::commands::require_branch_write(
        app,
        &stream.worktree(),
        Some(&stream.branch),
        operation,
    )?;
    Ok(stream)
}

/// OCR-FR-LNKQ: `blocked` refuses with the missing artifacts, a read that
/// fails refuses with its code, and `ready` and `all_done` admit the start.
/// Then a change edit proposal that holds files of the change refuses it.
fn check_apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    change: &str,
) -> Result<(), String> {
    match crate::openspec::apply::read_apply_progress_now(app, &stream.id, change) {
        Ok(read) if read.state == "blocked" => {
            Err(format!("{ERR_CHANGE_NOT_READY}: {}", read.missing_artifacts.join(",")))
        }
        Ok(_) => Ok(()),
        Err(error) => Err(format!("{ERR_APPLY_PROGRESS_UNAVAILABLE}: {}", error.code())),
    }?;
    // OCR-FR-LNKQ: a pending change edit proposal, or a closed one whose
    // commit is still owed, holds uncommitted files of the change.
    let store = machine_store(app)?;
    let held = crate::change_proposals::list_proposals_impl(&store, &stream.id, change).iter().any(
        |proposal| {
            proposal.state == crate::change_proposals::ChangeProposalState::Pending
                || proposal.commit_owed
        },
    );
    if held {
        return Err(ERR_PROPOSAL_PENDING.to_string());
    }
    Ok(())
}

/// OCR-FR-TCFQ: a blank instruction, a pending proposal of the change, and a
/// resolved request thread each refuse a rework start.
fn check_rework<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    change: &str,
    instruction: &str,
    thread_id: Option<&str>,
) -> Result<(), String> {
    if instruction.trim().is_empty() {
        return Err(ERR_BLANK_INSTRUCTION.to_string());
    }
    let store = machine_store(app)?;
    let pending = crate::change_proposals::list_proposals_impl(&store, &stream.id, change)
        .iter()
        .any(|proposal| proposal.state == crate::change_proposals::ChangeProposalState::Pending);
    if pending {
        return Err(ERR_PROPOSAL_PENDING.to_string());
    }
    if let Some(thread_id) = thread_id {
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: stream.id.clone(),
            change: change.to_string(),
        };
        let thread = crate::comments::locate_discussion_of(&store, &target, thread_id)
            .ok_or_else(|| crate::comments::ERR_DISCUSSION_NOT_FOUND.to_string())?;
        if thread.resolved {
            return Err(ERR_DISCUSSION_RESOLVED.to_string());
        }
    }
    Ok(())
}

/// The repository machine store of the open project (RMS).
pub(crate) fn machine_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<crate::fs::RootFs, String> {
    app.try_state::<ProjectState>()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?
        .require_store()
        .map_err(|_| ERR_NO_PROJECT_OPEN.to_string())
}

/// OCR-FR-UAWO: a new whole-change discussion whose first comment is the
/// instruction, in the author's name, through the human write path.
fn open_request_thread<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    instruction: &str,
) -> Result<String, String> {
    let project = app.try_state::<ProjectState>().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let settings = app
        .try_state::<crate::global_settings::GlobalSettingsStore>()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    let worktree = project.require_root()?;
    let author = crate::comments::acting_participant(&settings, &project)?;
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: change.to_string(),
    };
    let discussion = crate::comments::open_discussion_spoken_in(
        &store,
        &worktree,
        &target,
        None,
        crate::comments::Opening {
            body: instruction.to_string(),
            attachments: Vec::new(),
            spoken_original: None,
        },
        crate::comments::Stamp { by: &author, at: &crate::notes::now_rfc3339() },
    )?;
    // AGC-FR-TFEN: the instruction is a comment of the author.
    for comment in &discussion.comments {
        crate::agent_conversations::note_author_comment(app, &comment.id);
    }
    crate::comments::emit_discussion_changed(app, &discussion);
    Ok(discussion.id)
}

/// OCR-FR-NIOZ, OCR-FR-RYKA, OCR-FR-VUXZ: the run, durably, at the latest end
/// of the run order, with no draft and with `keep`.
fn enqueue<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream: &crate::streams::WorkStream,
    data: RunOpenSpec,
    restarted_from: Option<&GraduationRun>,
) -> Result<GraduationRun, String> {
    let input = match restarted_from {
        // OCR-FR-SAVO: over the discarded run's captured input.
        Some(discarded) => discarded.input.clone(),
        None => {
            let prompt = if data.phase == PHASE_REWORK {
                rework_prompt(&data.change)
            } else {
                apply_prompt(&data.change)
            };
            CapturedGraduationInput {
                draft_id: String::new(),
                draft_name: String::new(),
                prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
                prompt,
                captured_at: crate::notes::now_rfc3339(),
            }
        }
    };
    let mut run =
        commands::new_run(&queue::project_key(app), stream, input, StandingWork::Keep, None);
    run.openspec = Some(data);
    run.restarted_from_run_id = restarted_from.map(|discarded| discarded.id.clone());
    // GRS-FR-KDOY: both log streams are created in the write that creates the
    // run.
    crate::graduation::logs::initialize(app, &mut run);
    runs::save_run(app, &mut run).map_err(|failure| write_failure(&failure))?;
    events::announce_queue(app, &run);
    Ok(run)
}

/// OCR contract surface, typed refusals: a run record that does not write is
/// `write_failed`, with the reason of the store as its detail. A failure that
/// is already typed keeps its code.
fn write_failure(failure: &str) -> String {
    if failure == ERR_NO_PROJECT_OPEN {
        failure.to_string()
    } else {
        format!("{ERR_WRITE_FAILED}: {failure}")
    }
}

/// GRD-FR-WJLO: read the guard of the stream branch before the store hold,
/// so a slow read holds no other graduation command.
pub(crate) fn prime_stream_write<R: tauri::Runtime>(app: &tauri::AppHandle<R>, stream_id: &str)
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    if let Some(stream) = crate::streams::stream_of(app, stream_id) {
        crate::git::write_guard::commands::prime_branch_guard(
            app,
            &stream.worktree(),
            Some(&stream.branch),
        );
    }
}

/// OCR-FR-QDOZ: one `INFO` record of a start, or one `WARN` record of a
/// refusal. No record holds the instruction.
fn log_start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    phase: &str,
    outcome: &Result<GraduationRun, String>,
) {
    match outcome {
        Ok(run) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation enqueued an openspec change run",
            log_fields! {
                "run_id" => run.id.clone(),
                "stream_id" => stream_id.to_string(),
                "change" => change.to_string(),
                "phase" => phase,
            },
        ),
        Err(code) => log_refusal(app, stream_id, change, phase, code),
    }
}

fn log_refusal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    phase: &str,
    code: &str,
) {
    // The code alone: the detail after a colon names a run id or artifact ids,
    // which are safe, but a refusal of the write check can carry prose.
    let code = code.split(':').next().unwrap_or_default().trim().to_string();
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation refused an openspec change run start",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "change" => change.to_string(),
            "phase" => phase,
            "code" => code,
        },
    );
}
