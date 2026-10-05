//! What one scenario's run did, and the assertions that read its record and
//! its dispatches (GTE-FR-TCUW, GTE-FR-XKGB, GTE-FR-JYWB).
//!
//! The repository assertions stand in `repository_assert.rs` and the acts in
//! `acts.rs`, each an `impl Outcome` of its own.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::Manager;

use super::super::{Fixture, ScriptedDispatch, Seen};
use super::dispatch_assert::DispatchAssert;
use super::hooks::TurnContext;
use crate::graduation::{
    GraduationEscalationOrigin, GraduationLogStream, GraduationRun, GraduationRunState,
};

/// What one scenario's run did, and the assertions that read it.
///
/// GTE-FR-JYWB: every assertion names the scenario and, where it belongs to
/// one, the turn — so a failure in a suite of journeys says which journey.
pub(crate) struct Outcome {
    pub(super) name: &'static str,
    pub(super) fx: Fixture,
    pub(super) ctx: TurnContext,
    pub(super) stream_id: String,
    pub(super) branch: String,
    pub(super) worktree: PathBuf,
    /// The revision the stream stood at before the run in focus started.
    pub(super) base: Option<String>,
    pub(super) run: GraduationRun,
    pub(super) dispatch: Arc<ScriptedDispatch>,
    /// The run the focus moved from, where an act moved it.
    pub(super) previous: Option<String>,
    /// The stream branch's history before a revert, newest first.
    pub(super) history_before: Vec<String>,
    /// GTE-FR-NMUF: the base revision an update is pinned to, where a scenario
    /// pinned one before the base moved on.
    pub(super) base_pin: Option<String>,
    /// The refusal the last merge or update request answered with.
    pub(super) reconcile_refusal: Option<String>,
    /// The stream and base branch tips before the last merge or update.
    pub(super) tips_before: Option<(Option<String>, String)>,
    /// The attempt a merge or update stood at before the author answered it.
    pub(super) attempt_before: Option<String>,
    /// GTE-FR-LMXV: what both branches and both working copies held when the
    /// last merge started.
    pub(super) live_before: Option<super::merge_arrange::LiveState>,
    /// GTE-FR-YAEB: the runs the production queue was offered a dispatch for
    /// during the last act, each with whether its stream was free.
    pub(super) offered: Vec<(String, bool)>,
    /// GTE-FR-TNZF: what the last request of a merge answered.
    pub(super) merge_answer: Option<Result<crate::streams::StreamMergeResult, String>>,
}

impl Outcome {
    pub(crate) fn fixture(&self) -> &Fixture {
        &self.fx
    }

    /// The stream's working copy, so a scenario can say what a turn's execution
    /// directory must and must not be.
    pub(crate) fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// Every dispatch the loop made in the last act, in order (GTE-FR-XKGB).
    pub(crate) fn dispatches(&self) -> Vec<Seen> {
        self.dispatch.seen()
    }

    // -- the run record (GTE-FR-TCUW) ---------------------------------------

    pub(crate) fn expect_state(self, state: GraduationRunState) -> Self {
        assert_eq!(self.run.state, state, "[{}] the run's persisted state", self.name);
        self
    }

    pub(crate) fn expect_pass(self, pass: u32) -> Self {
        assert_eq!(self.run.pass(), pass, "[{}] the pass the checkpoint holds", self.name);
        self
    }

    pub(crate) fn expect_turns(self, work: u32, review: u32) -> Self {
        assert_eq!(self.run.work_turns, work, "[{}] work turns spent", self.name);
        assert_eq!(
            self.run.review_turns, review,
            "[{}] review turns spent",
            self.name
        );
        self
    }

    pub(crate) fn expect_pass_window(self, floor: u32, limit: u32) -> Self {
        assert_eq!(
            (self.run.pass_floor(), self.run.checkpoint.pass_limit),
            (floor, limit),
            "[{}] the checkpoint's pass budget window",
            self.name
        );
        self
    }

    /// GRL-FR-GQAB: the findings the next work turn is to act on.
    pub(crate) fn expect_loop_instruction_contains(self, fragment: &str) -> Self {
        let held = self
            .run
            .checkpoint
            .loop_instruction
            .clone()
            .unwrap_or_default();
        assert!(
            held.contains(fragment),
            "[{}] the checkpoint's instruction holds {fragment:?}, but it is {held:?}",
            self.name
        );
        self
    }

    /// GXD-FR-HGSU: the ordered questions the run is waiting on.
    pub(crate) fn expect_escalation_questions(self, questions: &[&str]) -> Self {
        let asked: Vec<String> = self
            .run
            .escalation
            .as_ref()
            .map(|e| e.questions.iter().map(|q| q.question.clone()).collect())
            .unwrap_or_default();
        assert_eq!(
            asked,
            questions.iter().map(|q| q.to_string()).collect::<Vec<_>>(),
            "[{}] the escalation's questions, in the order they were asked",
            self.name
        );
        self
    }

    pub(crate) fn expect_no_escalation(self) -> Self {
        assert!(
            self.run.escalation.is_none(),
            "[{}] the run holds no escalation",
            self.name
        );
        self
    }

    /// GXD-FR-XPUR: which phase raised the escalation the run waits on.
    pub(crate) fn expect_escalation_origin(self, origin: GraduationEscalationOrigin) -> Self {
        assert_eq!(
            self.run.escalation.as_ref().map(|e| e.origin),
            Some(origin),
            "[{}] the phase the escalation came from",
            self.name
        );
        self
    }

    pub(crate) fn expect_blocker(self, code: &str) -> Self {
        let held = self.run.blocker.as_ref().map(|b| b.code.clone());
        assert_eq!(
            held.as_deref(),
            Some(code),
            "[{}] the blocker the run rests on",
            self.name
        );
        self
    }

    /// GRL-FR-QZFB: how many times in a row the run met that blocker.
    pub(crate) fn expect_blocker_attempt(self, attempt: u32) -> Self {
        assert_eq!(
            self.run.blocker.as_ref().map(|b| b.attempt),
            Some(attempt),
            "[{}] the blocker's attempt",
            self.name
        );
        self
    }

    /// GRL-FR-XBUE / GRS-FR-KJVN: the run's own durable log says why it rested
    /// and what it spent, which is where the author reads it back.
    pub(crate) fn expect_rest_reason(self, reason: &str, pass_budget: u32) -> Self {
        let records = super::super::lines_of(
            &self.fx,
            &self.run.id,
            GraduationLogStream::Structured,
        );
        let rested: Vec<&serde_json::Value> = records
            .iter()
            .filter(|record| {
                record
                    .get("fields")
                    .and_then(|fields| fields.get("reason"))
                    .and_then(|v| v.as_str())
                    == Some(reason)
            })
            .collect();
        assert_eq!(
            rested.len(),
            1,
            "[{}] the run's log states {reason:?} exactly once",
            self.name
        );
        assert_eq!(
            rested[0]
                .get("fields")
                .and_then(|fields| fields.get("pass_budget"))
                .and_then(|v| v.as_u64()),
            Some(u64::from(pass_budget)),
            "[{}] the budget the run says it spent",
            self.name
        );
        self
    }

    /// GRL-FR-XBUE: the run rests rather than blocking, so the author reads a
    /// budget rather than an internal fault.
    pub(crate) fn expect_blocker_absent(self) -> Self {
        assert_eq!(
            self.run.blocker.as_ref().map(|b| b.code.clone()),
            None,
            "[{}] the run rests on no blocker",
            self.name
        );
        self
    }

    /// Put the run's checkpoint into a shape an older build wrote, so a journey
    /// can prove what this one does with a record it did not create.
    pub(crate) fn rewriting_checkpoint(
        mut self,
        edit: impl FnOnce(&mut crate::graduation::GraduationCheckpoint),
    ) -> Self {
        edit(&mut self.run.checkpoint);
        crate::graduation::save_run(&self.fx.app, &mut self.run)
            .unwrap_or_else(|reason| panic!("[{}] the record is writable: {reason}", self.name));
        self
    }

    pub(crate) fn expect_interruption(self, reason: &str) -> Self {
        let held = self
            .run
            .interruption
            .as_ref()
            .map(|i| i.reason.as_str().to_string());
        assert_eq!(
            held.as_deref(),
            Some(reason),
            "[{}] why the run stopped",
            self.name
        );
        self
    }

    pub(crate) fn expect_no_interruption(self) -> Self {
        assert!(
            self.run.interruption.is_none(),
            "[{}] the run records no interruption",
            self.name
        );
        self
    }

    /// GXD-FR-GMDI / GXD-FR-TJRV: the phase the checkpoint says the run
    /// resumes at.
    pub(crate) fn expect_resume_phase(self, part: &str) -> Self {
        assert_eq!(
            self.run.checkpoint.resume_part.as_deref(),
            Some(part),
            "[{}] the phase the run resumes at",
            self.name
        );
        self
    }

    /// What the author is told about the stop.
    pub(crate) fn expect_interruption_detail_contains(self, fragment: &str) -> Self {
        let held = self
            .run
            .interruption
            .as_ref()
            .map(|i| i.detail.clone())
            .unwrap_or_default();
        assert!(
            held.contains(fragment),
            "[{}] the interruption says {fragment:?}, but it says {held:?}",
            self.name
        );
        self
    }

    /// GXD-FR-QGYA: the change set the checkpoint holds, written durably before
    /// the run was reported as stopped.
    pub(crate) fn expect_checkpoint_paths(self, paths: &[&str]) -> Self {
        let mut held = self.run.checkpoint.changed_paths.clone();
        held.sort();
        assert_eq!(held, sorted(paths), "[{}] the change set the checkpoint holds", self.name);
        self
    }

    /// GRL-FR-CKBL: what the repository's ignore rules kept out of the change
    /// set.
    pub(crate) fn expect_hidden_paths(self, paths: &[&str]) -> Self {
        let mut held = self.run.checkpoint.hidden_paths.clone();
        held.sort();
        assert_eq!(held, sorted(paths), "[{}] the paths the ignore rules kept out", self.name);
        self
    }

    /// GRL-FR-DNKA: the work turn's own account of what it did and did not do.
    pub(crate) fn expect_agent_account_contains(self, fragment: &str) -> Self {
        let held = self.run.checkpoint.agent_account.clone().unwrap_or_default();
        assert!(
            held.contains(fragment),
            "[{}] the checkpoint's agent account holds {fragment:?}, but it is {held:?}",
            self.name
        );
        self
    }

    // -- standing work and the base (GRD-FR-HQPD, GRD-FR-KDWA) ----------------

    /// GRD-FR-KDWA: what the standing-work step did.
    pub(crate) fn expect_standing_work(
        self,
        committed: bool,
        pushed: Option<bool>,
        push_failure: Option<&str>,
    ) -> Self {
        let outcome = self
            .run
            .standing_work_outcome
            .clone()
            .unwrap_or_else(|| panic!("[{}] the run records its standing-work step", self.name));
        assert_eq!(
            outcome.commit.is_some(),
            committed,
            "[{}] whether the standing work was committed",
            self.name
        );
        assert_eq!(outcome.pushed, pushed, "[{}] whether the push landed", self.name);
        assert_eq!(
            outcome.push_failure.map(|failure| failure.code),
            push_failure.map(str::to_string),
            "[{}] the reason the push did not land",
            self.name
        );
        self
    }

    /// GRD-FR-HQPD: the standing commit is the run's base.
    pub(crate) fn expect_base_is_standing_commit(self) -> Self {
        let standing = self
            .run
            .standing_work_outcome
            .as_ref()
            .and_then(|outcome| outcome.commit.clone());
        assert!(standing.is_some(), "[{}] the standing work was committed", self.name);
        assert_eq!(
            self.run.base_commit, standing,
            "[{}] the run's base is the standing commit",
            self.name
        );
        self
    }

    /// GRD-FR-YBUM: the run's base is the revision the stream stood at.
    pub(crate) fn expect_base_is_head_before_run(self) -> Self {
        assert_eq!(
            self.run.base_commit, self.base,
            "[{}] the run's base is the revision the stream stood at",
            self.name
        );
        self
    }

    /// GRD-FR-BNTC: the run stacks on the last commit of the run before it.
    pub(crate) fn expect_base_is_last_commit_of_previous(self) -> Self {
        let previous = self.previous_run();
        assert_eq!(
            self.run.base_commit.as_deref(),
            previous.commits.last().map(String::as_str),
            "[{}] the run starts from the commit the run before it made",
            self.name
        );
        self
    }

    // -- the runs around this one --------------------------------------------

    /// GRD-FR-ZAMI: the run in focus was restarted from the run before it.
    pub(crate) fn expect_restarted_from_previous(self) -> Self {
        assert_eq!(
            self.run.restarted_from_run_id, self.previous,
            "[{}] the run names the run it was restarted from",
            self.name
        );
        self
    }

    /// The persisted state of the run the focus moved from.
    pub(crate) fn expect_previous_state(self, state: GraduationRunState) -> Self {
        assert_eq!(
            self.previous_run().state,
            state,
            "[{}] the state of the run before this one",
            self.name
        );
        self
    }

    /// GRD-FR-BNTC: the run the focus moved from still holds its stream, in
    /// memory and on the stream's durable record.
    pub(crate) fn expect_previous_holds_its_stream(self) -> Self {
        let previous = self.previous_run();
        let holder = self
            .fx
            .app
            .state::<crate::graduation::GraduationState>()
            .holder_of(&previous.stream_id);
        let busy = crate::streams::stream_of(&self.fx.app, &previous.stream_id)
            .and_then(|stream| stream.busy_run_id);
        assert_eq!(
            (holder.as_deref(), busy.as_deref()),
            (Some(previous.id.as_str()), Some(previous.id.as_str())),
            "[{}] the run before this one still holds its stream",
            self.name
        );
        self
    }

    /// The persisted state of a run the scenario enqueued beside its own.
    pub(crate) fn expect_state_of(self, label: &'static str, state: GraduationRunState) -> Self {
        let other = self.ctx.others.get(label);
        assert_eq!(
            self.fx.reload(&other.run_id).state,
            state,
            "[{}] the state of the run labelled {label}",
            self.name
        );
        self
    }

    /// GRD-FR-BNTC: whether a hook's drive of that run could claim its stream.
    pub(crate) fn expect_other_claimed(self, label: &'static str, claimed: bool) -> Self {
        assert_eq!(
            self.ctx.others.get(label).claimed,
            Some(claimed),
            "[{}] whether the run labelled {label} claimed its stream",
            self.name
        );
        self
    }

    fn previous_run(&self) -> GraduationRun {
        let id = self
            .previous
            .as_ref()
            .unwrap_or_else(|| panic!("[{}] no act moved the focus from another run", self.name));
        self.fx.reload(id)
    }

    // -- logs and the draft ----------------------------------------------------

    /// GRD-FR-IKVE: the run's log persistence failed on the named stream.
    pub(crate) fn expect_logs_failed(self, stream: GraduationLogStream) -> Self {
        let persistence = &self.run.logs.persistence;
        assert!(!persistence.is_healthy(), "[{}] the run's logs are failed", self.name);
        assert_eq!(
            persistence.failure.as_ref().map(|failure| failure.stream),
            Some(stream),
            "[{}] the log stream that failed",
            self.name
        );
        self
    }

    pub(crate) fn expect_logs_healthy(self) -> Self {
        assert!(
            self.run.logs.persistence.is_healthy(),
            "[{}] the run's logs are healthy",
            self.name
        );
        self
    }

    /// GRD-FR-DXWL: whether draft storage refuses a write to the run's draft.
    pub(crate) fn expect_draft_locked(self, locked: bool) -> Self {
        let refused =
            crate::graduation::require_unlocked_draft(&self.fx.app, &self.run.input.draft_id).is_err();
        assert_eq!(refused, locked, "[{}] whether the draft is locked", self.name);
        self
    }

    // -- the dispatches (GTE-FR-XKGB) ---------------------------------------

    /// The parts the loop dispatched, in order.
    pub(crate) fn expect_dispatch_order(self, parts: &[&str]) -> Self {
        let found: Vec<String> = self.dispatches().into_iter().map(|s| s.part).collect();
        assert_eq!(
            found,
            parts.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
            "[{}] the parts the loop dispatched, in order",
            self.name
        );
        self
    }

    /// Assert one dispatch by its position.
    ///
    /// The closure's own return is discarded, so a chain of dispatch
    /// assertions reads the same way whether or not it ends in a semicolon.
    pub(crate) fn expect_dispatch<T>(
        self,
        index: usize,
        check: impl FnOnce(DispatchAssert) -> T,
    ) -> Self {
        let seen = self.dispatches();
        let dispatch = seen.get(index).unwrap_or_else(|| {
            panic!(
                "[{}] dispatch {index} was never made; the loop made {}",
                self.name,
                seen.len()
            )
        });
        check(DispatchAssert {
            scenario: self.name,
            index,
            seen: dispatch.clone(),
        });
        self
    }

    pub(crate) fn expect_dispatch_count(self, count: usize) -> Self {
        assert_eq!(
            self.dispatches().len(),
            count,
            "[{}] the dispatches the loop made",
            self.name
        );
        self
    }
}

impl Outcome {
    /// GTE-FR-YAEB / WKS-FR-RQVM: the stream's queue offered the named run a
    /// dispatch once the stream was free, and nothing else was offered.
    pub(crate) fn expect_queue_offered_dispatch(self, label: &'static str) -> Self {
        let other = self.ctx.others.get(label);
        let attempts = self
            .fx
            .app
            .state::<crate::graduation::driver::drive::SpawnTripwire>()
            .take();
        assert_eq!(
            attempts,
            vec![(other.run_id.clone(), true)],
            "[{}] the stream's queue offered the run labelled {label} a dispatch, with the stream free",
            self.name
        );
        self
    }

    /// The id of the run in focus, for a check that acts on it directly.
    pub(crate) fn run_id(&self) -> &str {
        &self.run.id
    }
}

/// GTE-FR-YAEB: a scenario that let the production queue attempt a dispatch it
/// did not expect fails when it ends.
impl Drop for Outcome {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let attempts = self
            .fx
            .app
            .try_state::<crate::graduation::driver::drive::SpawnTripwire>()
            .map(|tripwire| tripwire.take())
            .unwrap_or_default();
        assert!(
            attempts.is_empty(),
            "[{}] the production queue tried to dispatch {attempts:?}",
            self.name
        );
    }
}

pub(super) fn sorted(paths: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    out.sort();
    out
}
