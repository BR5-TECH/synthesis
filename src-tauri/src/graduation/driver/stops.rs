//! Why a turn stopped, in the author's terms (`GRD-graduation.md`
//! GRD-FR-PUXO, `GXD-graduation-execution.md` GXD-FR-UZHX, GXD-FR-WJOW,
//! `GLG-graduation-loop-logging.md` GLG-FR-RLQZ).
//!
//! The executor knows how a turn ended: it timed out, it exited with a code, it
//! was terminated, or it answered in a shape nobody can read. This file keeps
//! that fact through to the run record, the detail the author reads, and the
//! structured log, so that a stopped run says what stopped it.

use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::tools::agent_exec::finish::human_duration;

/// What the executor reported about a stopped turn. Counts and codes only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StopFacts {
    pub process_outcome: Option<&'static str>,
    pub duration_ms: Option<u64>,
    pub limit_ms: Option<u64>,
    pub exit_code: Option<i32>,
}

/// One stop: the reason recorded on the run, the sentence the author reads,
/// and the facts the structured log carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stop {
    pub reason: GraduationInterruptionReason,
    pub detail: String,
    pub facts: StopFacts,
}

impl Stop {
    /// A stop with no executor facts: a pause, a shutdown, a project change, or
    /// a cancellation whose cause was not recorded.
    pub fn of_reason(reason: GraduationInterruptionReason) -> Self {
        Self {
            reason,
            detail: detail_of(reason).to_string(),
            facts: StopFacts::default(),
        }
    }

    /// GLG-FR-RLQZ: a recorded stop that won over a turn which did return. The
    /// reason and the detail are the recorded stop's, and the log keeps the
    /// facts the executor reported about the turn.
    pub fn of_reason_with(reason: GraduationInterruptionReason, facts: StopFacts) -> Self {
        Self {
            facts,
            ..Self::of_reason(reason)
        }
    }
}

/// GLG-FR-RLQZ: the facts the executor reported about a turn that returned.
pub(super) fn facts_of(execution: &AgentExecution, limit_ms: u64) -> StopFacts {
    StopFacts {
        process_outcome: Some(execution.process_outcome.as_str()),
        duration_ms: Some(execution.duration_ms),
        limit_ms: Some(limit_ms),
        exit_code: execution.exit_code,
    }
}

/// GXD-FR-UZHX / GXD-FR-WJOW: the stop a turn that ended on its own makes.
///
/// `None` for a turn that completed, which is not a stop. A turn the token
/// cancelled with no recorded cause is `retryable_failure`: the caller reads
/// the recorded cause first, so this is the fallback for that case alone.
pub(super) fn stop_of_execution(execution: &AgentExecution, limit_ms: u64) -> Option<Stop> {
    let facts = facts_of(execution, limit_ms);
    let (reason, detail) = match execution.process_outcome {
        ProcessOutcome::Completed => return None,
        ProcessOutcome::Timeout => (
            GraduationInterruptionReason::ExecutionTimeout,
            format!(
                "The agent turn ran for {} and reached the execution time limit of {}, so the application stopped it.",
                human_duration(execution.duration_ms),
                human_duration(limit_ms)
            ),
        ),
        ProcessOutcome::NonZeroExit => (
            GraduationInterruptionReason::AgentExited,
            match execution.exit_code {
                Some(code) => format!(
                    "The agent process stopped with exit code {code} before it gave an answer."
                ),
                None => "The agent process stopped before it gave an answer.".to_string(),
            },
        ),
        ProcessOutcome::Terminated => (
            GraduationInterruptionReason::AgentTerminated,
            format!(
                "The agent process was stopped from outside the application after {}.",
                human_duration(execution.duration_ms)
            ),
        ),
        ProcessOutcome::InvalidStructuredOutput => (
            GraduationInterruptionReason::UnreadableAnswer,
            "The agent finished, but its answer was not in a shape this application can read."
                .to_string(),
        ),
        ProcessOutcome::Cancelled => (
            GraduationInterruptionReason::RetryableFailure,
            detail_of(GraduationInterruptionReason::RetryableFailure).to_string(),
        ),
    };
    Some(Stop { reason, detail, facts })
}

/// GXD-FR-WJOW: a turn that could not be launched. The sentence names the kind
/// of error and nothing of its text; the full error goes to the application log.
pub(super) fn stop_of_launch_error(error: &AgentExecutionError, limit_ms: u64) -> Stop {
    let detail = match error {
        AgentExecutionError::IntegrationUnresolved(_) => {
            "The project has no agentic integration to run the turn with. Choose one in Project settings."
        }
        AgentExecutionError::UnsupportedIntegration(_) => {
            "The project's agentic integration cannot run graduation turns."
        }
        AgentExecutionError::CredentialUnavailable(_) => {
            "The credential of the project's agentic integration was not available."
        }
        AgentExecutionError::ExecutionDirectoryInvalid(_) => {
            "The run's working directory could not be used."
        }
        AgentExecutionError::RequestTooLarge(_) => {
            "The task of this turn is larger than the executor accepts."
        }
        AgentExecutionError::TaskInvalid(_) => "The task of this turn is not valid.",
        AgentExecutionError::RuntimeUnavailable => "Docker is not available on this machine.",
        AgentExecutionError::ImageUnavailable(_) => {
            "The agent's container image could not be found or pulled."
        }
        AgentExecutionError::LaunchFailed(_) => "The agent's container could not be started.",
        AgentExecutionError::VendorImageUnresolved(_) => {
            "The project names no container image for its agent. Set one in Project settings, in the Docker section."
        }
        AgentExecutionError::SessionStateUnavailable(_) => {
            "The agent's session directory could not be prepared."
        }
        AgentExecutionError::SupplementaryMountInvalid(_) => {
            "A file this turn needs could not be mounted."
        }
        AgentExecutionError::RepositoryMaskingUnavailable(_) => {
            "The repository metadata could not be hidden from the agent."
        }
    };
    Stop {
        reason: GraduationInterruptionReason::LaunchFailed,
        detail: detail.to_string(),
        facts: StopFacts {
            process_outcome: None,
            duration_ms: None,
            limit_ms: Some(limit_ms),
            exit_code: None,
        },
    }
}

/// The name of a launch error's kind, for the application log. A credential
/// error's text is not logged, because it comes from the credential path.
pub(super) fn launch_error_kind(error: &AgentExecutionError) -> &'static str {
    match error {
        AgentExecutionError::IntegrationUnresolved(_) => "integration_unresolved",
        AgentExecutionError::UnsupportedIntegration(_) => "unsupported_integration",
        AgentExecutionError::CredentialUnavailable(_) => "credential_unavailable",
        AgentExecutionError::ExecutionDirectoryInvalid(_) => "execution_directory_invalid",
        AgentExecutionError::RequestTooLarge(_) => "request_too_large",
        AgentExecutionError::TaskInvalid(_) => "task_invalid",
        AgentExecutionError::RuntimeUnavailable => "runtime_unavailable",
        AgentExecutionError::ImageUnavailable(_) => "image_unavailable",
        AgentExecutionError::LaunchFailed(_) => "launch_failed",
        AgentExecutionError::VendorImageUnresolved(_) => "vendor_image_unresolved",
        AgentExecutionError::SessionStateUnavailable(_) => "session_state_unavailable",
        AgentExecutionError::SupplementaryMountInvalid(_) => "supplementary_mount_invalid",
        AgentExecutionError::RepositoryMaskingUnavailable(_) => "repository_masking_unavailable",
    }
}

/// The full text of a launch error for the application log, or `None` where
/// the text may carry credential material.
pub(super) fn launch_error_text(error: &AgentExecutionError) -> Option<String> {
    match error {
        AgentExecutionError::CredentialUnavailable(_) => None,
        other => Some(format!("{other:?}")),
    }
}

/// What the author is told about a stop that carries no executor facts.
pub(super) fn detail_of(reason: GraduationInterruptionReason) -> &'static str {
    match reason {
        GraduationInterruptionReason::AuthorPause => "You paused this run.",
        GraduationInterruptionReason::ExecutionAbandoned => {
            "The application stopped while this run was working."
        }
        GraduationInterruptionReason::ApplicationShutdown => {
            "The application closed while this run was working."
        }
        GraduationInterruptionReason::ProjectChanged => {
            "The project changed while this run was working."
        }
        _ => "The run was stopped before the turn finished.",
    }
}

/// GXD-FR-WJOW: the detail a stop rests the run with, once the turn's work is
/// settled. A turn that could not launch ran nothing, so its detail stays the
/// one sentence for its error kind.
pub(super) fn settled_detail(stop: &Stop, committed: Option<bool>) -> String {
    if stop.reason == GraduationInterruptionReason::LaunchFailed {
        return stop.detail.clone();
    }
    format!("{} {}", stop.detail, committed_sentence(committed))
}

/// GXD-FR-WJOW: the sentence that says what became of the turn's work.
pub(super) fn committed_sentence(committed: Option<bool>) -> &'static str {
    match committed {
        Some(true) => "What the turn wrote is committed as an abandoned turn.",
        Some(false) => "The turn changed nothing, so nothing was committed.",
        None => "What the turn wrote is not committed.",
    }
}

/// GLG-FR-RLQZ: the one `run interrupted` record a stopped turn appends.
///
/// `false` means the record could not be stored, and the run already rests on
/// `log_persistence_failed` (GRD-FR-IKVE).
#[must_use]
pub(super) fn note_interrupted<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    stop: &Stop,
) -> bool {
    let level = if stop.reason.is_failure() {
        GraduationLogLevel::Error
    } else {
        GraduationLogLevel::Info
    };
    let mut event = StructuredEvent::new(GraduationLogProducer::StageTransition, "run interrupted")
        .at_level(level)
        .leaving(run.observability.current_stage)
        .with("reason", stop.reason.as_str());
    if let Some(outcome) = stop.facts.process_outcome {
        event = event.with("process_outcome", outcome);
    }
    if let Some(duration) = stop.facts.duration_ms {
        event = event.with("duration_ms", duration);
    }
    if let Some(limit) = stop.facts.limit_ms {
        event = event.with("limit_ms", limit);
    }
    if let Some(code) = stop.facts.exit_code {
        event = event.with("exit_code", code);
    }
    logs::emit(app, run, event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::agent_exec::runtime::CapturedStream;

    fn execution(outcome: ProcessOutcome, exit_code: Option<i32>, duration_ms: u64) -> AgentExecution {
        AgentExecution {
            process_outcome: outcome,
            exit_code,
            response: None,
            stdout: CapturedStream::default(),
            stderr: CapturedStream::default(),
            duration_ms,
            resume_unavailable: false,
            session: None,
            container_removed: true,
        }
    }

    const TWO_HOURS: u64 = 2 * 60 * 60 * 1000;

    // GXD-FR-UZHX / GXD-FR-WJOW: a timed-out turn names the time it ran and
    // the limit it reached, and carries both into the log facts.
    #[test]
    fn a_timeout_names_the_elapsed_time_and_the_limit() {
        let stop = stop_of_execution(
            &execution(ProcessOutcome::Timeout, None, TWO_HOURS + 2_000),
            TWO_HOURS,
        )
        .expect("a timeout is a stop");
        assert_eq!(stop.reason, GraduationInterruptionReason::ExecutionTimeout);
        assert_eq!(
            stop.detail,
            "The agent turn ran for 2 h and reached the execution time limit of 2 h, so the application stopped it."
        );
        assert_eq!(stop.facts.process_outcome, Some("timeout"));
        assert_eq!(stop.facts.duration_ms, Some(TWO_HOURS + 2_000));
        assert_eq!(stop.facts.limit_ms, Some(TWO_HOURS));
        assert_eq!(stop.facts.exit_code, None);
    }

    // GXD-FR-UZHX / GXD-FR-WJOW: a non-zero exit names its exit code.
    #[test]
    fn a_non_zero_exit_names_the_exit_code() {
        let stop = stop_of_execution(&execution(ProcessOutcome::NonZeroExit, Some(137), 5_000), TWO_HOURS)
            .expect("an exit is a stop");
        assert_eq!(stop.reason, GraduationInterruptionReason::AgentExited);
        assert!(stop.detail.contains("exit code 137"), "{}", stop.detail);
        assert_eq!(stop.facts.exit_code, Some(137));
        assert_eq!(stop.facts.process_outcome, Some("non_zero_exit"));
    }

    // GXD-FR-UZHX: an exit with no code still names the cause.
    #[test]
    fn a_non_zero_exit_without_a_code_still_says_the_process_stopped() {
        let stop = stop_of_execution(&execution(ProcessOutcome::NonZeroExit, None, 5_000), TWO_HOURS)
            .expect("an exit is a stop");
        assert_eq!(stop.reason, GraduationInterruptionReason::AgentExited);
        assert_eq!(stop.detail, "The agent process stopped before it gave an answer.");
    }

    // GXD-FR-UZHX: termination and an unreadable answer each have their own reason.
    #[test]
    fn termination_and_an_unreadable_answer_have_their_own_reasons() {
        let terminated = stop_of_execution(&execution(ProcessOutcome::Terminated, None, 90_000), TWO_HOURS)
            .expect("a termination is a stop");
        assert_eq!(terminated.reason, GraduationInterruptionReason::AgentTerminated);
        assert!(terminated.detail.contains("after 1 min 30 s"), "{}", terminated.detail);
        let unreadable = stop_of_execution(
            &execution(ProcessOutcome::InvalidStructuredOutput, Some(0), 1_000),
            TWO_HOURS,
        )
        .expect("an unreadable answer is a stop");
        assert_eq!(unreadable.reason, GraduationInterruptionReason::UnreadableAnswer);
    }

    // GRD-FR-PUXO: a cancellation with no recorded cause is the one stop that
    // stays `retryable_failure`; a completed turn is no stop at all.
    #[test]
    fn a_cancellation_without_a_cause_stays_retryable_and_a_completion_is_no_stop() {
        let cancelled = stop_of_execution(&execution(ProcessOutcome::Cancelled, None, 1_000), TWO_HOURS)
            .expect("a cancellation is a stop");
        assert_eq!(cancelled.reason, GraduationInterruptionReason::RetryableFailure);
        assert!(stop_of_execution(&execution(ProcessOutcome::Completed, Some(0), 1_000), TWO_HOURS).is_none());
    }

    // GXD-FR-WJOW: a launch error gives one sentence for its kind and none of
    // its text, and the credential error's text is not logged either.
    #[test]
    fn a_launch_error_gives_a_sentence_for_its_kind_and_none_of_its_text() {
        let error = AgentExecutionError::LaunchFailed("docker: /secret/path refused".to_string());
        let stop = stop_of_launch_error(&error, TWO_HOURS);
        assert_eq!(stop.reason, GraduationInterruptionReason::LaunchFailed);
        assert_eq!(stop.detail, "The agent's container could not be started.");
        assert!(!stop.detail.contains("/secret/path"));
        assert_eq!(launch_error_kind(&error), "launch_failed");
        assert!(launch_error_text(&error).is_some());
        let credential = AgentExecutionError::CredentialUnavailable("token abc".to_string());
        assert!(launch_error_text(&credential).is_none());
        assert!(!stop_of_launch_error(&credential, TWO_HOURS).detail.contains("abc"));
    }

    // GRD-FR-PUXO: a record written with `retryable_failure` still loads, and
    // every reason keeps the name the store and the frontend read.
    #[test]
    fn every_reason_round_trips_under_its_stored_name() {
        use GraduationInterruptionReason::*;
        let stored: crate::graduation::GraduationInterruption = serde_json::from_value(serde_json::json!({
            "reason": "retryable_failure",
            "at": "2026-10-03T22:02:36Z",
            "detail": "The run was stopped before the turn finished.",
            "streamReleased": true
        }))
        .expect("a stored record with the legacy reason loads");
        assert_eq!(stored.reason, RetryableFailure);
        for reason in [
            AuthorPause,
            ApplicationShutdown,
            ProjectChanged,
            RetryableFailure,
            LogPersistenceFailed,
            ExecutionAbandoned,
            ExecutionTimeout,
            AgentExited,
            AgentTerminated,
            UnreadableAnswer,
            LaunchFailed,
        ] {
            let value = serde_json::to_value(reason).expect("a reason serialises");
            assert_eq!(value, serde_json::json!(reason.as_str()));
            let back: GraduationInterruptionReason =
                serde_json::from_value(value).expect("a reason deserialises");
            assert_eq!(back, reason);
        }
    }

    // GRU-FR-BLSS / GLG-FR-RLQZ: only a pause, a shutdown and a project change
    // are stops the author or the application made on purpose.
    #[test]
    fn only_deliberate_stops_are_not_failures() {
        use GraduationInterruptionReason::*;
        for reason in [AuthorPause, ApplicationShutdown, ProjectChanged] {
            assert!(!reason.is_failure(), "{reason:?}");
        }
        for reason in [
            RetryableFailure,
            LogPersistenceFailed,
            ExecutionAbandoned,
            ExecutionTimeout,
            AgentExited,
            AgentTerminated,
            UnreadableAnswer,
            LaunchFailed,
        ] {
            assert!(reason.is_failure(), "{reason:?}");
        }
    }
}
