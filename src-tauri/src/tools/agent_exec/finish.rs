//! The one line the executor writes after a run, however it ended
//! (`EAC-execute-agent-cli.md` EAC-FR-THUT).
//!
//! Without it, a watched run that was stopped reads as output that ends in the
//! middle of a command: nothing on the run's own channel says it was stopped,
//! by what, or after how long.

use super::final_event::FINAL_EVENT_GRACE;
use super::protocol::EnvelopeInvalid;
use super::{AgentExecution, ProcessOutcome};
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

/// EAC-FR-THUT: the line of `execution`. A run that the executor stopped after
/// its final event says so and names the grace, whatever its outcome
/// (EAC-FR-HZIE).
pub fn finished_line_of(
    execution: &AgentExecution,
    limit_ms: u64,
    stopped_after_answer: bool,
) -> String {
    let line = finished_line(
        execution.process_outcome,
        execution.exit_code,
        execution.duration_ms,
        limit_ms,
        execution.refusal,
    );
    if stopped_after_answer {
        with_stop_after_answer(line, FINAL_EVENT_GRACE.as_millis() as u64)
    } else {
        line
    }
}

/// EAC-FR-PPXB: the one record of a run that the executor stopped after its
/// final event. Only this record shows that the process outlived its answer,
/// most probably because of something the agent started. It names the vendor,
/// the container, and the grace, and nothing of the task or the output.
pub(super) fn warn_stopped_after_answer<S>(sink: &S, vendor: &str, container: &str)
where
    S: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        sink,
        &BUFFER,
        &[Domain::Ai, Domain::Backend],
        "agent process still ran after its final event and was stopped",
        log_fields! {
            "vendor" => vendor,
            "container" => container,
            "grace_ms" => FINAL_EVENT_GRACE.as_millis() as u64,
        },
    );
}

/// EAC-FR-THUT: the line that names how a run ended and after how long.
///
/// It carries counts and codes alone, and no task material and no output.
pub fn finished_line(
    outcome: ProcessOutcome,
    exit_code: Option<i32>,
    duration_ms: u64,
    limit_ms: u64,
    refusal: Option<EnvelopeInvalid>,
) -> String {
    let after = human_duration(duration_ms);
    match outcome {
        ProcessOutcome::Completed => format!("Turn finished after {after}."),
        ProcessOutcome::Timeout => format!(
            "Turn stopped: it reached the execution time limit of {} after {after}.",
            human_duration(limit_ms)
        ),
        ProcessOutcome::NonZeroExit => match exit_code {
            Some(code) => {
                format!("Turn stopped: the agent process exited with code {code} after {after}.")
            }
            None => format!("Turn stopped: the agent process exited after {after}."),
        },
        ProcessOutcome::Terminated => {
            format!("Turn stopped: the agent process was terminated after {after}.")
        }
        ProcessOutcome::Cancelled => format!("Turn stopped: it was cancelled after {after}."),
        ProcessOutcome::InvalidStructuredOutput => match refusal {
            Some(reason) => format!(
                "Turn finished after {after}, but its answer could not be read: {}.",
                reason.sentence()
            ),
            None => format!("Turn finished after {after}, but its answer could not be read."),
        },
    }
}

/// EAC-FR-THUT / EAC-FR-HZIE: `line`, followed by the sentence that says the
/// process still ran when the grace ended and was stopped.
pub fn with_stop_after_answer(line: String, grace_ms: u64) -> String {
    format!(
        "{line} The agent process still ran {} after its answer, so it was stopped.",
        human_duration(grace_ms)
    )
}

/// A duration as a person reads it: "2 h", "1 h 5 min", "45 min",
/// "3 min 20 s", "12 s". Seconds are dropped from ten minutes and more.
pub fn human_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let rest = seconds % 60;
    if hours > 0 {
        if minutes == 0 {
            format!("{hours} h")
        } else {
            format!("{hours} h {minutes} min")
        }
    } else if minutes >= 10 || (minutes > 0 && rest == 0) {
        format!("{minutes} min")
    } else if minutes > 0 {
        format!("{minutes} min {rest} s")
    } else {
        format!("{rest} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_HOURS: u64 = 2 * 60 * 60 * 1000;

    // EAC-FR-THUT: every outcome has its own line, naming the duration; a
    // timeout also names the limit and an exit names its code.
    #[test]
    fn every_outcome_has_a_line_naming_how_the_run_ended() {
        assert_eq!(
            finished_line(ProcessOutcome::Timeout, None, TWO_HOURS + 2_000, TWO_HOURS, None),
            "Turn stopped: it reached the execution time limit of 2 h after 2 h."
        );
        assert_eq!(
            finished_line(ProcessOutcome::NonZeroExit, Some(137), 5_000, TWO_HOURS, None),
            "Turn stopped: the agent process exited with code 137 after 5 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::NonZeroExit, None, 5_000, TWO_HOURS, None),
            "Turn stopped: the agent process exited after 5 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Terminated, None, 90_000, TWO_HOURS, None),
            "Turn stopped: the agent process was terminated after 1 min 30 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Cancelled, None, 60_000, TWO_HOURS, None),
            "Turn stopped: it was cancelled after 1 min."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Completed, Some(0), 45 * 60 * 1000, TWO_HOURS, None),
            "Turn finished after 45 min."
        );
        assert_eq!(
            finished_line(
                ProcessOutcome::InvalidStructuredOutput,
                Some(0),
                12_000,
                TWO_HOURS,
                None
            ),
            "Turn finished after 12 s, but its answer could not be read."
        );
    }

    // EAC-FR-THUT: a refused answer says in fixed words why it was refused.
    #[test]
    fn a_refused_answer_names_its_refusal() {
        assert_eq!(
            finished_line(
                ProcessOutcome::InvalidStructuredOutput,
                Some(0),
                67_000,
                TWO_HOURS,
                Some(EnvelopeInvalid::EscalationShape),
            ),
            "Turn finished after 1 min 7 s, but its answer could not be read: the escalation broke a structure rule."
        );
    }

    // EAC-FR-THUT, EAC-FR-HZIE: a run stopped after its answer says so and
    // names the grace.
    #[test]
    fn a_run_stopped_after_its_answer_says_so_and_names_the_grace() {
        let finished = finished_line(
            ProcessOutcome::Completed,
            None,
            56 * 60 * 1000 + 31_000,
            TWO_HOURS,
            None,
        );
        assert_eq!(
            with_stop_after_answer(finished, 30_000),
            "Turn finished after 56 min. The agent process still ran 30 s after its answer, so it was stopped."
        );
        // A refused answer keeps its own words, and the stop follows them.
        let refused = finished_line(
            ProcessOutcome::InvalidStructuredOutput,
            None,
            12_000,
            TWO_HOURS,
            Some(EnvelopeInvalid::VendorRunFailed),
        );
        let line = with_stop_after_answer(refused.clone(), 30_000);
        assert!(line.starts_with(&refused), "{line}");
        assert!(line.ends_with("still ran 30 s after its answer, so it was stopped."), "{line}");
    }

    // EAC-FR-THUT: durations read in the units a person uses.
    #[test]
    fn durations_read_in_hours_minutes_and_seconds() {
        assert_eq!(human_duration(TWO_HOURS), "2 h");
        assert_eq!(human_duration(TWO_HOURS + 59_000), "2 h");
        assert_eq!(human_duration(65 * 60 * 1000), "1 h 5 min");
        assert_eq!(human_duration(45 * 60 * 1000 + 30_000), "45 min");
        assert_eq!(human_duration(200_000), "3 min 20 s");
        assert_eq!(human_duration(60_000), "1 min");
        assert_eq!(human_duration(12_400), "12 s");
        assert_eq!(human_duration(0), "0 s");
        assert_eq!(human_duration(999), "0 s");
        assert_eq!(human_duration(599_000), "9 min 59 s");
        assert_eq!(human_duration(600_000), "10 min");
        assert_eq!(human_duration(659_000), "10 min");
        assert_eq!(human_duration(3_599_999), "59 min");
        assert_eq!(human_duration(3_600_000), "1 h");
    }
}
