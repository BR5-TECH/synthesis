//! What the author does to a run at rest, and what repairs the obstruction a
//! hook left (GTE-FR-TPLD, GTE-FR-RZKC, GTE-FR-YAEB).
//!
//! Each act that drives the run again rebinds the scripted seam, so the
//! dispatch assertions after it read the turns of that act alone.


use super::super::{Runtime, ScriptedDispatch};
use super::hooks::{drive_on, index_lock_of, log_path, without_loop};
use super::outcome::Outcome;
use super::scenario::bind;
use super::script::TurnScript;
use super::settings::{guard, head_of};
use crate::graduation::{
    GraduationEscalationAnswer, GraduationInterruptionReason, GraduationLogStream, GraduationRun,
    GraduationRunState, StandingWork,
};

/// What a refused command must leave as it was (GTE-FR-RZKC).
#[derive(Debug, PartialEq)]
struct Snapshot {
    state: GraduationRunState,
    blocker: Option<(String, u32)>,
    questions: Vec<String>,
    interruption: Option<GraduationInterruptionReason>,
    resume_part: Option<String>,
    pending_answers: usize,
    commits: Vec<String>,
    pass: u32,
    tip: Option<String>,
    /// Who holds the stream in memory, and whom its durable record names.
    holders: (Option<String>, Option<String>),
}

impl Outcome {
    fn snapshot(&self) -> Snapshot {
        let run = self.fx.reload(&self.run.id);
        Snapshot {
            state: run.state,
            blocker: run.blocker.as_ref().map(|b| (b.code.clone(), b.attempt)),
            questions: run
                .escalation
                .as_ref()
                .map(|e| e.questions.iter().map(|q| q.question.clone()).collect())
                .unwrap_or_default(),
            interruption: run.interruption.as_ref().map(|i| i.reason),
            resume_part: run.checkpoint.resume_part.clone(),
            pending_answers: run.checkpoint.pending_escalation_answers.len(),
            commits: run.commits.clone(),
            pass: run.pass(),
            tip: self.branch_tip(),
            holders: self.stream_holders(),
        }
    }

    // -- acts that drive the run again ----------------------------------------

    /// GRD-FR-CYIB: the author's Continue, and then the run driven again under
    /// a script of its own.
    pub(crate) fn continued(mut self, scripts: Vec<TurnScript>) -> Self {
        let continued = self.fx.continue_run(&self.run);
        self.dispatch = bind(&self.ctx, scripts);
        self.run = self.fx.drive(&continued, self.dispatch.clone());
        self
    }

    /// GXD-FR-BJYT: the author answers every question, and the run is driven
    /// again under a script of its own.
    pub(crate) fn answered(mut self, answers: &[&str], scripts: Vec<TurnScript>) -> Self {
        let questions = self
            .run
            .escalation
            .as_ref()
            .map(|e| e.questions.len())
            .unwrap_or_default();
        assert_eq!(
            questions,
            answers.len(),
            "[{}] an answer for every question the run asked",
            self.name
        );
        let payload = self.answers_for(answers);
        let answered = without_loop(&self.fx.app, || {
            crate::graduation::answer_graduation_escalation(
                self.fx.app.clone(),
                self.run.id.clone(),
                payload,
            )
        })
        .unwrap_or_else(|reason| panic!("[{}] the answers were refused: {reason}", self.name));
        self.dispatch = bind(&self.ctx, scripts);
        self.run = self.fx.drive(&answered, self.dispatch.clone());
        self
    }

    /// GRD-FR-ZAMI: the author restarts the discarded run on its own stream,
    /// and the new run is driven under a script of its own. The focus moves to
    /// the new run.
    pub(crate) fn restarted(mut self, choice: StandingWork, scripts: Vec<TurnScript>) -> Self {
        let previous = self.run.id.clone();
        let fresh = without_loop(&self.fx.app, || {
            crate::graduation::restart_graduation_run(
                self.fx.app.clone(),
                previous.clone(),
                self.stream_id.clone(),
                choice,
                None,
            )
        })
        .unwrap_or_else(|reason| panic!("[{}] the restart was refused: {reason}", self.name));
        self.base = self.branch_tip();
        self.ctx = self.ctx.for_run(&fresh.id, &self.stream_id, &self.worktree);
        self.dispatch = bind(&self.ctx, scripts);
        self.run = self.fx.drive(&fresh, self.dispatch.clone());
        self.previous = Some(previous);
        self
    }

    /// GTE-FR-YAEB: drive a run the scenario enqueued. The focus moves to it.
    pub(crate) fn driving(mut self, label: &'static str, scripts: Vec<TurnScript>) -> Self {
        let other = self.ctx.others.get(label);
        let previous = self.run.id.clone();
        self.stream_id = other.stream_id.clone();
        self.branch = other.branch.clone();
        self.worktree = other.worktree.clone();
        self.base = head_of(&self.worktree);
        self.ctx = self.ctx.for_run(&other.run_id, &other.stream_id, &other.worktree);
        self.dispatch = bind(&self.ctx, scripts);
        let queued = self.fx.reload(&other.run_id);
        self.run = self.fx.drive(&queued, self.dispatch.clone());
        self.previous = Some(previous);
        self
    }

    /// GRD-FR-CYIB: the author's Continue, with no dispatch after it, so the run
    /// waits `queued`.
    pub(crate) fn continuing_without_a_dispatch(mut self) -> Self {
        self.dispatch = ScriptedDispatch::new(Vec::new());
        self.run = self.fx.continue_run(&self.run);
        self
    }

    /// GTC-FR-19: another write takes the stream's index while the run rests.
    pub(crate) fn holding_the_index_while_resting(self) -> Self {
        super::hooks::hold_index_of(&self.worktree);
        self
    }

    /// GRD-FR-VLFO: the author starts another run on the stream in focus.
    pub(crate) fn enqueuing_on_the_stream(self, label: &'static str, prompt: &str) -> Self {
        self.ctx.enqueue_beside(label, prompt);
        self
    }

    /// GRD-FR-BNTC: a run the scenario enqueued cannot claim its stream now,
    /// so it takes no turn and stays queued.
    pub(crate) fn expect_cannot_start(self, label: &'static str) -> Self {
        let other = self.ctx.others.get(label);
        let dispatch = ScriptedDispatch::new(Vec::new());
        let started = drive_on(&self.fx.app, &other.run_id, &other.stream_id, dispatch.clone());
        assert!(!started, "[{}] the run labelled {label} could not start", self.name);
        assert_eq!(dispatch.turns_taken(), 0, "[{}] it took no turn", self.name);
        assert_eq!(
            self.fx.reload(&other.run_id).state,
            GraduationRunState::Queued,
            "[{}] it is still queued",
            self.name
        );
        self
    }

    // -- acts that end or undo a run ------------------------------------------

    /// GRD-FR-EWTN: the author discards the run at rest.
    pub(crate) fn discarded(mut self) -> Self {
        self.dispatch = ScriptedDispatch::new(Vec::new());
        without_loop(&self.fx.app, || {
            crate::graduation::discard_graduation_run(self.fx.app.clone(), self.run.id.clone())
        })
        .unwrap_or_else(|reason| panic!("[{}] the discard was refused: {reason}", self.name));
        self.run = self.fx.reload(&self.run.id);
        self
    }

    /// GRD-FR-BLCR: the author reverts what the run committed.
    pub(crate) fn reverted(mut self) -> Self {
        self.dispatch = ScriptedDispatch::new(Vec::new());
        self.history_before = self.history_since(None);
        crate::graduation::revert_graduation_run(self.fx.app.clone(), self.run.id.clone())
            .unwrap_or_else(|reason| panic!("[{}] the revert was refused: {reason}", self.name));
        self.run = self.fx.reload(&self.run.id);
        self
    }

    // -- repairs of what a hook obstructed --------------------------------------

    /// GTC-FR-19: the other write lets go of the stream's index.
    pub(crate) fn releasing_the_index(self) -> Self {
        let (git_dir, lock) = index_lock_of(&self.worktree);
        guard(&git_dir)
            .delete_path(&lock, false)
            .unwrap_or_else(|reason| panic!("[{}] the index lock was held: {reason}", self.name));
        self
    }

    /// The branch an arrangement created goes.
    pub(crate) fn deleting_branch(self, name: &str) -> Self {
        self.fx
            .repo()
            .find_branch(name, git2::BranchType::Local)
            .and_then(|mut branch| branch.delete())
            .unwrap_or_else(|reason| panic!("[{}] the branch {name} was deletable: {reason}", self.name));
        self
    }

    /// GRD-FR-IKVE: the log stream's file can be written again.
    pub(crate) fn repairing_the_log(self, stream: GraduationLogStream) -> Self {
        let path = log_path(&self.fx.app, &self.run.id, stream);
        guard(path.parent().expect("the run's log directory"))
            .delete_path(&path, true)
            .unwrap_or_else(|reason| panic!("[{}] the log was obstructed: {reason}", self.name));
        self
    }

    /// The stream's working copy goes while the run rests.
    pub(crate) fn removing_the_working_copy(self) -> Self {
        guard(self.worktree.parent().expect("the working copy's parent"))
            .delete_path(&self.worktree, true)
            .unwrap_or_else(|reason| panic!("[{}] the working copy was removable: {reason}", self.name));
        self
    }

    // -- refusals (GTE-FR-RZKC) ---------------------------------------------

    pub(crate) fn refusing_continue(self, code: &str) -> Self {
        self.refused("continue", code, |app, run| {
            crate::graduation::continue_graduation_run(app.clone(), run.id.clone())
        })
    }

    pub(crate) fn refusing_pause(self, code: &str) -> Self {
        self.refused("pause", code, |app, run| {
            crate::graduation::pause_graduation_run(app.clone(), run.id.clone())
        })
    }

    pub(crate) fn refusing_discard(self, code: &str) -> Self {
        self.refused("discard", code, |app, run| {
            crate::graduation::discard_graduation_run(app.clone(), run.id.clone())
        })
    }

    pub(crate) fn refusing_revert(self, code: &str) -> Self {
        self.refused("revert", code, |app, run| {
            crate::graduation::revert_graduation_run(app.clone(), run.id.clone())
        })
    }

    pub(crate) fn refusing_restart(self, code: &str) -> Self {
        let stream_id = self.stream_id.clone();
        self.refused("restart", code, move |app, run| {
            crate::graduation::restart_graduation_run(
                app.clone(),
                run.id.clone(),
                stream_id,
                StandingWork::Keep,
                None,
            )
        })
    }

    pub(crate) fn refusing_answers(self, answers: &[&str], code: &str) -> Self {
        let payload = self.answers_for(answers);
        self.refused("answers", code, move |app, run| {
            crate::graduation::answer_graduation_escalation(app.clone(), run.id.clone(), payload)
        })
    }

    fn refused(
        mut self,
        what: &str,
        code: &str,
        act: impl FnOnce(&tauri::AppHandle<Runtime>, &GraduationRun) -> Result<GraduationRun, String>,
    ) -> Self {
        self.dispatch = ScriptedDispatch::new(Vec::new());
        let before = self.snapshot();
        let app = self.fx.app.clone();
        let answer = without_loop(&app, || act(&app, &self.run));
        match answer {
            Ok(_) => panic!("[{}] the {what} was accepted, but {code} was expected", self.name),
            Err(reason) => assert!(
                reason.starts_with(code),
                "[{}] the {what} was refused with {reason:?}, not {code:?}",
                self.name
            ),
        }
        assert_eq!(
            self.snapshot(),
            before,
            "[{}] the refused {what} changed nothing",
            self.name
        );
        self.run = self.fx.reload(&self.run.id);
        self
    }

    /// One answer per recorded question, in position order. Fewer answers
    /// than questions make the partial set a refusal is about.
    fn answers_for(&self, answers: &[&str]) -> Vec<GraduationEscalationAnswer> {
        let positions: Vec<u32> = self
            .run
            .escalation
            .as_ref()
            .map(|e| e.questions.iter().map(|q| q.position).collect())
            .unwrap_or_default();
        answers
            .iter()
            .enumerate()
            .map(|(index, answer)| GraduationEscalationAnswer {
                position: positions.get(index).copied().unwrap_or(index as u32 + 1),
                answer: answer.to_string(),
                summary: answer.to_string(),
            })
            .collect()
    }
}
