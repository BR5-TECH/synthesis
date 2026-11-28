//! The helpers of a launch that read no launch state: the check of the
//! execution directory (EAC-FR-06), the reading of a run's outcome, the
//! removal of its container (EAC-FR-26), and the result of a resume that
//! cannot continue its session (EAC-FR-30).
//!
//! Split out of `launch.rs` for size alone.

use std::time::Instant;

use super::*;

impl AgentCliExecutor {
    /// EAC-FR-06. Routed through `FsAccess` rather than a bare path predicate,
    /// so a directory outside the session's allowlist is refused here rather
    /// than mounted into a container.
    pub(super) fn validate_directory(
        &self,
        fs: &FsAccess,
        path: &std::path::Path,
    ) -> Result<PathBuf, AgentExecutionError> {
        use AgentExecutionError::ExecutionDirectoryInvalid as Invalid;

        let info = fs.file_info(path).map_err(|error| Invalid(directory_problem(&error)))?;
        if info.kind != EntryKind::Dir {
            return Err(Invalid(DirectoryProblem::NotADirectory));
        }
        // EAC-FR-02 states the execution directory is a **canonical** one, and
        // EAC-FR-ZKMR is the first rule to depend on it: the container is given
        // this path, and Git resolves the repository through paths this module
        // reads back from Git in canonical form. Two spellings of one directory
        // would mount a store the worktree's own records do not name.
        Ok(crate::changes::canonicalize_lenient(path))
    }

    /// Turn a runtime outcome into the two-axis result (EAC-FR-19, EAC-FR-22,
    /// EAC-FR-23).
    pub(super) fn interpret(
        &self,
        descriptor: &descriptor::VendorExecutionDescriptor,
        outcome: runtime::RuntimeOutcome,
        started: Instant,
        container_removed: bool,
        expected_session_id: Option<&str>,
        result_contract: Option<protocol::ResultContract>,
    ) -> AgentExecution {
        let base = |process_outcome: ProcessOutcome, response, session| AgentExecution {
            process_outcome,
            exit_code: outcome.exit_code,
            response,
            stdout: outcome.stdout.clone(),
            stderr: outcome.stderr.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            resume_unavailable: false,
            session,
            refusal: None,
            container_removed,
        };
        // EAC-FR-22: a refused turn says why.
        let refused = |reason: EnvelopeInvalid, session| AgentExecution {
            refusal: Some(reason),
            ..base(ProcessOutcome::InvalidStructuredOutput, None, session)
        };

        match outcome.end {
            RunEnd::TimedOut => return base(ProcessOutcome::Timeout, None, None),
            RunEnd::Cancelled => return base(ProcessOutcome::Cancelled, None, None),
            RunEnd::Terminated => return base(ProcessOutcome::Terminated, None, None),
            // A non-zero exit is not an occasion to look for an envelope: the
            // agent did not finish its turn, so whatever is on stdout is a
            // fragment of one at best.
            RunEnd::Exited if outcome.exit_code != Some(0) => {
                return base(ProcessOutcome::NonZeroExit, None, None)
            }
            // EAC-FR-HZIE: the agent wrote its final event, and the process was
            // stopped after the grace. That is a clean exit with no code.
            RunEnd::Exited | RunEnd::EndedAfterFinalEvent => {}
        }

        // EAC-FR-23: a truncated stdout is a prefix, and a prefix is never
        // parsed. Parsing one would risk accepting a document that happens to
        // close early as a complete report.
        if outcome.stdout.truncated {
            return refused(EnvelopeInvalid::OutputTruncated, None);
        }

        let Some(text) = outcome.stdout.as_utf8() else {
            return refused(EnvelopeInvalid::NotUtf8, None);
        };

        match descriptor.extract_for(text, expected_session_id, result_contract) {
            Ok((envelope, vendor_session)) => {
                // The vendor's own session id fills in where the agent left one
                // out: an agent does not reliably know the id its CLI assigned,
                // and without it EAC-FR-30's resume has nothing to carry.
                let session = merge_session(envelope.session.clone(), vendor_session);
                base(ProcessOutcome::Completed, Some(envelope), session)
            }
            // EAC-FR-21: the session of a refused document stays with the caller,
            // and a session that is not the assigned one is never handed back.
            Err(EnvelopeInvalid::SessionMismatch) => {
                refused(EnvelopeInvalid::SessionMismatch, None)
            }
            // Where the executor assigned the session, only that session is
            // kept: a refusal decided before the identity was asserted
            // (`vendor_run_failed`) does not hand back another one.
            Err(reason) => {
                let session = descriptor
                    .reported_session(text)
                    .filter(|id| expected_session_id.is_none_or(|assigned| assigned == id))
                    .and_then(|id| {
                        merge_session(None, Some(id)).map(protocol::SessionRef::sanitized)
                    });
                refused(reason, session)
            }
        }
    }
}

/// EAC-FR-26: how many times a removal is asked for before it is reported as
/// not confirmed, and the pause between two attempts.
const REMOVE_ATTEMPTS: usize = 3;
const REMOVE_RETRY_PAUSE: Duration = Duration::from_millis(500);

/// EAC-FR-26: remove the container, and ask again where the runtime refused.
/// A refusal can be short-lived, such as a removal that is already in progress.
pub(super) async fn remove_confirmed<S: LogSink + Clone + Send + Sync + 'static>(
    sink: &S,
    runtime: &dyn DockerRuntime,
    name: &str,
) -> bool {
    for attempt in 1..=REMOVE_ATTEMPTS {
        let refusal = match runtime.remove_container(name).await {
            Ok(()) => return true,
            Err(error) => error,
        };
        // The kind of refusal and no text of it: the runtime's own words stay
        // out of the log.
        logging::log_warn(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "agent container removal was refused",
            log_fields! {
                "container" => name,
                "attempt" => attempt as i64,
                "kind" => match refusal {
                    RuntimeError::Unavailable => "runtime_unavailable",
                    RuntimeError::ImageUnavailable => "image_unavailable",
                    RuntimeError::LaunchFailed(_) => "refused",
                },
            },
        );
        if attempt < REMOVE_ATTEMPTS {
            tokio::time::sleep(REMOVE_RETRY_PAUSE).await;
        }
    }
    false
}

/// EAC-FR-30: the result of a resume that cannot continue its session. No
/// container was created, so there is nothing to remove and nothing left.
pub(super) fn resume_unavailable(started: Instant) -> AgentExecution {
    AgentExecution {
        process_outcome: ProcessOutcome::Terminated,
        exit_code: None,
        response: None,
        stdout: CapturedStream::default(),
        stderr: CapturedStream::default(),
        duration_ms: started.elapsed().as_millis() as u64,
        resume_unavailable: true,
        session: None,
        refusal: None,
        container_removed: true,
    }
}
