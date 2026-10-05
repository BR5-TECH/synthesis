//! The one line the executor writes after a run, however it ended
//! (`EAC-execute-agent-cli.md` EAC-FR-THUT).
//!
//! Without it, a watched run that was stopped reads as output that ends in the
//! middle of a command: nothing on the run's own channel says it was stopped,
//! by what, or after how long.

use super::ProcessOutcome;

/// EAC-FR-THUT: the line that names how a run ended and after how long.
///
/// It carries counts and codes alone, and no task material and no output.
pub fn finished_line(
    outcome: ProcessOutcome,
    exit_code: Option<i32>,
    duration_ms: u64,
    limit_ms: u64,
) -> String {
    let after = human_duration(duration_ms);
    match outcome {
        ProcessOutcome::Completed => format!("Turn finished after {after}."),
        ProcessOutcome::Timeout => format!(
            "Turn stopped: it reached the execution time limit of {} after {after}.",
            human_duration(limit_ms)
        ),
        ProcessOutcome::NonZeroExit => match exit_code {
            Some(code) => format!("Turn stopped: the agent process exited with code {code} after {after}."),
            None => format!("Turn stopped: the agent process exited after {after}."),
        },
        ProcessOutcome::Terminated => {
            format!("Turn stopped: the agent process was terminated after {after}.")
        }
        ProcessOutcome::Cancelled => format!("Turn stopped: it was cancelled after {after}."),
        ProcessOutcome::InvalidStructuredOutput => {
            format!("Turn finished after {after}, but its answer could not be read.")
        }
    }
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
            finished_line(ProcessOutcome::Timeout, None, TWO_HOURS + 2_000, TWO_HOURS),
            "Turn stopped: it reached the execution time limit of 2 h after 2 h."
        );
        assert_eq!(
            finished_line(ProcessOutcome::NonZeroExit, Some(137), 5_000, TWO_HOURS),
            "Turn stopped: the agent process exited with code 137 after 5 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::NonZeroExit, None, 5_000, TWO_HOURS),
            "Turn stopped: the agent process exited after 5 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Terminated, None, 90_000, TWO_HOURS),
            "Turn stopped: the agent process was terminated after 1 min 30 s."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Cancelled, None, 60_000, TWO_HOURS),
            "Turn stopped: it was cancelled after 1 min."
        );
        assert_eq!(
            finished_line(ProcessOutcome::Completed, Some(0), 45 * 60 * 1000, TWO_HOURS),
            "Turn finished after 45 min."
        );
        assert_eq!(
            finished_line(ProcessOutcome::InvalidStructuredOutput, Some(0), 12_000, TWO_HOURS),
            "Turn finished after 12 s, but its answer could not be read."
        );
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
