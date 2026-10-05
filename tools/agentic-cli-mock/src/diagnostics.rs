//! The one thing the mock ever says about itself.
//!
//! ACM-FR-23: a configuration error or an expectation mismatch writes exactly
//! one JSON object followed by exactly one newline to stderr and nothing else.
//! ACM-FR-24: that object never carries an expected or actual value, a decoded
//! payload, or the scenario path — location, length and category only.

use serde::Serialize;

/// ACM-FR-27: the two reserved codes. A scenario may configure neither
/// (ACM-FR-13), so a caller can always tell a simulated tool failure from a
/// failure of the mock itself.
pub const EXIT_CONFIGURATION_ERROR: u8 = 64;
pub const EXIT_EXPECTATION_MISMATCH: u8 = 65;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    ConfigurationError,
    ExpectationMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Scenario,
    Adapter,
    Args,
    RequiredArgs,
    Stdin,
}

/// ACM-FR-25: the fixed set. Nothing outside it is ever emitted, so a caller
/// can match on these categories rather than on prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchType {
    // Adapter validation.
    MissingHeadlessArgument,
    // `expected.args`.
    Arity,
    #[serde(rename = "value")]
    ValueMismatch,
    // `expected.required_args`.
    Absent,
    // `expected.stdin`.
    Length,
    Content,
    // Configuration errors: the invocation.
    MissingToolOption,
    UnknownTool,
    MissingScenarioOption,
    UnknownOption,
    // Configuration errors: the scenario document.
    Unreadable,
    MalformedJson,
    DuplicateKey,
    UnknownField,
    BadVersion,
    BadEncoding,
    BadType,
    BadExitCode,
    BadDelay,
}

/// The record itself. Field declaration order *is* the emitted order — a
/// derived `Serialize` writes struct fields in declaration order — which is
/// half of what makes ACM-FR-23's byte-for-byte determinism hold. The other
/// half is that every field not applicable to a failure is skipped rather than
/// emitted as null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureRecord {
    pub kind: Kind,
    pub code: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub check: Check,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch_type: Option<MismatchType>,
}

impl FailureRecord {
    /// ACM-FR-05 / ACM-FR-23: every configuration error reports `check:
    /// "scenario"`; which one it was lives in `mismatch_type`. `tool` is
    /// present only once `--tool` has resolved to a registered adapter, so a
    /// record for a missing or unknown tool carries no `tool` field at all.
    pub fn configuration_error(tool: Option<&str>, mismatch_type: MismatchType) -> Self {
        FailureRecord {
            kind: Kind::ConfigurationError,
            code: EXIT_CONFIGURATION_ERROR,
            tool: tool.map(str::to_owned),
            check: Check::Scenario,
            index: None,
            name: None,
            expected_length: None,
            actual_length: None,
            mismatch_type: Some(mismatch_type),
        }
    }

    pub fn expectation_mismatch(tool: &str, mismatch: Mismatch) -> Self {
        FailureRecord {
            kind: Kind::ExpectationMismatch,
            code: EXIT_EXPECTATION_MISMATCH,
            tool: Some(tool.to_owned()),
            check: mismatch.check,
            index: mismatch.index,
            name: mismatch.name,
            expected_length: mismatch.expected_length,
            actual_length: mismatch.actual_length,
            mismatch_type: Some(mismatch.mismatch_type),
        }
    }

    /// The wire form: one compact object, one newline, nothing else.
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("failure record is always serialisable");
        line.push('\n');
        line
    }
}

/// What a failed check reports, before it is dressed as a record. Kept separate
/// from `FailureRecord` so the matchers in `matching` never need to know which
/// tool was selected or which exit code the failure maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub check: Check,
    pub index: Option<usize>,
    pub name: Option<String>,
    pub expected_length: Option<usize>,
    pub actual_length: Option<usize>,
    pub mismatch_type: MismatchType,
}

impl Mismatch {
    pub fn new(check: Check, mismatch_type: MismatchType) -> Self {
        Mismatch {
            check,
            index: None,
            name: None,
            expected_length: None,
            actual_length: None,
            mismatch_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_error_omits_every_inapplicable_field() {
        let record = FailureRecord::configuration_error(None, MismatchType::MissingToolOption);
        assert_eq!(
            record.to_line(),
            "{\"kind\":\"configuration_error\",\"code\":64,\"check\":\"scenario\",\"mismatch_type\":\"missing_tool_option\"}\n"
        );
    }

    #[test]
    fn configuration_error_carries_tool_once_the_adapter_resolved() {
        let record =
            FailureRecord::configuration_error(Some("claude"), MismatchType::MissingScenarioOption);
        assert_eq!(
            record.to_line(),
            "{\"kind\":\"configuration_error\",\"code\":64,\"tool\":\"claude\",\"check\":\"scenario\",\"mismatch_type\":\"missing_scenario_option\"}\n"
        );
    }

    /// ACM-FR-23: fields appear in the documented order regardless of how the
    /// mismatch was built.
    #[test]
    fn every_field_serialises_in_the_documented_order() {
        let record = FailureRecord::expectation_mismatch(
            "claude",
            Mismatch {
                check: Check::Args,
                index: Some(3),
                name: Some("prompt".into()),
                expected_length: Some(12),
                actual_length: Some(7),
                mismatch_type: MismatchType::ValueMismatch,
            },
        );
        assert_eq!(
            record.to_line(),
            "{\"kind\":\"expectation_mismatch\",\"code\":65,\"tool\":\"claude\",\"check\":\"args\",\"index\":3,\"name\":\"prompt\",\"expected_length\":12,\"actual_length\":7,\"mismatch_type\":\"value\"}\n"
        );
    }

    #[test]
    fn record_is_exactly_one_object_and_one_newline() {
        let line = FailureRecord::configuration_error(None, MismatchType::MalformedJson).to_line();
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.ends_with('\n'));
        assert!(!line.trim_end().contains('\n'));
    }

    #[test]
    fn check_and_mismatch_type_use_the_documented_spellings() {
        let record = FailureRecord::expectation_mismatch(
            "codex",
            Mismatch::new(Check::RequiredArgs, MismatchType::Absent),
        );
        assert!(record.to_line().contains("\"check\":\"required_args\""));

        let record = FailureRecord::expectation_mismatch(
            "codex",
            Mismatch::new(Check::Adapter, MismatchType::MissingHeadlessArgument),
        );
        assert!(record
            .to_line()
            .contains("\"mismatch_type\":\"missing_headless_argument\""));
    }
}
