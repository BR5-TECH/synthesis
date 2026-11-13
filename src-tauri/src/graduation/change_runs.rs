//! OpenSpec change runs (`OCR-openspec-change-runs.md`).
//!
//! An OpenSpec change run is a stream run with no draft, on a change that
//! already stands on its stream. An **apply run** implements the tasks of the
//! change and commits the work onto the stream branch. A **rework run** revises
//! the planning files of the change in a worktree of its own, and its finish
//! gives the result to the author as one change edit proposal. Both use the
//! state machine, the queues, the observability, and the loop of every run;
//! this module holds the data they carry and the places they differ.

use super::*;

mod start;

pub use start::*;

/// OCR-FR-VHQW: the phase of an apply run.
pub const PHASE_APPLY: &str = "apply";
/// OCR-FR-VHQW: the phase of a rework run.
pub const PHASE_REWORK: &str = "rework";

/// OCR-FR-GTOM: `openspec/changes/<change>/` is not a folder in the stream's
/// working copy.
pub const ERR_CHANGE_NOT_FOUND: &str = "change_not_found";
/// OCR-FR-MGPU: carries the run id after a colon.
pub const ERR_OPENSPEC_RUN_ACTIVE: &str = "openspec_run_active";
/// OCR-FR-LNKQ: carries the missing artifacts after a colon.
pub const ERR_CHANGE_NOT_READY: &str = "change_not_ready";
/// OCR-FR-LNKQ: carries the code of the read after a colon.
pub const ERR_APPLY_PROGRESS_UNAVAILABLE: &str = "apply_progress_unavailable";
/// OCR-FR-TCFQ.
pub const ERR_BLANK_INSTRUCTION: &str = "blank_instruction";
/// OCR-FR-TCFQ, OCR-FR-PBKH.
pub const ERR_PROPOSAL_PENDING: &str = crate::change_proposals::ERR_PROPOSAL_PENDING;
/// OCR-FR-TCFQ, OCR-FR-PBKH.
pub const ERR_DISCUSSION_RESOLVED: &str = crate::comments::ERR_DISCUSSION_RESOLVED;
/// OCR-FR-PBKH.
pub const ERR_WRITE_FAILED: &str = crate::change_proposals::ERR_WRITE_FAILED;

/// GRD-FR-LKGC, OCR contract surface `OpenSpecRunData`: what an OpenSpec run
/// records about its change.
///
/// The three fields of a rework run name who asked for it and where its result
/// goes. A record written before they existed reads as a run without them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOpenSpec {
    pub phase: String,
    pub change: String,
    /// OCR-FR-TBGA: the author's instruction of a rework run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    /// OCR-FR-TBGA: the request thread of a rework run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// OCR-FR-TBGA: the participant the finish of a rework run writes as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requester: Option<crate::comments::Participant>,
    /// OCR-FR-GWGE: the proposal the finish of a rework run recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
}

impl RunOpenSpec {
    /// The data of a run of `phase` on `change`, with no rework fields.
    pub fn of_phase(phase: &str, change: &str) -> Self {
        RunOpenSpec {
            phase: phase.to_string(),
            change: change.to_string(),
            instruction: None,
            thread_id: None,
            requester: None,
            proposal_id: None,
        }
    }
}

impl GraduationRun {
    /// OCR-FR-VHQW: an apply run.
    pub fn is_change_apply(&self) -> bool {
        self.openspec.as_ref().is_some_and(|data| data.phase == PHASE_APPLY)
    }

    /// OCR-FR-VHQW: a rework run.
    pub fn is_change_rework(&self) -> bool {
        self.openspec.as_ref().is_some_and(|data| data.phase == PHASE_REWORK)
    }

    /// OCR-FR-VHQW: an apply run or a rework run.
    pub fn is_change_run(&self) -> bool {
        self.is_change_apply() || self.is_change_rework()
    }

    /// OCR-FR-FZVF: `Apply <change>` or `Rework <change>`, for an OpenSpec
    /// change run alone.
    pub fn change_run_title(&self) -> Option<String> {
        let data = self.openspec.as_ref()?;
        match data.phase.as_str() {
            PHASE_APPLY => Some(format!("Apply {}", data.change)),
            PHASE_REWORK => Some(format!("Rework {}", data.change)),
            _ => None,
        }
    }

    /// OCR-FR-FZVF: the name a listing carries for the run: the title of an
    /// OpenSpec change run, or the draft's name.
    pub fn listing_name(&self) -> String {
        self.change_run_title().unwrap_or_else(|| self.input.draft_name.clone())
    }
}

/// OCR contract surface, fixed text: the captured prompt of an apply run.
pub fn apply_prompt(change: &str) -> String {
    format!("Apply the OpenSpec change \"{change}\": implement its tasks.")
}

/// OCR contract surface, fixed text: the captured prompt of a rework run.
pub fn rework_prompt(change: &str) -> String {
    format!("Rework the OpenSpec change \"{change}\" on the instruction of the author.")
}

/// OCR-FR-KXJX: the comment of a finish that found no difference.
pub fn nothing_to_change_comment(change: &str) -> String {
    format!("The rework pass found nothing to change in \"{change}\".")
}

/// OCR-FR-OXVH / CEP-FR-OIZT: the id of a rework run of `change` on the
/// stream that is not terminal, if one stands.
///
/// The run order's index names the candidates, so only the records of the
/// stream's non-terminal runs are opened.
pub fn rework_run_on_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Option<String> {
    let key = queue::project_key(app);
    if key.is_empty() || stream_id.is_empty() {
        return None;
    }
    let fs = store::store_fs(app).ok()?;
    let base = store::store_base(app).ok()?;
    let index = store::read_queue_index(&fs, &base, &key);
    index
        .runs
        .iter()
        .filter(|entry| entry.stream_id == stream_id && !entry.state.is_terminal())
        .filter_map(|entry| store::read_run_record(&fs, &base, &entry.run_id).ok())
        .find(|run| {
            run.is_change_rework() && run.openspec.as_ref().is_some_and(|d| d.change == change)
        })
        .map(|run| run.id)
}

/// OCR-FR-MGPU: the id of a non-terminal OpenSpec run of any phase on the
/// stream that names `change`.
pub(crate) fn active_openspec_run(
    queue: &GraduationQueue,
    stream_id: &str,
    change: &str,
) -> Option<String> {
    queue
        .runs
        .iter()
        .find(|run| {
            run.stream_id == stream_id
                && !run.state.is_terminal()
                && run.openspec.as_ref().is_some_and(|data| data.change == change)
        })
        .map(|run| run.id.clone())
}

/// OCR-FR-FJFI: remove the rework worktree, its registration, and its
/// scratch branch of an **ended** rework run.
///
/// Idempotent. A run a loop of this process is still inside is left alone:
/// its own thread removes what the run owns when the turn that stands in it
/// has returned. A removal that fails is logged and stops nothing.
pub fn reclaim_ended_rework_run<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    if !run.is_change_rework() || !run.state.is_terminal() {
        return;
    }
    if app.try_state::<GraduationState>().is_some_and(|state| state.is_looping(&run.id)) {
        return;
    }
    if !driver::rework_workspace::stands(app, run) {
        return;
    }
    match driver::rework_workspace::release(app, run) {
        Ok(()) => logging::log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation reclaimed what a rework run owned",
            log_fields! { "run_id" => run.id.clone(), "stream_id" => run.stream_id.clone() },
        ),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not reclaim what a rework run owned",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        ),
    }
}
