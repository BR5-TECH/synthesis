//! What a graduation run is, and what it records (`GRD-graduation.md`).

use super::*;

// ---------------------------------------------------------------------------
// The state machine (GRD-FR-QJHM)
// ---------------------------------------------------------------------------

/// GRD-FR-QJHM: the nine states a run stands in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationRunState {
    /// Waiting for its stream.
    #[default]
    Queued,
    /// A work turn holds the stream.
    Working,
    /// A review turn is judging what the work turn wrote.
    Reviewing,
    /// An escalation, or a review that did not settle, waits on the author.
    AwaitingAuthor,
    /// A condition the author clears.
    Blocked,
    /// Stopped, or resting on a retryable failure.
    Interrupted,
    /// Terminal: the work is committed on the stream.
    Completed,
    /// Terminal.
    Discarded,
    /// Terminal.
    Failed,
}

impl GraduationRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Working => "working",
            Self::Reviewing => "reviewing",
            Self::AwaitingAuthor => "awaiting_author",
            Self::Blocked => "blocked",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Discarded => "discarded",
            Self::Failed => "failed",
        }
    }

    /// GRD-FR-QJHM: the three states a run never leaves.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Discarded | Self::Failed)
    }

    /// GRD-FR-BNTC: a run holds its stream while it is doing agent work or
    /// resting on a condition inside one. Every other state holds nothing.
    pub fn holds_stream(self) -> bool {
        matches!(self, Self::Working | Self::Reviewing | Self::Blocked)
    }

    /// GRD-FR-DXWL: a draft a non-terminal run holds is locked.
    pub fn locks_draft(self) -> bool {
        !self.is_terminal()
    }

    /// GRD-FR-EGWS: the states a stream's queue starts a run from.
    pub fn waits_in_queue(self) -> bool {
        matches!(self, Self::Queued)
    }
}

// ---------------------------------------------------------------------------
// The captured prompt (GRD-FR-FTBQ)
// ---------------------------------------------------------------------------

/// GRD-FR-FTBQ: the prompt a run answers, as it stood when the author judged it
/// finished.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedGraduationInput {
    pub draft_id: String,
    /// The draft's name at capture.
    pub draft_name: String,
    /// The whole text of the draft's one prompt at capture.
    pub prompt: String,
    /// sha256 of the captured bytes.
    pub prompt_checksum: String,
    /// RFC 3339 UTC.
    pub captured_at: String,
}

// ---------------------------------------------------------------------------
// Escalation (GXD-FR-HGSU)
// ---------------------------------------------------------------------------

/// One response an agent proposed for a question it asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationProposedResponse {
    /// The value submitted when this response is chosen.
    pub answer: String,
    /// One sentence, at most 5 words; submitted with the answer.
    pub summary: String,
    /// Display-only; at most 2 sentences and 12 words in total.
    pub description: String,
}

/// GXD-FR-HGSU: one recorded question, at the **stable position** the
/// application assigned it.
///
/// The position is what every later act names. A question's text is never an
/// identifier: two questions may read alike, and an answer matched by text
/// would attach to whichever question happened to match first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationEscalationQuestion {
    /// 1-based, stable, assigned in the order the questions arrived.
    pub position: u32,
    pub question: String,
    /// 0 to 3; empty where no fixed response suits.
    #[serde(default)]
    pub options: Vec<GraduationProposedResponse>,
}

/// GXD-FR-XPUR: which turn an answered escalation is delivered into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationEscalationOrigin {
    /// A work turn asked, and the answers continue that same turn.
    Work,
    /// A review turn asked, and the answers go to a fresh review turn.
    Review,
    /// A semantic merge turn asked, and the answers go to a fresh merge turn of
    /// the same attempt. Its questions reach the author unjudged: what such a
    /// turn asks is which of two contradictory requirements stands, and the
    /// contradiction is itself the evidence that nothing settles it.
    SemanticMerge,
}

impl GraduationEscalationOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Review => "review",
            Self::SemanticMerge => "semantic_merge",
        }
    }
}

/// One answer the author gave, at the position of the question it answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationEscalationAnswer {
    pub position: u32,
    pub answer: String,
    pub summary: String,
}

/// GXD-FR-HGSU: what a run in `awaiting_author` is waiting for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationEscalation {
    /// The agent's own reason, whole.
    pub reason: String,
    /// One to eight, in the order they arrived.
    pub questions: Vec<GraduationEscalationQuestion>,
    /// Which turn the answers are delivered into.
    pub origin: GraduationEscalationOrigin,
    /// RFC 3339 UTC.
    pub raised_at: String,
    /// The vendor session to resume, where the origin's turn can be resumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<GraduationResumeRef>,
}

/// GXD-FR-XPUR: the vendor session an answered escalation continues.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationResumeRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

// ---------------------------------------------------------------------------
// The review verdict (`../ai/GRL-graduation-loop.md` GRL-FR-VIAT)
// ---------------------------------------------------------------------------

/// GRL-FR-VIAT: how serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Critical,
    Major,
    Minor,
}

impl ReviewSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }
}

/// GRL-FR-VIAT: one problem, stated once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub description: String,
    /// Project-relative; may name a path that does not exist yet.
    #[serde(default)]
    pub affected_files: Vec<String>,
    /// Directly actionable by the next work turn.
    pub correction: String,
}

/// GRL-FR-VIAT: what a review turn answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewVerdict {
    /// `ready` or `revise`, and nothing else.
    pub verdict: ReviewOutcome,
    /// Never blank.
    pub rationale: String,
    /// Empty for `ready`; at least one for `revise`.
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
}

/// GRL-FR-VIAT: the two verdicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    Ready,
    Revise,
}

// ---------------------------------------------------------------------------
// Stopping (GRD-FR-XVUD, GRD-FR-IKVE)
// ---------------------------------------------------------------------------

/// GRD-FR-XVUD: why a run stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationInterruptionReason {
    /// The author's own stop, through Pause or through a discard.
    AuthorPause,
    ApplicationShutdown,
    ProjectChanged,
    /// GRD-FR-PUXO: a stop whose cause was not recorded. Records written
    /// before the causes below existed hold it too, so it stays readable.
    RetryableFailure,
    /// A required log stream could not be written (GRD-FR-IKVE).
    LogPersistenceFailed,
    /// GRD-FR-XVUD: the record said the run was working and no loop was behind
    /// it, so the application stopped without stopping the run.
    ExecutionAbandoned,
    /// GXD-FR-UZHX: the turn reached the project's execution timeout.
    ExecutionTimeout,
    /// GXD-FR-UZHX: the agent process exited with a non-zero code.
    AgentExited,
    /// GXD-FR-UZHX: the agent process was terminated from outside.
    AgentTerminated,
    /// GXD-FR-UZHX: the agent finished, but its answer could not be read.
    UnreadableAnswer,
    /// GXD-FR-UZHX: the turn could not be launched.
    LaunchFailed,
}

impl GraduationInterruptionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthorPause => "author_pause",
            Self::ApplicationShutdown => "application_shutdown",
            Self::ProjectChanged => "project_changed",
            Self::RetryableFailure => "retryable_failure",
            Self::LogPersistenceFailed => "log_persistence_failed",
            Self::ExecutionAbandoned => "execution_abandoned",
            Self::ExecutionTimeout => "execution_timeout",
            Self::AgentExited => "agent_exited",
            Self::AgentTerminated => "agent_terminated",
            Self::UnreadableAnswer => "unreadable_answer",
            Self::LaunchFailed => "launch_failed",
        }
    }

    /// GRU-FR-BLSS / GLG-FR-RLQZ: whether the stop is a failure, as opposed to
    /// a stop the author or the application made on purpose.
    pub fn is_failure(self) -> bool {
        !matches!(
            self,
            Self::AuthorPause | Self::ApplicationShutdown | Self::ProjectChanged
        )
    }
}

/// GRD-FR-XVUD: why an interrupted run stopped and what it waits for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationInterruption {
    pub reason: GraduationInterruptionReason,
    /// RFC 3339 UTC.
    pub at: String,
    /// A short, author-facing explanation. Never a credential and never file
    /// content.
    #[serde(default)]
    pub detail: String,
    /// GRD-FR-MDQZ: the run gave its stream back. True for an author pause.
    #[serde(default)]
    pub stream_released: bool,
    /// GRD-FR-CYIB: when the author resumed a paused run; null otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_requested_at: Option<String>,
}

/// A run that ended on a failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

/// GRD-FR-QJHM: what blocks a run, and the act that clears it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationBlocker {
    pub code: String,
    pub message: String,
    /// What the author does about it, in their own terms.
    pub clears_by: String,
    /// GRU-FR-LBPR: which consecutive attempt this code blocked the run on,
    /// counted from one. A record written before this field existed reads as
    /// zero, which a surface renders as no attempt at all.
    #[serde(default)]
    pub attempt: u32,
}

// ---------------------------------------------------------------------------
// The checkpoint (GXD-FR-GMDI)
// ---------------------------------------------------------------------------

/// GXD-FR-GMDI: what a stopped run is continued from.
///
/// Normalized and carrying no secret: no credential, no session of a turn that
/// is not resumable, and no byte of a project file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationCheckpoint {
    /// GXD-FR-GMDI: the pass the run stands at.
    ///
    /// **This is the one place the pass is counted.** It advances at exactly one
    /// moment — a review that answers `revise` and starts another pass — so a
    /// run that resumed after an interruption, or that the author answered an
    /// escalation for, continues the pass it was in rather than spending the
    /// one it has left. A record written before this field existed reads as
    /// zero, which [`GraduationRun::pass`] reads as the first pass.
    #[serde(default)]
    pub pass: u32,
    /// What the working copy holds against the base commit.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// What the repository's own ignore rules keep out of that change set.
    ///
    /// Most of it is build output and is exactly where it should be. The part
    /// that matters is authored work: a file the turn wrote that a rule hides
    /// is reviewed by nobody and committed by nothing, so a module whose
    /// submodules are hidden this way lands broken. An ignored directory reads
    /// as one entry with a trailing separator.
    #[serde(default)]
    pub hidden_paths: Vec<String>,
    /// How many more there were than the bound carries.
    #[serde(default)]
    pub hidden_paths_omitted: u32,
    /// GRL-FR-GQAB: what the next work turn is to act on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_instruction: Option<String>,
    /// GRL-FR-DNKA: the work turn's own account of what it did and did not do.
    ///
    /// Held for **every** work turn that completed, whichever outcome it
    /// reported: a turn that reported a failure carries that failure's message,
    /// and a turn that reported success carries its summary on the same terms.
    /// An agent-reported failure is material rather than an outcome, so the run
    /// goes to review like any other either way.
    ///
    /// The account the review most needs is the one a **successful** turn gives:
    /// a turn that wrote real work and knows it left some of what was asked
    /// undone reports success, that being the only outcome that does not
    /// misdescribe what it wrote, and says the rest here.
    ///
    /// `default` so a checkpoint written before this field existed reads as no
    /// account rather than failing to load, which is the safe reading — a run
    /// resumed across the change is reviewed on the change set alone, exactly as
    /// it would have been.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_account: Option<String>,
    /// Answers waiting to be delivered into the turn that asked.
    #[serde(default)]
    pub pending_escalation_answers: Vec<GraduationEscalationAnswer>,
    /// GRL-FR-TVXI: consecutive unreadable verdicts.
    #[serde(default)]
    pub verdict_refusals: u32,
    /// GXD-FR-GMDI: the pass this run's current budget window starts from.
    ///
    /// One for a new run, and a record written before this field existed reads
    /// as zero, which [`GraduationCheckpoint::floor`] reads as the first pass.
    /// Continue after an exhausted budget moves the floor to the next pass, so
    /// the run gets a whole budget again and repeats no completed pass
    /// (GRL-FR-XBUE).
    #[serde(default)]
    pub pass_floor: u32,
    /// GXD-FR-PWYD: the highest pass the current window allows.
    ///
    /// Recomputed at every dispatch from the settings then held (GRL-FR-KWNP),
    /// so it is what a surface renders as the run's pass bound. Zero before the
    /// first dispatch, which is no bound yet rather than a bound of none.
    #[serde(default)]
    pub pass_limit: u32,
    /// GXD-FR-TJRV: the phase the run resumes at, `work` or `review`.
    ///
    /// A run blocked in the review resumes there rather than at the start of
    /// the pass: the work did not change while the run was blocked, so a work
    /// turn would spend a container to decide again what nobody reported a
    /// problem with. A record written before this field existed reads as
    /// `None`, which resumes at the work turn — the behaviour it was written
    /// under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_part: Option<String>,
    /// GRL-FR-QZFB: the code of the blocker last raised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_code: Option<String>,
    /// GRL-FR-QZFB: how many times in a row `blocked_code` was raised.
    #[serde(default)]
    pub block_attempts: u32,
    /// The vendor session a resumed turn continues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<GraduationResumeRef>,
}

// ---------------------------------------------------------------------------
// The run (GRD contract surface)
// ---------------------------------------------------------------------------

/// GRD-FR-BSNI: where a direct run works, pinned when the author confirmed.
///
/// Recorded once and never changed. The worktree is the author's own, so the
/// record names it by the path it stood at and the branch it held.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectTarget {
    /// Absolute path of the pinned worktree.
    pub worktree_path: String,
    /// The directory's basename.
    pub worktree_name: String,
    /// The branch the worktree held at confirmation.
    pub branch: String,
}

/// GRD-FR-XRDY: why a queued direct run is not dispatched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetHold {
    /// `target_branch_changed`.
    pub code: String,
    /// The branch the run pinned.
    pub expected_branch: String,
    /// The branch the worktree holds now; absent where it is detached or gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_branch: Option<String>,
}

/// GRD-FR-XRDY: the one code a target hold carries.
pub const HOLD_TARGET_BRANCH_CHANGED: &str = "target_branch_changed";

/// GRD-FR-ZVNO: the prefix of the queue key of an ordinary worktree's queue.
pub const WORKTREE_QUEUE_PREFIX: &str = "worktree:";

/// GRD-FR-ZVNO: the queue key of the queue an ordinary worktree owns.
pub fn worktree_queue_key(worktree_path: &str) -> String {
    format!("{WORKTREE_QUEUE_PREFIX}{worktree_path}")
}

/// Whether a queue key names an ordinary worktree rather than a stream.
pub fn is_worktree_queue_key(key: &str) -> bool {
    key.starts_with(WORKTREE_QUEUE_PREFIX)
}

/// One graduation run, whole.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationRun {
    pub id: String,
    /// GRD-FR-PZAK: the stream this run runs in, recorded once and never
    /// changed. It outlives the stream.
    ///
    /// Empty for a direct run on an ordinary worktree. A direct run on a
    /// stream's working copy names that stream (GRD-FR-PZAK).
    #[serde(default)]
    pub stream_id: String,
    #[serde(default)]
    pub stream_name: String,
    /// GRD-FR-BSNI: where a direct run works. Absent on a stream run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_target: Option<DirectTarget>,
    /// GRD-FR-XRDY: why dispatch is withheld from this queued direct run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_hold: Option<TargetHold>,
    /// The project whose run order holds it.
    #[serde(default)]
    pub project_key: String,
    pub state: GraduationRunState,
    /// GRD-FR-HQPD: what this run does with work standing uncommitted in its
    /// stream when it is dispatched. Chosen when the run was enqueued.
    #[serde(default)]
    pub standing_work: StandingWork,
    /// GRD-FR-RJFC: the message a commit the choice makes takes. A run that
    /// carries none commits under its own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing_work_message: Option<String>,
    /// GRD-FR-KDWA: what the standing-work step did. Written at the first
    /// dispatch, and absent before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing_work_outcome: Option<StandingWorkOutcome>,
    pub input: CapturedGraduationInput,
    /// GRD-FR-YBUM: the stream revision this run is measured from. Absent until
    /// the first turn is dispatched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// The revisions this run created, in order.
    #[serde(default)]
    pub commits: Vec<String>,
    /// GRD-FR-TKUR: whether the stream's queue may start it.
    #[serde(default = "yes")]
    pub auto_start: bool,
    /// GRD-FR-JOFE: where the author filed it. Independent of `state`.
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// GRL-FR-ARPX: the passes spent against the bound.
    #[serde(default)]
    pub work_turns: u32,
    #[serde(default)]
    pub review_turns: u32,
    #[serde(default)]
    pub checkpoint: GraduationCheckpoint,
    /// GRD-FR-OSCG / GRS-FR-CGSP: how far each of the run's two log streams
    /// stands, and whether the last write succeeded. It holds no log payload.
    #[serde(default)]
    pub logs: crate::graduation::logs::GraduationLogIndexes,
    #[serde(default)]
    pub observability: observability::GraduationObservability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation: Option<GraduationEscalation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<GraduationBlocker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption: Option<GraduationInterruption>,
    /// GRD-FR-ZAMI: the discarded run this one was restarted from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restarted_from_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<GraduationFailure>,
    /// GRD-FR-MRNQ: what a merge run pins and carries. Present on a merge run
    /// alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<GraduationMergeData>,
    pub created_at: String,
    pub updated_at: String,
}

fn yes() -> bool {
    true
}

/// GRD-FR-ZVNO: the queue key for a stream id and a direct target.
pub fn queue_key_of(stream_id: &str, direct: Option<&DirectTarget>) -> String {
    match direct {
        Some(target) if stream_id.is_empty() => worktree_queue_key(&target.worktree_path),
        _ => stream_id.to_string(),
    }
}

/// GRD-FR-HQPD: what a run does with work standing uncommitted in its stream
/// when its turn comes.
///
/// The author chooses it when the run is enqueued, and the run applies it at
/// the moment it is dispatched. A record written before the choice existed
/// reads as `Commit`, which is what such a run was enqueued under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandingWork {
    /// The run works on top of what stands in the stream.
    Keep,
    /// Commit it first, under a message naming the author as its writer.
    #[default]
    Commit,
    /// Commit it first, then push the stream branch.
    CommitAndPush,
}

impl StandingWork {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Commit => "commit",
            Self::CommitAndPush => "commit_and_push",
        }
    }

    /// Whether this choice commits what stands in the stream.
    pub fn commits(self) -> bool {
        matches!(self, Self::Commit | Self::CommitAndPush)
    }

    /// Whether this choice pushes the stream branch.
    pub fn pushes(self) -> bool {
        matches!(self, Self::CommitAndPush)
    }
}

/// GRD-FR-KDWA: what the standing-work step of one run did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingWorkOutcome {
    /// The revision the standing work was committed as, or none where nothing
    /// stood in the stream or the choice committed nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Whether the remote took the stream branch. None where the choice asked
    /// for no push.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed: Option<bool>,
    /// GRD-FR-PXVJ: why the remote did not take it. The run is not stopped by
    /// it, so this is a report rather than a blocker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_failure: Option<StandingWorkPushFailure>,
}

/// GRD-FR-KDWA: the typed reason a push did not land.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingWorkPushFailure {
    /// The typed cause, as the push reports it. It carries no credential.
    pub code: String,
}

impl GraduationRun {
    /// A run in the state a fresh enqueue leaves it in.
    ///
    /// The one place a whole record is composed for a test, so a field added
    /// here reaches every fixture rather than each one restating the shape.
    #[cfg(test)]
    pub fn new_for_test(id: &str, draft_id: &str, updated_at: &str) -> Self {
        Self {
            id: id.to_string(),
            stream_id: "w1".to_string(),
            stream_name: "a stream".to_string(),
            direct_target: None,
            target_hold: None,
            project_key: "p".to_string(),
            state: GraduationRunState::Queued,
            standing_work: StandingWork::default(),
            standing_work_message: None,
            standing_work_outcome: None,
            input: CapturedGraduationInput {
                draft_id: draft_id.to_string(),
                draft_name: draft_id.to_string(),
                prompt: "a prompt".to_string(),
                prompt_checksum: "0".repeat(64),
                captured_at: updated_at.to_string(),
            },
            base_commit: None,
            commits: Vec::new(),
            auto_start: true,
            archived: false,
            archived_at: None,
            work_turns: 0,
            review_turns: 0,
            logs: crate::graduation::logs::GraduationLogIndexes::default(),
            checkpoint: GraduationCheckpoint::default(),
            observability: observability::GraduationObservability::default(),
            escalation: None,
            blocker: None,
            interruption: None,
            restarted_from_run_id: None,
            failure: None,
            merge: None,
            created_at: updated_at.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    /// GRD-FR-BNTC: whether this run is holding its stream right now.
    pub fn holds_stream(&self) -> bool {
        self.state.holds_stream()
    }

    /// GRD-FR-BSNI: whether this run works directly in a worktree.
    pub fn is_direct(&self) -> bool {
        self.direct_target.is_some()
    }

    /// GRD-FR-ZVNO: the key of the queue this run waits in and the lock it
    /// takes: the stream's id where it has a stream, else its worktree's.
    pub fn queue_key(&self) -> String {
        queue_key_of(&self.stream_id, self.direct_target.as_ref())
    }

    /// GRD-FR-OYPY: whether this is a direct run that was dispatched and has
    /// not ended.
    ///
    /// A run that holds its stream is dispatched even before its base commit
    /// is written, so the window between the claim and the base is covered.
    pub fn is_dispatched_direct(&self) -> bool {
        self.is_direct()
            && !self.state.is_terminal()
            && (self.base_commit.is_some() || self.holds_stream())
    }

    /// What the run's provenance names: the stream, or the pinned worktree.
    pub fn target_label(&self) -> String {
        match &self.direct_target {
            Some(target) => target.worktree_name.clone(),
            None => self.stream_name.clone(),
        }
    }

    /// GXD-FR-GMDI: the pass this run stands at, counted from one.
    pub fn pass(&self) -> u32 {
        self.checkpoint.pass.max(1)
    }

    /// GXD-FR-GMDI: the pass this run's budget window starts from, counted
    /// from one.
    pub fn pass_floor(&self) -> u32 {
        self.checkpoint.pass_floor.max(1)
    }

    /// GRL-FR-XBUE: how many passes the run's current budget window grants.
    ///
    /// Zero before the first dispatch, which is no window yet.
    pub fn pass_budget(&self) -> u32 {
        self.checkpoint
            .pass_limit
            .saturating_sub(self.pass_floor())
            .saturating_add(if self.checkpoint.pass_limit > 0 { 1 } else { 0 })
    }

    /// GRL-FR-XBUE: whether the run has spent the window it stands in, so that
    /// no further pass may start.
    ///
    /// A run before its first dispatch has no window yet, and has spent
    /// nothing.
    /// `limit` is the window's highest pass. A run whose checkpoint holds none
    /// yet — a record written before the window existed — is measured against
    /// the limit the caller computed from the settings, so the author's first
    /// Continue resets its budget rather than spending a pass repeating work.
    pub fn pass_budget_spent(&self, limit: u32) -> bool {
        let limit = if self.checkpoint.pass_limit > 0 {
            self.checkpoint.pass_limit
        } else {
            limit
        };
        limit > 0 && self.pass() >= limit
    }

    /// Whether the pass the checkpoint holds stands past the window entirely.
    ///
    /// A run only reaches this by being resumed under a budget smaller than the
    /// one its earlier passes ran under, which is the author's own change and
    /// rests the run rather than starting a pass they did not grant.
    pub fn pass_beyond_window(&self) -> bool {
        self.checkpoint.pass_limit > 0 && self.pass() > self.checkpoint.pass_limit
    }

    /// GRD-FR-EGWS: whether a stream's queue may start this run.
    pub fn is_eligible(&self) -> bool {
        self.auto_start && self.state.waits_in_queue()
    }

    /// GRD-FR-DXWL: whether this run locks its draft.
    pub fn locks_draft(&self) -> bool {
        self.state.locks_draft()
    }
}

/// GRD-FR-LGDV / GRD-FR-VLFO: the project's whole run order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationQueue {
    pub project_key: String,
    /// Every run the project has made, in the project's run order.
    pub runs: Vec<GraduationRun>,
}

/// What the Drafts panel is told about a draft's run (`DRS-FR-18`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftGraduation {
    pub run_id: String,
    pub state: GraduationRunState,
    /// DRS-FR-19: a non-terminal run holds the draft.
    pub locked: bool,
    /// DRS-FR-KQTW: a run of this draft has committed its work onto a stream
    /// and is not discarded.
    pub graduated: bool,
}
