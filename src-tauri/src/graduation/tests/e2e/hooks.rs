//! What a scripted turn may do to the application while it runs
//! (GTE-FR-HWQM), and the runs a scenario enqueues beside its own
//! (GTE-FR-YAEB).
//!
//! A hook acts through the production commands and the production state, as
//! the author, the application or the machine does. Only the obstructions a
//! machine makes — a held index, a log stream that cannot be written — are
//! made on the filesystem, through a guarded handle, because nothing in the
//! application makes them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::Manager;

use super::super::{Runtime, ScriptedDispatch};
use super::script::TurnScript;
use super::settings::guard;
use crate::graduation::{GraduationInterruptionReason, GraduationLogStream, GraduationState};

/// A run a scenario enqueued beside the one it drives.
#[derive(Clone)]
pub(crate) struct OtherRun {
    pub(crate) run_id: String,
    pub(crate) stream_id: String,
    pub(crate) branch: String,
    pub(crate) worktree: PathBuf,
    /// Whether a hook's drive of it claimed its stream. `None` until one tried.
    pub(crate) claimed: Option<bool>,
}

/// The runs a scenario enqueued beside its own, by the label it gave each.
#[derive(Default)]
pub(crate) struct Others {
    runs: Mutex<BTreeMap<&'static str, OtherRun>>,
}

impl Others {
    pub(crate) fn insert(&self, label: &'static str, run: OtherRun) {
        self.runs.lock().unwrap().insert(label, run);
    }

    pub(crate) fn get(&self, label: &'static str) -> OtherRun {
        self.runs
            .lock()
            .unwrap()
            .get(label)
            .cloned()
            .unwrap_or_else(|| panic!("no run was enqueued under the label {label:?}"))
    }

    fn record_claim(&self, label: &'static str, claimed: bool) {
        if let Some(run) = self.runs.lock().unwrap().get_mut(label) {
            run.claimed = Some(claimed);
        }
    }
}

/// Everything a hook may act on: the application, the run the turn belongs
/// to, and where that run's files stand.
#[derive(Clone)]
pub(crate) struct TurnContext {
    pub(crate) app: tauri::AppHandle<Runtime>,
    pub(crate) run_id: String,
    pub(crate) stream_id: String,
    pub(crate) worktree: PathBuf,
    pub(crate) project_root: PathBuf,
    pub(crate) others: Arc<Others>,
    /// GTE-FR-CZPM: the repository update guard, where a hook or an act holds it.
    pub(crate) guard: super::merge_acts::HeldGuard,
}

impl TurnContext {
    /// The same application, bound to another run of it.
    pub(crate) fn for_run(&self, run_id: &str, stream_id: &str, worktree: &Path) -> Self {
        Self {
            run_id: run_id.to_string(),
            stream_id: stream_id.to_string(),
            worktree: worktree.to_path_buf(),
            ..self.clone()
        }
    }

    /// GRD-FR-MDQZ: the author's own pause.
    pub(crate) fn pause(&self) {
        crate::graduation::pause_graduation_run(self.app.clone(), self.run_id.clone())
            .unwrap_or_else(|reason| panic!("the pause was refused: {reason}"));
    }

    /// GRD-FR-EWTN: the author discards the run.
    pub(crate) fn discard(&self) {
        without_loop(&self.app, || {
            crate::graduation::discard_graduation_run(self.app.clone(), self.run_id.clone())
        })
        .unwrap_or_else(|reason| panic!("the discard was refused: {reason}"));
    }

    /// GRD-FR-TWMA: a project change or a shutdown stops every running turn.
    pub(crate) fn stop_application(&self, reason: GraduationInterruptionReason) {
        crate::graduation::interrupt_running(&self.app, reason);
    }

    /// GRD-FR-VLFO: a run enqueued on this stream, recorded under a label.
    ///
    /// The record is written directly rather than through `start_graduation`,
    /// so no draft lock, no image preflight and no dispatch is involved. Its
    /// auto-start stays on, so a later release of the stream may offer it a
    /// dispatch the scenario then expects.
    pub(crate) fn enqueue_beside(&self, label: &'static str, prompt: &str) {
        let stream = crate::streams::stream_of(&self.app, &self.stream_id).expect("the stream");
        let input = crate::graduation::CapturedGraduationInput {
            draft_id: format!("d-{label}"),
            draft_name: "editor draft".to_string(),
            prompt: prompt.to_string(),
            prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
            captured_at: crate::notes::now_rfc3339(),
        };
        let project_key = self.app.state::<crate::project::ProjectState>().slot_key();
        let mut run = crate::graduation::commands::new_run(
            &project_key,
            &stream,
            input,
            crate::graduation::StandingWork::default(),
            None,
        );
        crate::graduation::save_run(&self.app, &mut run).expect("a saved run");
        self.others.insert(
            label,
            OtherRun {
                run_id: run.id.clone(),
                stream_id: stream.id.clone(),
                branch: stream.branch.clone(),
                worktree: stream.worktree(),
                claimed: None,
            },
        );
    }

    /// GRB-FR-TXVL: the author stops the update of the stream.
    pub(crate) fn cancel_update(&self) {
        crate::streams::cancel_work_stream_update(self.app.clone(), self.stream_id.clone())
            .unwrap_or_else(|reason| panic!("the update cancellation was refused: {reason}"));
    }


    /// GTC-FR-19: Git's own lock on the stream's index, as a concurrent `git`
    /// process holds it.
    pub(crate) fn hold_index(&self) {
        hold_index_of(&self.worktree);
    }

    /// GRD-FR-IKVE: a directory stands where the log stream's file belongs, so
    /// every append fails as a full disk or a lost volume makes it fail.
    pub(crate) fn lose_log(&self, stream: GraduationLogStream) {
        let path = log_path(&self.app, &self.run_id, stream);
        let fs = guard(path.parent().expect("the run's log directory"));
        let _ = fs.delete_path(&path, false);
        fs.create_dir(&path).expect("the obstruction");
    }

    /// GRD-FR-XVUD: the process behind the run is gone, and the application
    /// that launches next reads the queue. A process that ended holds no claim,
    /// so the claim goes first.
    pub(crate) fn relaunch(&self) {
        self.app.state::<GraduationState>().release(&self.stream_id);
        crate::graduation::list_graduation_queue(self.app.clone())
            .unwrap_or_else(|reason| panic!("the queue was not readable: {reason}"));
    }

    /// GRD-FR-BNTC: drive a run the scenario enqueued, and record whether its
    /// stream could be claimed.
    ///
    /// On a thread of its own: the hook runs inside the turn's runtime, and a
    /// run that is claimed builds a runtime of its own to be driven on.
    pub(crate) fn drive_other(&self, label: &'static str, scripts: Vec<TurnScript>) -> bool {
        let other = self.others.get(label);
        let context = self.for_run(&other.run_id, &other.stream_id, &other.worktree);
        let dispatch = super::scenario::bind(&context, scripts);
        let app = self.app.clone();
        let (run_id, stream_id) = (other.run_id.clone(), other.stream_id.clone());
        let claimed = std::thread::spawn(move || drive_on(&app, &run_id, &stream_id, dispatch))
            .join()
            .expect("the second run's thread ended");
        self.others.record_claim(label, claimed);
        claimed
    }
}

/// Drive one run to rest through the scripted seam, and say whether its stream
/// could be claimed.
pub(crate) fn drive_on(
    app: &tauri::AppHandle<Runtime>,
    run_id: &str,
    stream_id: &str,
    dispatch: Arc<ScriptedDispatch>,
) -> bool {
    crate::graduation::driver::drive_for_test(
        app,
        run_id,
        stream_id,
        dispatch as Arc<dyn crate::graduation::driver::GraduationDispatch<Runtime>>,
    )
}

/// Run one command with the queue's own dispatch turned off, so nothing it
/// starts reaches the production seam (GTE-FR-YAEB).
pub(crate) fn without_loop<T>(app: &tauri::AppHandle<Runtime>, act: impl FnOnce() -> T) -> T {
    let state = app.state::<GraduationState>();
    let was_enabled = state.loop_enabled();
    state.set_loop_enabled(false);
    let answer = act();
    state.set_loop_enabled(was_enabled);
    answer
}

/// Where Git's own index lock of a working copy stands, and the git directory
/// that holds it.
pub(crate) fn index_lock_of(worktree: &Path) -> (PathBuf, PathBuf) {
    let git_dir = git2::Repository::open(worktree)
        .expect("the stream's repository")
        .path()
        .to_path_buf();
    let lock = git_dir.join("index.lock");
    (git_dir, lock)
}

pub(crate) fn hold_index_of(worktree: &Path) {
    let (git_dir, lock) = index_lock_of(worktree);
    guard(&git_dir)
        .write_text_atomic(&lock, "")
        .expect("the index lock is writable");
}

/// The file one log stream of a run is written to.
pub(crate) fn log_path(
    app: &tauri::AppHandle<Runtime>,
    run_id: &str,
    stream: GraduationLogStream,
) -> PathBuf {
    crate::graduation::logs::paths_for(app, run_id)
        .expect("the run's log paths")
        .stream(stream)
}
