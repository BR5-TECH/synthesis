//! The check gate on the answer of a `work` or `merge_work` turn
//! (`../ai/GRL-graduation-loop.md` GRL-FR-IJTT, GRL-FR-CPXI, GRL-FR-TYRV,
//! GRL-FR-XWNQ, GRL-FR-HLNB).
//!
//! The gate reads the check record a turn reported as the agent's claim. It
//! runs no check, and it reads nothing from the working copy. What it decides
//! is only whether the answer may go to review: an answer it refuses gets a
//! check correction, then an escalate correction, and then the run rests for
//! the author (`gate_flow`).

use super::*;
use crate::graduation::{GraduationCheckCause, GraduationRestCheck};
use crate::tools::agent_exec::protocol::AgentOutcome;

/// GRL-FR-IJTT: at most this many check records.
pub const MAX_CHECKS: usize = 64;
/// GRL-FR-IJTT: the longest check name, in code points.
pub const MAX_NAME: usize = 128;
/// GRL-FR-IJTT: the longest check command, in code points.
pub const MAX_COMMAND: usize = 1024;
/// GRL-FR-IJTT: the longest `no_checks_reason`, in code points.
pub const MAX_REASON: usize = 1024;

/// GRL-FR-IJTT: the success answer of a `work` and a `merge_work` turn.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkResult {
    pub checks: Vec<CheckRecord>,
    #[serde(default)]
    pub no_checks_reason: Option<String>,
}

/// GRL-FR-IJTT: one full check the turn ran, as it reports it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRecord {
    pub name: String,
    pub command: String,
    pub finished: bool,
    /// Present in every record, and null where the check did not end.
    #[serde(deserialize_with = "present_or_null")]
    pub exit_status: Option<i64>,
    /// GRL-FR-HLNB: the 1-based position of an entry of `input.decisions`.
    #[serde(default)]
    pub waived_by: Option<u64>,
}

/// A field that must be present, and may be null. A deserializer named with
/// `deserialize_with` makes a missing key an error, which plain `Option` does
/// not.
fn present_or_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(d)
}

fn bounded(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max
}

impl WorkResult {
    /// GRL-FR-IJTT: the result object of an answer, where it is a valid
    /// `WorkResult`.
    pub fn read(result: &serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        let read: Self = serde_json::from_value(serde_json::Value::Object(result.clone())).ok()?;
        if read.checks.len() > MAX_CHECKS {
            return None;
        }
        if read.no_checks_reason.as_deref().is_some_and(|reason| !bounded(reason, MAX_REASON)) {
            return None;
        }
        let valid = read.checks.iter().all(|check| {
            bounded(&check.name, MAX_NAME)
                && bounded(&check.command, MAX_COMMAND)
                && check.waived_by.is_none_or(|position| position >= 1)
        });
        valid.then_some(read)
    }
}

/// What the gate read from one answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GateReading {
    /// `None` where the answer is accepted.
    pub cause: Option<GraduationCheckCause>,
    /// GLG-FR-FSZH: the counts the gate record carries.
    pub checks: usize,
    pub unfinished: usize,
    pub red: usize,
    pub waived: usize,
    /// GRD-FR-PYJN: each check of the record that did not pass.
    pub failing: Vec<GraduationRestCheck>,
}

/// GRL-FR-CPXI / GRL-FR-TYRV / GRL-FR-XWNQ / GRL-FR-HLNB: judge the readable
/// answer of a completed `work` or `merge_work` turn.
///
/// `decisions` is the number of entries the turn's input `decisions` held, so
/// a `waived_by` is valid only where it names one of them.
pub fn judge(execution: &AgentExecution, decisions: usize) -> GateReading {
    let Some(response) = execution.response.as_ref() else {
        return GateReading {
            cause: Some(GraduationCheckCause::RecordMissing),
            ..GateReading::default()
        };
    };
    let record = response.result.as_ref().and_then(WorkResult::read);
    let mut reading = record.as_ref().map(|r| read_record(r, decisions)).unwrap_or_default();
    reading.cause = match response.outcome {
        // GRL-FR-XWNQ: a recorded escalation never reaches the gate, so an
        // escalation answer here is one whose escalation was not recorded. It
        // holds no record (envelope exclusivity), and the author was not asked.
        AgentOutcome::EscalationRequired => Some(GraduationCheckCause::RecordMissing),
        // GRL-FR-TYRV: a work turn that cannot finish asks the author.
        AgentOutcome::Failure => Some(GraduationCheckCause::WorkFailed),
        AgentOutcome::Success => match record.as_ref() {
            None => Some(GraduationCheckCause::RecordMissing),
            Some(record)
                if record.checks.is_empty()
                    && record.no_checks_reason.as_deref().is_none_or(|r| r.trim().is_empty()) =>
            {
                Some(GraduationCheckCause::RecordMissing)
            }
            Some(_) if reading.unfinished > 0 => Some(GraduationCheckCause::ChecksUnfinished),
            Some(_) if reading.red > 0 => Some(GraduationCheckCause::ChecksRed),
            Some(_) => None,
        },
    };
    reading
}

/// The counts of one record, and the checks of it that did not pass.
fn read_record(record: &WorkResult, decisions: usize) -> GateReading {
    let mut reading = GateReading { checks: record.checks.len(), ..GateReading::default() };
    for check in &record.checks {
        let waived =
            check.waived_by.is_some_and(|position| position >= 1 && position as usize <= decisions);
        let failed = if !check.finished {
            reading.unfinished += 1;
            true
        } else if check.exit_status != Some(0) {
            if waived {
                reading.waived += 1;
                false
            } else {
                reading.red += 1;
                true
            }
        } else {
            false
        };
        if failed {
            reading.failing.push(GraduationRestCheck {
                name: check.name.clone(),
                finished: check.finished,
                exit_status: check.exit_status,
            });
        }
    }
    reading
}

#[cfg(test)]
#[path = "check_gate/tests.rs"]
mod tests;
