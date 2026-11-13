//! What the stores of other modules ask about a plan update: the
//! `apply_running` gate and its admission (OPU-FR-DEEB, OPU-FR-WOKJ), the
//! decisions that wait on the questions (OPU-FR-EVBZ), the hand-over of
//! answers (OPU-FR-TPYT), and what a run that ends leaves (OPU-FR-ZBXF,
//! OPU-FR-FSCV).

use super::*;
use crate::change_proposals::ApplyRunGate;

/// The records of the project's runs that are not terminal, read from the
/// run order's index.
fn live_runs<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<GraduationRun> {
    let key = queue::project_key(app);
    let (Ok(fs), Ok(base)) = (store::store_fs(app), store::store_base(app)) else {
        return Vec::new();
    };
    if key.is_empty() {
        return Vec::new();
    }
    store::read_queue_index(&fs, &base, &key)
        .runs
        .iter()
        .filter(|entry| !entry.state.is_terminal())
        .filter_map(|entry| store::read_run_record(&fs, &base, &entry.run_id).ok())
        .collect()
}

/// CEP-FR-YYZZ: the apply run of `change` on the stream that is not terminal.
fn apply_run_on<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Option<GraduationRun> {
    live_runs(app).into_iter().find(|run| {
        run.stream_id == stream_id
            && run.is_change_apply()
            && run.openspec.as_ref().is_some_and(|data| data.change == change)
    })
}

/// OPU-FR-DEEB, OPU-FR-WOKJ: the gate of an apply run of the change, with
/// the plan thread whose recording it admits, if any.
pub fn apply_run_gate<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Option<ApplyRunGate> {
    let run = apply_run_on(app, stream_id, change)?;
    let admits_thread = run
        .rest
        .as_ref()
        .filter(|rest| rest.reason == REST_PLAN_FINDINGS && rest.proposal_id.is_none())
        .filter(|_| run.state == GraduationRunState::AwaitingAuthor)
        .and_then(|rest| rest.thread_id.clone());
    Some(ApplyRunGate { run_id: run.id, admits_thread })
}

/// OPU-FR-WOKJ: write the id of the admitted plan proposal on the rest. A
/// write that fails is logged: the proposal stands, and the author can still
/// decide it.
pub fn admit_plan_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    proposal_id: &str,
) {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let written = runs::load_run(app, run_id).and_then(|mut run| {
        match run.rest.as_mut() {
            Some(rest) if rest.reason == REST_PLAN_FINDINGS && rest.proposal_id.is_none() => {
                rest.proposal_id = Some(proposal_id.to_string());
            }
            _ => return Ok(()),
        }
        runs::save_run(app, &mut run)
    });
    match written {
        Ok(()) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation admitted a plan proposal at a plan findings rest",
            log_fields! { "run_id" => run_id.to_string(), "proposal_id" => proposal_id.to_string() },
        ),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not write an admitted plan proposal on its rest",
            log_fields! { "run_id" => run_id.to_string(), "code" => reason },
        ),
    }
}

/// OPU-FR-EVBZ / CEP-FR-KGOL: whether `proposal_id` is the plan proposal of a
/// plan update whose question set still stands in its plan thread.
pub fn plan_questions_stand<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    proposal_id: &str,
) -> bool {
    let Some(rest) = live_runs(app).into_iter().filter_map(|run| run.rest).find(|rest| {
        rest.reason == REST_PLAN_UPDATE && rest.proposal_id.as_deref() == Some(proposal_id)
    }) else {
        return false;
    };
    let (Some(set_id), Some(thread_id)) = (rest.set_id.as_deref(), rest.thread_id.as_deref())
    else {
        return false;
    };
    let Some(project) = app.try_state::<ProjectState>() else {
        return false;
    };
    let (Ok(root), Ok(worktree)) = (project.require_store(), project.require_root()) else {
        return false;
    };
    crate::comments::read_question_set(&root, &worktree, thread_id)
        .is_some_and(|set| set.set_id == set_id)
}

/// OPU-FR-TPYT: what one hand-over wrote on a plan update rest, so that a
/// submission whose append then fails can put the rest back.
#[derive(Debug, Clone, PartialEq)]
pub struct AnswerHandOver {
    pub run_id: String,
    pub set_id: String,
    /// The answers the rest held before the hand-over.
    pub previous: Option<Vec<crate::comments::QuestionAnswer>>,
}

/// The run whose `plan_update` rest names `set_id`, if one is not terminal.
fn run_of_set<R: tauri::Runtime>(app: &tauri::AppHandle<R>, set_id: &str) -> Option<GraduationRun> {
    live_runs(app).into_iter().find(|run| {
        run.rest.as_ref().is_some_and(|rest| {
            rest.reason == REST_PLAN_UPDATE && rest.set_id.as_deref() == Some(set_id)
        })
    })
}

/// OPU-FR-TPYT: write the answers of a submission on the plan update rest
/// that names `set_id`, durably, before the submission appends anything. A
/// submission of no plan update set writes nothing and answers `None`. A
/// write that fails refuses the submission with `write_failed`.
pub fn hand_over_answers<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    set_id: &str,
    answers: &[crate::comments::QuestionAnswer],
) -> Result<Option<AnswerHandOver>, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let Some(mut run) = run_of_set(app, set_id) else {
        return Ok(None);
    };
    let mut previous = None;
    if let Some(rest) = run.rest.as_mut() {
        // OPU-FR-TPYT: a later submission of the same set replaces these.
        previous = rest.answers.replace(answers.to_vec());
    }
    if let Err(reason) = runs::save_run(app, &mut run) {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not hand plan update answers to the run",
            log_fields! { "run_id" => run.id.clone(), "set_id" => set_id.to_string(), "code" => reason },
        );
        return Err(ERR_WRITE_FAILED.to_string());
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation took the answers of a plan update",
        log_fields! {
            "run_id" => run.id.clone(),
            "change" => run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default(),
            "set_id" => set_id.to_string(),
            "answers" => answers.len() as u64,
        },
    );
    Ok(Some(AnswerHandOver { run_id: run.id, set_id: set_id.to_string(), previous }))
}

/// OPU-FR-TPYT, OPU-FR-THOZ: the submission did not append after its
/// hand-over, so the set still stands. The rest gets back the answers it held
/// before, and Continue stays refused while the set stands. A rest that no
/// longer names the set is left as it is. A write that fails is logged.
pub fn undo_hand_over<R: tauri::Runtime>(app: &tauri::AppHandle<R>, hand_over: &AnswerHandOver) {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let written = runs::load_run(app, &hand_over.run_id).and_then(|mut run| {
        match run.rest.as_mut() {
            Some(rest)
                if rest.reason == REST_PLAN_UPDATE
                    && rest.set_id.as_deref() == Some(hand_over.set_id.as_str()) =>
            {
                rest.answers.clone_from(&hand_over.previous);
            }
            _ => return Ok(false),
        }
        runs::save_run(app, &mut run).map(|()| true)
    });
    let (run_id, set_id) = (hand_over.run_id.clone(), hand_over.set_id.clone());
    match written {
        Ok(restored) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation took back plan update answers whose submission failed",
            log_fields! { "run_id" => run_id, "set_id" => set_id, "restored" => restored },
        ),
        Err(reason) => logging::log_error(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not take back plan update answers whose submission failed",
            log_fields! { "run_id" => run_id, "set_id" => set_id, "code" => reason },
        ),
    }
}

/// OPU-FR-ZBXF: the set and the plan thread of the plan update that a discard
/// or a revert ends: the set of a `plan_update` rest, or the set that a
/// recording reserved before a later step rested the run `blocked`
/// (OPU-FR-BKZX).
fn plan_question_set(run: &GraduationRun) -> Option<(&str, &str)> {
    let rest = run.rest.as_ref().filter(|rest| rest.reason == REST_PLAN_UPDATE);
    if let Some(rest) = rest {
        return rest.set_id.as_deref().zip(rest.thread_id.as_deref());
    }
    let recording = run
        .checkpoint
        .plan
        .recording
        .as_ref()
        .filter(|recording| recording.rest == REST_PLAN_UPDATE)?;
    recording.set_id.as_deref().zip(recording.thread_id.as_deref())
}

/// OPU-FR-ZBXF: a discard or a revert of a run at rest on `plan_update`
/// deletes the question set of its plan update where it stands. So does the
/// discard of a run whose recording reserved the set and then rested
/// `blocked`, because that set belongs to the same plan update. The plan
/// thread and the plan proposal stay. A deletion that fails is logged.
pub fn delete_plan_question_set<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    let Some((set_id, thread_id)) = plan_question_set(run) else {
        return;
    };
    let Some(project) = app.try_state::<ProjectState>() else {
        return;
    };
    let deleted = project.require_store().and_then(|root| {
        let worktree = project.require_root()?;
        crate::comments::delete_discussion_question_set(app, &root, &worktree, thread_id, set_id)
    });
    match deleted {
        Ok(deleted) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation removed the question set of a discarded plan update",
            log_fields! {
                "run_id" => run.id.clone(),
                "set_id" => set_id.to_string(),
                "deleted" => deleted,
            },
        ),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not remove the question set of a discarded plan update",
            log_fields! { "run_id" => run.id.clone(), "set_id" => set_id.to_string(), "code" => reason },
        ),
    }
}

/// OPU-FR-FSCV: the private ref of the plan base goes when the run reaches a
/// terminal state. Idempotent, and a removal that fails is logged.
pub fn reclaim_plan_base<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    if !run.is_change_apply() || !run.state.is_terminal() {
        return;
    }
    let Some(worktree) = direct::target_worktree(app, run) else {
        return;
    };
    match plan_base::remove_ref(&worktree, &run.id) {
        Ok(false) => {}
        Ok(true) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation removed the plan base of an ended apply run",
            log_fields! { "run_id" => run.id.clone() },
        ),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not remove the plan base of an ended apply run",
            log_fields! { "run_id" => run.id.clone(), "code" => reason },
        ),
    }
}
