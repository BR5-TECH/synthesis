//! The recording of a plan update and of a `plan_findings` rest
//! (`OPU-openspec-plan-updates.md` OPU-FR-QXUE through OPU-FR-BKZX,
//! OPU-FR-MCSG, OPU-FR-VGFJ).
//!
//! A valid plan update is recorded in three steps: the plan thread, the
//! question set, and the plan proposal. The checkpoint records the id of each
//! step when it completes, so a step that is refused rests the run `blocked`
//! at the resume phase `record`, and Continue takes the first step that is not
//! done. Only when every step has succeeded does the run rest for the author.

use std::path::Path;

use super::drive::Step;
use super::turns::note_blocked;
use super::*;
use crate::change_proposals::{AppSites, NewChangeProposal, RecordRefusal};
use crate::comments::{self, Participant};
use crate::graduation::logs::{self, GraduationLogProducer, StructuredEvent};
use crate::graduation::plan_updates::{self as plan, scratch, PlanRecording};
use crate::graduation::{change_runs, GraduationRest};

/// OPU-FR-QXUE, OPU-FR-MCSG: start a recording and run its steps.
pub(super) fn begin<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    recording: PlanRecording,
) -> Step {
    run.checkpoint.plan.recording = Some(recording);
    run.checkpoint.plan.correction = None;
    // A plan update is read in place of the check gate (OPU-FR-MBIY).
    run.checkpoint.check_gate = None;
    run.checkpoint.continue_check_correction = None;
    let _ = crate::graduation::save_run(app, run);
    proceed(app, run, worktree)
}

/// OPU-FR-BKZX / GXD-FR-GMDI: the recording, resumed at the phase `record`.
/// No turn is dispatched and no pass is spent.
pub(super) fn record_again<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
) -> Step {
    if run.checkpoint.plan.recording.is_none() {
        return Step::Stop;
    }
    if transitions::transition(app, run, GraduationRunState::Working).is_err() {
        return Step::Stop;
    }
    proceed(app, run, worktree)
}

/// The steps that are not done, then the rest.
fn proceed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
) -> Step {
    // A discard or a pause that landed before the recording stands.
    if crate::graduation::load_run(app, &run.id).is_ok_and(|stored| {
        stored.state.is_terminal() || stored.state == GraduationRunState::Interrupted
    }) {
        return Step::Stop;
    }
    let (Some(mut recording), Some(change)) =
        (run.checkpoint.plan.recording.clone(), run.openspec.as_ref().map(|d| d.change.clone()))
    else {
        return Step::Stop;
    };
    let store = match change_runs::machine_store(app) {
        Ok(store) => store,
        Err(_) => return block(app, run, change_runs::ERR_WRITE_FAILED),
    };
    let update = recording.rest == plan::REST_PLAN_UPDATE;
    // Step 1: the plan thread, as the Synthesis participant.
    if recording.thread_id.is_none() {
        let body =
            if update { recording.reason.clone() } else { findings_body(&change, &recording) };
        match open_thread(app, &store, run, &change, body) {
            Ok(id) => recording.thread_id = Some(id),
            Err(code) => return block(app, run, &code),
        }
        keep(app, run, &recording);
    }
    let thread_id = recording.thread_id.clone().unwrap_or_default();
    // Step 2: the question set, where there are questions.
    if update && !recording.questions.is_empty() && recording.set_id.is_none() {
        match reserve_set(app, &store, &thread_id, &recording) {
            Ok(set_id) => recording.set_id = Some(set_id),
            Err(code) => return block(app, run, &code),
        }
        keep(app, run, &recording);
    }
    // Step 3: the plan proposal, where there are hunks.
    if update && !recording.proposal_done {
        match record_proposal(app, &store, run, worktree, &change, &thread_id, &recording) {
            Ok(proposal_id) => {
                recording.proposal_id = proposal_id;
                recording.proposal_done = true;
            }
            Err(code) => return block(app, run, &code),
        }
        keep(app, run, &recording);
    }
    rest(app, run, worktree, &change, recording)
}

/// OPU-FR-BKZX: the checkpoint records each step when it completes.
fn keep<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    recording: &PlanRecording,
) {
    run.checkpoint.plan.recording = Some(recording.clone());
    let _ = crate::graduation::save_run(app, run);
}

/// OPU-FR-FOPN, OPU-FR-MCSG: every step has succeeded. The rest is written
/// with the checkpoint, the run moves to `awaiting_author`, and the scratch
/// folder goes.
fn rest<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    change: &str,
    recording: PlanRecording,
) -> Step {
    let event =
        StructuredEvent::new(GraduationLogProducer::StageTransition, "run rested for the author")
            .leaving(run.observability.current_stage)
            .with("reason", recording.rest.clone())
            .with("origin", recording.origin.clone())
            .with("questions", recording.questions.len() as u64)
            .with("proposal", recording.proposal_id.is_some());
    // GRD-FR-IKVE: the rest is set only after its record is stored.
    if !logs::emit(app, run, event) {
        return Step::Stop;
    }
    run.rest = Some(GraduationRest::of_recording(crate::notes::now_rfc3339(), &recording));
    run.checkpoint.plan.rest_session = recording.session.clone();
    run.checkpoint.plan.recording = None;
    run.checkpoint.resume_part = None;
    transitions::clear_block_count(run);
    let _ = transitions::transition(app, run, GraduationRunState::AwaitingAuthor);
    if let Err(reason) = scratch::clear(worktree) {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation could not remove the scratch folder of a plan update",
            log_fields! { "run_id" => run.id.clone(), "code" => reason },
        );
    }
    // OPU-FR-VGFJ: the counts and the ids, and no text.
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation recorded a plan rest",
        log_fields! {
            "run_id" => run.id.clone(),
            "change" => change.to_string(),
            "reason" => recording.rest.clone(),
            "origin" => recording.origin.clone(),
            "questions" => recording.questions.len() as u64,
            "findings" => recording.findings.len() as u64,
            "thread_id" => recording.thread_id.clone().unwrap_or_default(),
            "set_id" => recording.set_id.clone().unwrap_or_default(),
            "proposal_id" => recording.proposal_id.clone().unwrap_or_default(),
        },
    );
    Step::Stop
}

/// OPU-FR-BKZX: rest the run `blocked` with the code of the store that
/// refused the step, at the resume phase `record`.
fn block<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    code: &str,
) -> Step {
    note_blocked(app, run, code);
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation could not record a plan update",
        log_fields! { "run_id" => run.id.clone(), "code" => code.to_string() },
    );
    let (message, clears_by) = match code {
        change_runs::ERR_PROPOSAL_PENDING => (
            "A change edit proposal of this change waits for the author's decision, so the plan update was not recorded.",
            "Decide the open proposal, then continue the run.",
        ),
        _ => (
            "The plan update could not be recorded in the change discussion.",
            "Continue the run to record the plan update again.",
        ),
    };
    let _ = transitions::block(app, run, code, message, clears_by, phases::PART_RECORD);
    Step::Stop
}

/// OPU-FR-QXUE, OPU-FR-MCSG: a new whole-change discussion whose first
/// comment is `body`, appended as the Synthesis participant.
fn open_thread<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &crate::fs::RootFs,
    run: &GraduationRun,
    change: &str,
    body: String,
) -> Result<String, String> {
    let target = comments::DiscussionTarget::Change {
        stream_id: run.stream_id.clone(),
        change: change.to_string(),
    };
    let discussion = comments::open_discussion_unchecked(
        store,
        &target,
        None,
        body,
        Vec::new(),
        &Participant::synthesis(),
        &crate::notes::now_rfc3339(),
    )
    .map_err(|_| change_runs::ERR_WRITE_FAILED.to_string())?;
    comments::emit_discussion_changed(app, &discussion);
    Ok(discussion.id)
}

/// OPU-FR-MCSG: the fixed text, then the description and the correction of
/// each `plan` finding, in order.
fn findings_body(change: &str, recording: &PlanRecording) -> String {
    let mut body = plan::plan_findings_comment(change);
    for (index, finding) in recording.findings.iter().filter(|f| f.is_plan()).enumerate() {
        body.push_str(&format!(
            "\n\n{}. {}\n   Correction: {}",
            index + 1,
            finding.description,
            finding.correction
        ));
    }
    body
}

/// OPU-FR-VXVO: the question set, with a fresh id, the Synthesis participant
/// as `asked_by`, and each option's `summary` as its `value`.
fn reserve_set<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &crate::fs::RootFs,
    thread_id: &str,
    recording: &PlanRecording,
) -> Result<String, String> {
    let project = app
        .try_state::<crate::project::ProjectState>()
        .ok_or_else(|| change_runs::ERR_WRITE_FAILED.to_string())?;
    let worktree = project.require_root().map_err(|_| change_runs::ERR_WRITE_FAILED.to_string())?;
    let set = comments::PendingQuestionSet {
        set_id: crate::notes::new_note_id(),
        discussion_id: thread_id.to_string(),
        asked_by: Participant::synthesis(),
        asked_at: crate::notes::now_rfc3339(),
        questions: recording
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| comments::PendingQuestion {
                position: index + 1,
                text: question.question.clone(),
                options: question
                    .options
                    .iter()
                    .enumerate()
                    .map(|(at, option)| comments::QuestionOption {
                        position: at + 1,
                        value: option.summary.clone(),
                        description: Some(option.description.clone()),
                    })
                    .collect(),
            })
            .collect(),
    };
    match comments::reserve_discussion_question_set(app, store, &worktree, thread_id, &set) {
        Ok(()) => Ok(set.set_id),
        // The plan thread is the run's own, so a set that holds its slot is
        // the one an earlier attempt of this step reserved.
        Err(code) if code == comments::ERR_QUESTION_SET_ALREADY_PENDING => {
            comments::read_question_set(store, &worktree, thread_id)
                .map(|held| held.set_id)
                .ok_or(code)
        }
        Err(code) => Err(code),
    }
}

/// OPU-FR-OQRA: the plan proposal, into the plan thread, as the Synthesis
/// participant, with the reason as its rationale. `None` where the scratch
/// folder gives no hunk.
fn record_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &crate::fs::RootFs,
    run: &GraduationRun,
    worktree: &Path,
    change: &str,
    thread_id: &str,
    recording: &PlanRecording,
) -> Result<Option<String>, String> {
    let hunks = match scratch::read(worktree, change) {
        Ok(hunks) => hunks,
        Err(scratch::ScratchError::Refused(cause)) => return Err(cause.as_str().to_string()),
        Err(scratch::ScratchError::Unreadable(_)) => {
            return Err(change_runs::ERR_WRITE_FAILED.to_string())
        }
    };
    if hunks.is_empty() {
        return Ok(None);
    }
    let synthesis = Participant::synthesis();
    let recorded = crate::change_proposals::record_plan_proposal(
        app,
        store,
        &AppSites { app },
        NewChangeProposal {
            stream_id: &run.stream_id,
            change,
            hunks: &hunks,
            rationale: &recording.reason,
            agent: &synthesis,
            thread_id,
        },
        &run.id,
    );
    match recorded {
        Ok(proposal) => Ok(Some(proposal.id)),
        Err(RecordRefusal::NoChange) => Ok(None),
        // The proposal an earlier attempt of this step recorded in the plan
        // thread is this step's.
        Err(RecordRefusal::ProposalPending) => {
            match crate::change_proposals::pending_change_proposal(store, &run.stream_id, change) {
                Some(pending) if pending.thread_id == thread_id => Ok(Some(pending.id)),
                _ => Err(RecordRefusal::ProposalPending.code().to_string()),
            }
        }
        Err(refusal) => Err(refusal.code().to_string()),
    }
}
