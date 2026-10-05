//! What one scripted turn does, and what it then answers (GTE-FR-BWKD,
//! GTE-FR-HNSA, GTE-FR-HWQM).

use std::sync::Mutex;

use super::super::{Answer, Turn};
use super::hooks::TurnContext;
use super::settings::Setting;
use crate::graduation::{
    GraduationInterruptionReason, GraduationLogStream, ReviewFinding, ReviewOutcome,
    ReviewSeverity, ReviewVerdict,
};
use crate::tools::agent_exec::{AgentExecutionError, ProcessOutcome};

/// GTE-FR-HWQM: something a turn does to the application while it runs.
pub(crate) type Hook = Box<dyn Fn(&TurnContext) + Send + Sync>;

/// GTE-FR-HNSA, GTE-FR-BWKD: what one scripted turn does to the execution
/// directory, and what it then answers.
pub(crate) struct TurnScript {
    pub(super) inner: Turn,
    /// GTE-FR-VNOU: a setting this turn changes while it runs, so the next
    /// dispatch of the same run reads a different value.
    pub(super) changes_setting: Option<(Setting, Option<u64>)>,
    /// GTE-FR-HWQM: what the turn does to the application before it answers,
    /// in the order the scenario named them.
    pub(super) hooks: Vec<Hook>,
}

impl TurnScript {
    fn of(answer: Answer) -> Self {
        Self {
            inner: Turn::answering(answer),
            changes_setting: None,
            hooks: Vec::new(),
        }
    }

    // -- answers -------------------------------------------------------------

    /// A work turn that reports the task finished.
    pub(crate) fn reports_success() -> Self {
        Self::of(Answer::Success)
    }

    /// GRL-FR-DNKA: a work turn that reports what it could not do. The turn
    /// completed, so the run still goes to review.
    pub(crate) fn reports_failure(message: &str) -> Self {
        Self::of(Answer::ReportedFailure(message.to_string()))
    }

    /// A review turn that lets the work through.
    pub(crate) fn answers_ready(rationale: &str) -> Self {
        Self::answers_verdict(ReviewOutcome::Ready, rationale, Vec::new())
    }

    /// GRL-FR-VIAT: a `ready` verdict that carries findings, which the loop
    /// cannot read as a verdict.
    pub(crate) fn answers_ready_with(findings: Vec<ReviewFinding>) -> Self {
        Self::answers_verdict(ReviewOutcome::Ready, "The work answers the prompt.", findings)
    }

    /// A review turn that asks for one change of the named severity.
    pub(crate) fn answers_revise(severity: ReviewSeverity, description: &str) -> Self {
        Self::answers_revise_with(vec![finding(severity, description)])
    }

    /// The same, over findings the scenario composed itself.
    pub(crate) fn answers_revise_with(findings: Vec<ReviewFinding>) -> Self {
        Self::answers_verdict(ReviewOutcome::Revise, "One thing is not delivered.", findings)
    }

    /// GRL-FR-VIAT: a `revise` verdict with no finding, which the loop cannot
    /// read as a verdict.
    pub(crate) fn answers_revise_without_findings() -> Self {
        Self::answers_verdict(ReviewOutcome::Revise, "Something is not delivered.", Vec::new())
    }

    /// GRL-FR-VIAT: a verdict whose rationale says nothing.
    pub(crate) fn answers_with_blank_rationale() -> Self {
        Self::answers_verdict(ReviewOutcome::Ready, "   ", Vec::new())
    }

    fn answers_verdict(verdict: ReviewOutcome, rationale: &str, findings: Vec<ReviewFinding>) -> Self {
        Self::of(Answer::Verdict(ReviewVerdict {
            verdict,
            rationale: rationale.to_string(),
            findings,
        }))
    }

    /// GRL-FR-TVXI: a `result` object of a shape the loop cannot read.
    pub(crate) fn answers_unreadably() -> Self {
        Self::of(Answer::RawResult(serde_json::json!({
            "verdict": "maybe",
            "notes": "nothing this application can read as a verdict",
        })))
    }

    /// GRL-FR-VBCL: the turn stops to ask the author something.
    pub(crate) fn escalates(questions: &[&str]) -> Self {
        Self::of(Answer::Escalate(
            questions.iter().map(|q| q.to_string()).collect(),
        ))
    }

    /// The same, under a reason the scenario names.
    pub(crate) fn escalates_because(reason: &str, questions: &[&str]) -> Self {
        Self::of(Answer::EscalateBecause {
            reason: reason.to_string(),
            questions: questions.iter().map(|q| q.to_string()).collect(),
        })
    }

    /// GRL-FR-ZDKP: the turn was stopped where it stood. Deterministic — the
    /// harness produces the outcome rather than waiting for a clock
    /// (GTE-FR-RSQD).
    pub(crate) fn is_cancelled() -> Self {
        Self::of(Answer::Process(ProcessOutcome::Cancelled))
    }

    /// GXD-FR-XEUX: the turn ran out of the time its execution controls
    /// allowed. Produced by the harness, not waited for.
    pub(crate) fn times_out() -> Self {
        Self::of(Answer::Process(ProcessOutcome::Timeout))
    }

    /// GXD-FR-XEUX: the agent's process ended with a non-zero status.
    pub(crate) fn exits_nonzero() -> Self {
        Self::of(Answer::Process(ProcessOutcome::NonZeroExit))
    }

    /// GXD-FR-XEUX: the agent answered in a shape the protocol does not hold.
    pub(crate) fn answers_malformed() -> Self {
        Self::of(Answer::Process(ProcessOutcome::InvalidStructuredOutput))
    }

    /// GXD-FR-XEUX: the agent's process was ended from outside.
    pub(crate) fn is_terminated() -> Self {
        Self::of(Answer::Process(ProcessOutcome::Terminated))
    }

    /// GXD-FR-XEUX: nothing was launched at all.
    pub(crate) fn cannot_launch(error: AgentExecutionError) -> Self {
        Self::of(Answer::Unlaunchable(error))
    }

    /// GXD-FR-LBYG: an envelope claiming a change set of its own, which the
    /// loop must not read as evidence about the filesystem.
    pub(crate) fn claims_paths(paths: &[&str]) -> Self {
        Self::of(Answer::ClaimingPaths(
            paths.iter().map(|p| p.to_string()).collect(),
        ))
    }

    // -- effects -------------------------------------------------------------

    /// GTE-FR-HNSA: a file this turn writes into its execution directory.
    pub(crate) fn writes(mut self, path: &str, content: &str) -> Self {
        self.inner = self.inner.writing(path, content);
        self
    }

    /// GTE-FR-HNSA: a file this turn removes.
    pub(crate) fn deletes(mut self, path: &str) -> Self {
        self.inner = self.inner.deleting(path);
        self
    }

    /// GTE-FR-HNSA: a whole directory this turn removes, as a turn that
    /// reorganizes a module does.
    pub(crate) fn removes_dir(mut self, path: &str) -> Self {
        self.inner = self.inner.removing_dir(path);
        self
    }

    /// GTE-FR-HNSA: a symbolic link this turn installs, as a project's own
    /// dependency install writes.
    #[cfg(unix)]
    pub(crate) fn links(mut self, path: &str, target: &str) -> Self {
        self.inner = self.inner.linking(path, target);
        self
    }

    /// The turn answers with a vendor session the next turn can resume.
    pub(crate) fn leaves_a_session(mut self) -> Self {
        self.inner = self.inner.with_session();
        self
    }

    /// The turn answers as scripted even where the run was stopped while it
    /// ran, as an agent that finished at that same moment does.
    pub(crate) fn finishing_anyway(mut self) -> Self {
        self.inner = self.inner.ignoring_cancellation();
        self
    }

    /// GTE-FR-VNOU: change a project setting while this turn runs, so the next
    /// dispatch of the same run reads the new value.
    pub(crate) fn changing(mut self, setting: Setting, value: u64) -> Self {
        self.changes_setting = Some((setting, Some(value)));
        self
    }

    // -- hooks ---------------------------------------------------------------

    /// GTE-FR-HWQM: do something to the application while this turn runs.
    pub(crate) fn then(mut self, hook: impl Fn(&TurnContext) + Send + Sync + 'static) -> Self {
        self.hooks.push(Box::new(hook));
        self
    }

    /// GRD-FR-MDQZ: the author pauses the run while this turn runs.
    pub(crate) fn pausing_the_run(self) -> Self {
        self.then(|ctx| ctx.pause())
    }

    /// GRD-FR-EWTN: the author discards the run while this turn runs.
    pub(crate) fn discarding_the_run(self) -> Self {
        self.then(|ctx| ctx.discard())
    }

    /// GRD-FR-TWMA: the application stops every running turn.
    pub(crate) fn stopping_the_application(self, reason: GraduationInterruptionReason) -> Self {
        self.then(move |ctx| ctx.stop_application(reason))
    }

    /// GRD-FR-VLFO: the author starts another run on this stream while this
    /// turn runs.
    pub(crate) fn enqueuing_a_run_beside(self, label: &'static str, prompt: &str) -> Self {
        let prompt = prompt.to_string();
        self.then(move |ctx| ctx.enqueue_beside(label, &prompt))
    }

    /// GRB-FR-TXVL: the author stops the update this semantic turn belongs to.
    pub(crate) fn cancelling_the_update(self) -> Self {
        self.then(|ctx| ctx.cancel_update())
    }

    /// GTC-FR-19: another write takes the stream's index while this turn runs.
    pub(crate) fn holding_the_index(self) -> Self {
        self.then(|ctx| ctx.hold_index())
    }

    /// GRD-FR-IKVE: a log stream of the run stops being writable.
    pub(crate) fn losing_the_log(self, stream: GraduationLogStream) -> Self {
        self.then(move |ctx| ctx.lose_log(stream))
    }

    /// GRD-FR-XVUD: the process driving the run is gone, and the application
    /// that launches next reads the project's queue.
    pub(crate) fn abandoned_by_a_relaunch(self) -> Self {
        self.then(|ctx| ctx.relaunch())
    }

    /// GRD-FR-BNTC: a run the scenario enqueued is driven while this turn
    /// runs, under a script of its own.
    pub(crate) fn starting_another_run(self, label: &'static str, scripts: Vec<TurnScript>) -> Self {
        let held = Mutex::new(Some(scripts));
        self.then(move |ctx| {
            let scripts = held.lock().unwrap().take().unwrap_or_default();
            ctx.drive_other(label, scripts);
        })
    }
}

/// A review finding at the named severity.
pub(crate) fn finding(severity: ReviewSeverity, description: &str) -> ReviewFinding {
    ReviewFinding {
        severity,
        description: description.to_string(),
        affected_files: vec!["src/panel.ts".to_string()],
        correction: "Write the empty state.".to_string(),
    }
}
