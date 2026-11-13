//! The OpenSpec failure policy: the classes of a failure, the object a
//! command rejects with, and the diagnostics of the CLI
//! (`OFP-openspec-failure-policy.md` OFP-FR-PHOW through OFP-FR-RQXW,
//! OFP-FR-FKRV, OFP-FR-OFLU through OFP-FR-QOET, OFP-FR-QLQJ through
//! OFP-FR-EOAD).
//!
//! Diagnostics are the CLI's own words about a change. They reach the author
//! alone: no log record, no event payload, and no result of an agent tool
//! holds one (OFP-FR-WRDX). Nothing in this module logs.

use serde::{Deserialize, Serialize};

use super::{OpenSpecReadError, ValidationIssue};

/// OFP-FR-PQNS: the wait before a transient process runs once more.
pub const RETRY_DELAY_MS: u64 = 2_000;
/// OFP-FR-WNVO: how much standard error the seam keeps for each process.
pub const STDERR_KEEP: usize = 4 * 1024;
/// OFP-FR-QOET: the most entries a list of diagnostics holds.
pub const MAX_DIAGNOSTICS: usize = 50;
/// OFP-FR-QOET: the longest message and the longest fix, in bytes.
pub const MAX_TEXT: usize = 2 * 1024;

/// OFP-FR-SQDK: the codes of the class `environment`.
pub const CODE_MODE_INACTIVE: &str = "openspec_mode_inactive";
pub const CODE_CLI_UNAVAILABLE: &str = "cli_unavailable";
pub const CODE_OUTPUT_UNREADABLE: &str = "cli_output_unreadable";
pub const CODE_IMAGE_CLI_UNAVAILABLE: &str = "openspec_image_cli_unavailable";
/// OFP-FR-DGSC: the codes of the class `transient`.
pub const CODE_TIMED_OUT: &str = "cli_timed_out";
pub const CODE_FAILED: &str = "cli_failed";
/// OFP-FR-RQXW: the codes of the class `content` that are not archive codes.
pub const CODE_REJECTED: &str = "cli_rejected";
pub const CODE_CHANGE_NOT_READY: &str = "change_not_ready";
pub const CODE_TASKS_INCOMPLETE: &str = "tasks_incomplete";

/// OFP-FR-HQQN: the CLI `status` codes of an archive that a repair run can
/// fix.
pub const REPAIR_CAUSES: [&str; 3] =
    ["archive_spec_update_failed", "archive_spec_validation_failed", "archive_validation_failed"];

/// The refusals that wrap an OpenSpec code in their `cause` (OFP-FR-ZDEE).
/// None of them is a CLI `status` code of an archive.
const WRAPPING: [&str; 4] = [
    "apply_progress_unavailable",
    "openspec_validation_unavailable",
    "change_not_archivable",
    "archive_failed",
];

/// The prefix of every CLI `status` code of an archive (OFP-FR-RQXW).
const ARCHIVE_STATUS_PREFIX: &str = "archive_";

/// OFP-FR-PHOW: the class of a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Environment,
    Transient,
    Content,
    None,
}

/// OFP-FR-PHOW, OFP-FR-SQDK, OFP-FR-DGSC, OFP-FR-RQXW, OFP-FR-FKRV: the class
/// of a classifying code, from the class table alone. It reads no file, no
/// setting, and no clock.
///
/// A CLI `status` code of an archive is recognised by its `archive_` prefix,
/// which every archive code of the class table has. The wrapping refusal
/// `archive_failed` is not one: it takes the class of its cause.
pub fn classify_openspec_failure(code: &str) -> FailureClass {
    match code {
        CODE_MODE_INACTIVE
        | CODE_CLI_UNAVAILABLE
        | CODE_OUTPUT_UNREADABLE
        | CODE_IMAGE_CLI_UNAVAILABLE => FailureClass::Environment,
        CODE_TIMED_OUT | CODE_FAILED => FailureClass::Transient,
        CODE_REJECTED | CODE_CHANGE_NOT_READY | CODE_TASKS_INCOMPLETE => FailureClass::Content,
        code if code.starts_with(ARCHIVE_STATUS_PREFIX) && !WRAPPING.contains(&code) => {
            FailureClass::Content
        }
        _ => FailureClass::None,
    }
}

/// OFP contract surface `CliDiagnostic`: one message of the CLI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDiagnostic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl CliDiagnostic {
    /// A diagnostic with a message and a severity alone.
    pub fn of(severity: &str, message: impl Into<String>) -> Self {
        CliDiagnostic {
            severity: Some(severity.to_string()),
            message: message.into(),
            ..CliDiagnostic::default()
        }
    }

    /// OFP contract surface, fixed text: `- <severity> <code> <path>:
    /// <message>`, each absent part left out, then ` Fix: <fix>` where the
    /// entry has a fix.
    pub fn line(&self) -> String {
        let head: Vec<&str> = [&self.severity, &self.code, &self.path]
            .into_iter()
            .filter_map(|part| part.as_deref())
            .filter(|part| !part.is_empty())
            .collect();
        let mut line = if head.is_empty() {
            format!("- {}", self.message)
        } else {
            format!("- {}: {}", head.join(" "), self.message)
        };
        if let Some(fix) = self.fix.as_deref().filter(|fix| !fix.is_empty()) {
            line.push_str(" Fix: ");
            line.push_str(fix);
        }
        line
    }
}

/// OFP-FR-QOET: the first `max` bytes of `text`, cut at a character boundary.
fn cut(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// OFP-FR-QOET: at most 50 entries, the first in the order of the CLI, with
/// each message and each fix cut to its first 2 KiB.
pub fn bounded(diagnostics: Vec<CliDiagnostic>) -> Vec<CliDiagnostic> {
    diagnostics
        .into_iter()
        .take(MAX_DIAGNOSTICS)
        .map(|entry| CliDiagnostic {
            message: cut(entry.message, MAX_TEXT),
            fix: entry.fix.map(|fix| cut(fix, MAX_TEXT)),
            ..entry
        })
        .collect()
}

/// OFP-FR-WNVO: the last 4 KiB of `stderr`, keeping the end, with no
/// severity. Each sequence that is not valid UTF-8 is replaced. Empty
/// standard error gives no entry.
///
/// The 2 KiB cut of OFP-FR-QOET does not apply here: WNVO names the last
/// 4 KiB, and the end of standard error is where a CLI states why it stopped.
pub fn stderr_diagnostic(stderr: &[u8]) -> Option<CliDiagnostic> {
    let tail = &stderr[stderr.len().saturating_sub(STDERR_KEEP)..];
    let text = String::from_utf8_lossy(tail).trim().to_string();
    (!text.is_empty()).then(|| CliDiagnostic { message: text, ..Default::default() })
}

/// OFP-FR-NJSO: the `status` entries of CLI JSON, in the order of the CLI,
/// each with its `severity`, `code`, `message`, and `fix`. An entry with no
/// message gives its code as the message.
pub fn status_diagnostics(json: &serde_json::Value) -> Vec<CliDiagnostic> {
    let text = |entry: &serde_json::Value, key: &str| {
        entry.get(key).and_then(serde_json::Value::as_str).map(str::to_string)
    };
    let entries = json.get("status").and_then(serde_json::Value::as_array);
    bounded(
        entries
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let code = text(entry, "code");
                let message = text(entry, "message").or_else(|| code.clone())?;
                Some(CliDiagnostic {
                    severity: text(entry, "severity"),
                    code,
                    path: None,
                    message,
                    fix: text(entry, "fix"),
                })
            })
            .collect(),
    )
}

/// OFP-FR-RUOP: the issues of a validation that answers not valid, in order,
/// each with its level as the severity, its path, and its message.
pub fn issue_diagnostics(issues: &[ValidationIssue]) -> Vec<CliDiagnostic> {
    bounded(
        issues
            .iter()
            .map(|issue| CliDiagnostic {
                severity: Some(issue.level.to_ascii_lowercase()),
                code: None,
                path: (!issue.path.is_empty()).then(|| issue.path.clone()),
                message: issue.message.clone(),
                fix: None,
            })
            .collect(),
    )
}

/// OFP-FR-BWML: the diagnostics of `change_not_ready`: one `error` for each
/// missing artifact, one `warning` for each warning, and one `info` with the
/// instruction.
pub fn not_ready_diagnostics(
    missing: &[String],
    warnings: &[String],
    instruction: Option<&str>,
) -> Vec<CliDiagnostic> {
    let mut out: Vec<CliDiagnostic> = missing
        .iter()
        .map(|artifact| {
            CliDiagnostic::of("error", format!("The artifact \"{artifact}\" is missing."))
        })
        .collect();
    out.extend(warnings.iter().map(|warning| CliDiagnostic::of("warning", warning.clone())));
    if let Some(instruction) = instruction.filter(|text| !text.trim().is_empty()) {
        out.push(CliDiagnostic::of("info", instruction.to_string()));
    }
    bounded(out)
}

/// OFP contract surface `OpenSpecFailure`: what a command of the contract
/// surface rejects an OpenSpec failure with, camelCase on the wire
/// (OFP-FR-QLQJ).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSpecFailure {
    /// OFP-FR-ZDEE: the refusal code of the operation.
    pub code: String,
    /// OFP-FR-ZDEE: the OpenSpec code under a refusal that wraps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    /// OFP-FR-ZDEE: the change the refusal names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// OFP-FR-EOAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// OFP-FR-MQGG: the thread of a merge notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// OFP-FR-XQDY: the repair run of a merge notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_run_id: Option<String>,
    /// OFP-FR-PCQY: the code that refused the repair start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_refusal: Option<String>,
    /// OFP-FR-OFLU: empty where there is none.
    #[serde(default)]
    pub diagnostics: Vec<CliDiagnostic>,
}

impl OpenSpecFailure {
    /// A failure with a code alone.
    pub fn new(code: impl Into<String>) -> Self {
        OpenSpecFailure { code: code.into(), ..OpenSpecFailure::default() }
    }

    /// OFP-FR-EOAD, OFP-FR-OFLU: the failure of a read, with the detail and the
    /// diagnostics of its code.
    pub fn of_read(error: &OpenSpecReadError) -> Self {
        OpenSpecFailure {
            code: error.code().to_string(),
            detail: error.detail(),
            diagnostics: error.diagnostics().to_vec(),
            ..OpenSpecFailure::default()
        }
    }

    /// OFP-FR-ZDEE: a refusal `code` that wraps the code of a read as its
    /// cause, with the read's detail and diagnostics.
    pub fn wrapping(code: &str, error: &OpenSpecReadError) -> Self {
        OpenSpecFailure {
            code: code.to_string(),
            cause: Some(error.code().to_string()),
            detail: error.detail(),
            diagnostics: error.diagnostics().to_vec(),
            ..OpenSpecFailure::default()
        }
    }

    /// The same failure, naming `change`.
    pub fn for_change(mut self, change: &str) -> Self {
        self.change = Some(change.to_string());
        self
    }

    /// OFP-FR-BYYT: the `cause` where the failure carries one, and the `code`
    /// otherwise.
    pub fn classifying_code(&self) -> &str {
        self.cause.as_deref().unwrap_or(&self.code)
    }

    /// OFP-FR-PHOW: the class of the classifying code.
    pub fn class(&self) -> FailureClass {
        classify_openspec_failure(self.classifying_code())
    }
}

/// The failure as one line of text, in the typed form of its operation:
/// `<code>`, then `: <change>` for the two archive refusals
/// (`change_not_archivable: <change>: <reason>`, `archive_failed: <change>:
/// <code>`), then `: <cause>` or `: <detail>`. It never holds a diagnostic,
/// so a log record may carry it.
impl std::fmt::Display for OpenSpecFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;
        let archive = self.code == "change_not_archivable" || self.code == "archive_failed";
        if let Some(change) = self.change.as_ref().filter(|_| archive) {
            write!(f, ": {change}")?;
        }
        match (&self.cause, &self.detail) {
            (Some(cause), _) => write!(f, ": {cause}"),
            (None, Some(detail)) if !detail.is_empty() => write!(f, ": {detail}"),
            _ => Ok(()),
        }
    }
}

/// A refusal of a command that can reject an OpenSpec failure (OFP-FR-QLQJ):
/// a typed code as before, or an `OpenSpecFailure` object. On the wire the
/// code is a string and the failure is an object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Refusal {
    Code(String),
    OpenSpec(Box<OpenSpecFailure>),
}

impl Refusal {
    /// The code of the refusal: the part before the first colon of a typed
    /// code, or the code of a failure.
    pub fn code(&self) -> &str {
        match self {
            Refusal::Code(text) => text.split(':').next().unwrap_or_default().trim(),
            Refusal::OpenSpec(failure) => &failure.code,
        }
    }

    /// The OpenSpec failure, where the refusal is one.
    pub fn openspec(&self) -> Option<&OpenSpecFailure> {
        match self {
            Refusal::OpenSpec(failure) => Some(failure),
            Refusal::Code(_) => None,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Code(text) => f.write_str(text),
            Refusal::OpenSpec(failure) => failure.fmt(f),
        }
    }
}

impl From<String> for Refusal {
    fn from(text: String) -> Self {
        Refusal::Code(text)
    }
}

impl From<&str> for Refusal {
    fn from(text: &str) -> Self {
        Refusal::Code(text.to_string())
    }
}

impl From<OpenSpecFailure> for Refusal {
    fn from(failure: OpenSpecFailure) -> Self {
        Refusal::OpenSpec(Box::new(failure))
    }
}

/// The refusal as the text of a command that keeps its own form.
impl From<Refusal> for String {
    fn from(refusal: Refusal) -> Self {
        refusal.to_string()
    }
}

/// A refusal reads equal to the text of its line form, so a check of a typed
/// code needs no match on the variant.
impl PartialEq<&str> for Refusal {
    // The line form is built for the comparison: a refusal is compared with a
    // typed code in checks of a test, in no hot path.
    #[allow(clippy::cmp_owned)]
    fn eq(&self, other: &&str) -> bool {
        self.to_string() == *other
    }
}

impl PartialEq<str> for Refusal {
    // As above: the line form is built for the comparison.
    #[allow(clippy::cmp_owned)]
    fn eq(&self, other: &str) -> bool {
        self.to_string() == other
    }
}

impl PartialEq<String> for Refusal {
    // As above: the line form is built for the comparison.
    #[allow(clippy::cmp_owned)]
    fn eq(&self, other: &String) -> bool {
        self.to_string() == *other
    }
}

#[cfg(test)]
mod tests;
