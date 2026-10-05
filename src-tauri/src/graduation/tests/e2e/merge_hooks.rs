//! What a scripted turn of a merge run may do at a moment a turn cannot reach on
//! its own (GTE-FR-HWQM, GTE-FR-CZPM).
//!
//! The loop writes its `review verdict` record, and then applies the merge. No
//! turn runs between the two. A listener on the event that announces the record
//! runs on the loop's own thread, right after the record is durable, so what it
//! does lands exactly between the verdict and the apply.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{Listener, Manager};

use super::hooks::{log_path, TurnContext};
use super::settings::guard;
use super::script::TurnScript;
use crate::graduation::{GraduationLogStream, GraduationState};

/// What the run owns in the repository: the merge worktree directory, its
/// registration, its scratch branch and the private ref of its snapshot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Workspace {
    pub(crate) directory: bool,
    pub(crate) registration: bool,
    pub(crate) scratch_branch: bool,
    pub(crate) snapshot_ref: bool,
}

impl Workspace {
    pub(crate) const STANDING: Workspace = Workspace {
        directory: true,
        registration: true,
        scratch_branch: true,
        snapshot_ref: true,
    };
}

impl TurnContext {
    /// What the run in this context owns now, read from the repository.
    pub(crate) fn workspace(&self) -> Workspace {
        let run = &self.run_id;
        let repo = git2::Repository::open(&self.project_root).expect("the base repository");
        let directory = crate::graduation::store_base(&self.app)
            .expect("the run store")
            .merge_worktree(run);
        let scratch_branch = repo
            .find_branch(&format!("synthesis/merge-run/{run}"), git2::BranchType::Local)
            .is_ok();
        let snapshot_ref = repo.find_reference(&format!("refs/synthesis/merge/{run}")).is_ok();
        let registration = crate::streams::registers_worktree(&repo, &format!("{run}-mw"));
        Workspace {
            directory: directory.exists(),
            registration,
            scratch_branch,
            snapshot_ref,
        }
    }

    /// Whether a loop of this process is still inside the run, and whether the
    /// run still holds its stream.
    pub(crate) fn loop_state(&self) -> (bool, bool) {
        let state = self.app.state::<GraduationState>();
        (state.is_looping(&self.run_id), state.is_driving(&self.run_id))
    }

    /// Run `act` once, at the first moment the run's `review verdict` record is
    /// durable. The listener stands for the rest of the scenario and fires once.
    fn once_the_verdict_is_recorded(&self, act: Arc<dyn Fn(&TurnContext) + Send + Sync>) {
        let fired = Arc::new(AtomicBool::new(false));
        let ctx = self.clone();
        self.app
            .listen(crate::graduation::GRADUATION_LOG_RECORDS_APPENDED, move |event| {
                let payload: serde_json::Value =
                    serde_json::from_str(event.payload()).unwrap_or_default();
                let is_ours = payload["runId"] == ctx.run_id.as_str()
                    && payload["stream"] == "structured";
                if !is_ours || fired.load(Ordering::SeqCst) {
                    return;
                }
                let path = log_path(&ctx.app, &ctx.run_id, GraduationLogStream::Structured);
                let root = path.parent().expect("the stream's directory").to_path_buf();
                let last = guard(&root)
                    .read_text(&path)
                    .unwrap_or_default()
                    .lines()
                    .last()
                    .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                    .unwrap_or_default();
                if last["event"] == "review verdict" {
                    fired.store(true, Ordering::SeqCst);
                    act(&ctx);
                }
            });
    }
}

impl TurnScript {
    /// GTE-FR-HWQM: `act` runs between the `ready` verdict of this review and the
    /// apply that follows it, on the loop's own thread.
    pub(crate) fn then_once_the_verdict_is_recorded(
        self,
        act: impl Fn(&TurnContext) + Send + Sync + 'static,
    ) -> Self {
        let act: Arc<dyn Fn(&TurnContext) + Send + Sync> = Arc::new(act);
        self.then(move |ctx| ctx.once_the_verdict_is_recorded(act.clone()))
    }

    /// GRD-FR-EWTN: the author discards the run between the verdict and the apply.
    pub(crate) fn discarding_the_run_after_the_verdict(self) -> Self {
        self.then_once_the_verdict_is_recorded(|ctx| ctx.discard())
    }

    /// GRD-FR-MDQZ: the author pauses the run between the verdict and the apply.
    pub(crate) fn pausing_the_run_after_the_verdict(self) -> Self {
        self.then_once_the_verdict_is_recorded(|ctx| ctx.pause())
    }

    /// GRD-FR-IKVE: the structured log stream stops being writable between the
    /// verdict and the apply, so the record of the apply is the first one lost.
    pub(crate) fn losing_the_log_after_the_verdict(self) -> Self {
        self.then_once_the_verdict_is_recorded(|ctx| ctx.lose_log(GraduationLogStream::Structured))
    }

    /// GRD-FR-EWTN, GRD-FR-KZPT: the author discards the run while this turn
    /// runs, and what the run owns still stands, with the loop still inside the
    /// run and no longer holding its stream.
    pub(crate) fn discarding_the_run_and_checking_what_it_owns_stands(self) -> Self {
        self.then(|ctx| {
            assert_eq!(ctx.workspace(), Workspace::STANDING, "before the discard");
            assert_eq!(ctx.loop_state(), (true, true), "the loop holds the run and its stream");
            ctx.discard();
            assert_eq!(
                ctx.workspace(),
                Workspace::STANDING,
                "the discard pulled the worktree from under the turn"
            );
            assert_eq!(
                ctx.loop_state(),
                (true, false),
                "the loop is still inside the run, and the discard took the stream"
            );
        })
    }
}

impl super::outcome::Outcome {
    /// GRD-FR-KZPT: no loop of this process is inside the run any more.
    pub(crate) fn expect_no_loop_inside_the_run(self) -> Self {
        assert_eq!(
            self.ctx.loop_state(),
            (false, false),
            "[{}] no loop of this process is inside the run",
            self.name
        );
        self
    }

    /// GRD-FR-IKVE: a directory stands where the structured log stream's file
    /// belongs, as the hook left it.
    pub(crate) fn expect_structured_log_obstructed(self) -> Self {
        let path = log_path(&self.fx.app, &self.run.id, GraduationLogStream::Structured);
        assert!(path.is_dir(), "[{}] the structured stream is obstructed", self.name);
        self
    }
}
