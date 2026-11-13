//! What one scripted turn does and answers, for the dispatch seam with the
//! execution agent removed (GXD-FR-DRFF).

use super::*;

/// What one scripted turn answers with.
pub(super) enum Answer {
    /// The agent reported the work finished.
    Success,
    /// A review turn's verdict.
    Verdict(ReviewVerdict),
    /// A `result` object of any shape, for the verdicts the loop refuses.
    RawResult(serde_json::Value),
    /// The agent stopped to ask the author something.
    Escalate(Vec<String>),
    /// OPU-FR-YBLE: an escalation of the kind `plan_update`.
    PlanUpdate(crate::tools::agent_exec::protocol::AgentEscalation),
    /// The same, under a reason the test names, so an escalation outside the
    /// rules of ESU-FR-19 can be composed.
    EscalateBecause { reason: String, questions: Vec<String> },
    /// GXD-FR-LBYG: an envelope that claims a change set of its own in its
    /// summary, so a loop that believed one would be caught.
    ClaimingPaths(Vec<String>),
    /// GRL-FR-DNKA: the agent reported it could not do the work, and said what
    /// stopped it.
    ReportedFailure(String),
    /// GRL-FR-DNKA: the agent reported a failure whose message says nothing, so
    /// its summary is the only account there is.
    ReportedFailureWithoutMessage(String),
    /// GRL-FR-DNKA: the agent reported the work finished and said something
    /// about it in its summary — the case that carries an account the review
    /// most needs, because a turn that left part of the work undone reports
    /// success and says the rest here.
    SucceedingWith(String),
    /// The process did not complete.
    Process(ProcessOutcome),
    /// GRL-FR-QKDC: the turn ended, and its answer was refused for this reason.
    Refused(crate::tools::agent_exec::protocol::EnvelopeInvalid),
    /// GRL-FR-PTCE: the vendor did not resume the session it was asked to.
    ResumeUnavailable,
    /// GRL-FR-PTCE: the vendor started a new session in place of the one it
    /// was asked to resume, and reported success there.
    FromAFreshSession,
    /// Nothing was launched at all.
    Unlaunchable(AgentExecutionError),
}

/// One scripted turn: what it writes, what it then answers.
pub(super) struct Turn {
    pub(super) writes: Vec<(String, String)>,
    /// Symbolic links the turn installs, as a dependency install does.
    pub(super) links: Vec<(String, String)>,
    pub(super) deletes: Vec<String>,
    pub(super) removed_dirs: Vec<String>,
    pub(super) answer: Answer,
    /// Something to do inside the turn — cancel the run, move a branch.
    pub(super) side_effect: Option<Box<dyn Fn() + Send + Sync>>,
    /// Something to do once the verdict of this review is durable.
    pub(super) after_verdict: Option<Box<dyn Fn() + Send + Sync>>,
    /// The id of the resumable vendor session the answer carries, if any.
    pub(super) session: Option<String>,
    /// Whether the turn answers as scripted even where the run was stopped
    /// while it ran, as an agent that finished at the same moment does.
    pub(super) ignores_cancellation: bool,
    /// GRD-FR-UTTX: the execution time the executor reports for a turn that
    /// completed, so a scenario can spend the run's total time.
    pub(super) duration_ms: u64,
}

impl Turn {
    pub(super) fn answering(answer: Answer) -> Self {
        Self {
            writes: Vec::new(),
            links: Vec::new(),
            deletes: Vec::new(),
            removed_dirs: Vec::new(),
            answer,
            side_effect: None,
            after_verdict: None,
            session: None,
            ignores_cancellation: false,
            duration_ms: 0,
        }
    }

    pub(super) fn ignoring_cancellation(mut self) -> Self {
        self.ignores_cancellation = true;
        self
    }

    /// A work turn that reports the work finished.
    pub(super) fn work() -> Self {
        Self::answering(Answer::Success)
    }

    /// A work turn that reports the work finished and says something about it.
    pub(super) fn work_saying(summary: &str) -> Self {
        Self::answering(Answer::SucceedingWith(summary.to_string()))
    }

    /// A review turn that lets the work through.
    pub(super) fn ready() -> Self {
        Self::answering(Answer::Verdict(ReviewVerdict {
            verdict: ReviewOutcome::Ready,
            rationale: "The work answers the prompt.".to_string(),
            findings: Vec::new(),
        }))
    }

    /// A review turn that asks for one change of the named severity.
    pub(super) fn revise(severity: ReviewSeverity, description: &str) -> Self {
        Self::answering(Answer::Verdict(ReviewVerdict {
            verdict: ReviewOutcome::Revise,
            rationale: "One thing is not delivered.".to_string(),
            findings: vec![ReviewFinding {
                kind: None,
                severity,
                description: description.to_string(),
                affected_files: vec!["src/panel.ts".to_string()],
                correction: "Write the empty state.".to_string(),
            }],
        }))
    }

    pub(super) fn writing(mut self, path: &str, content: &str) -> Self {
        self.writes.push((path.to_string(), content.to_string()));
        self
    }

    pub(super) fn deleting(mut self, path: &str) -> Self {
        self.deletes.push(path.to_string());
        self
    }

    /// A symbolic link the turn installs, relative to the directory it runs in.
    ///
    /// This is what a project's own dependency install writes, and what the
    /// application must still be able to reclaim afterwards (FSA-FR-ZUCF).
    #[cfg(unix)]
    pub(super) fn linking(mut self, path: &str, target: &str) -> Self {
        self.links.push((path.to_string(), target.to_string()));
        self
    }

    /// Remove a whole directory, as a turn that reorganizes a module does.
    pub(super) fn removing_dir(mut self, path: &str) -> Self {
        self.removed_dirs.push(path.to_string());
        self
    }

    /// GRD-FR-UTTX: the turn completes after `ms` of execution time.
    pub(super) fn taking(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }

    /// The answer carries the resumable vendor session `id`.
    pub(super) fn with_session(mut self, id: &str) -> Self {
        self.session = Some(id.to_string());
        self
    }

    pub(super) fn doing(mut self, side_effect: impl Fn() + Send + Sync + 'static) -> Self {
        self.side_effect = Some(Box::new(side_effect));
        self
    }

    pub(super) fn after_the_verdict(mut self, act: impl Fn() + Send + Sync + 'static) -> Self {
        self.after_verdict = Some(Box::new(act));
        self
    }
}
