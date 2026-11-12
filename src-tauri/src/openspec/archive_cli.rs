//! The one write subcommand the application runs: `openspec archive`
//! (`OSM-openspec-mode.md` OSM-FR-XKQB; `OAR-openspec-archive.md`
//! OAR-FR-IAKL, OAR-FR-VAGH, OAR-FR-XTLB, OAR-FR-EUXZ).
//!
//! Only `crate::openspec_archive` calls it. The process runs on the terms of
//! every other CLI process of this module: the resolved CLI, the root of the
//! working copy, the fixed variables, the time limit, and the slot limit. Its
//! log records name the subcommand, the exit status, the duration, and the
//! byte count, and no argument or output (OSM-FR-IFWM).

use std::path::Path;

use serde::Deserialize;

use super::failure::{self, CliDiagnostic};
use super::{change, OnExitOne, OpenSpec, OpenSpecReadError};
use crate::logging::{LogBuffer, LogSink};

/// OAR-FR-VAGH: what an archive that succeeded gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveAnswer {
    /// `<YYYY-MM-DD>-<change>`, one path segment (OAR-FR-XTLB).
    pub archived_as: String,
}

/// OAR-FR-EUXZ: why an archive did not succeed, as the code the refusal
/// carries, with the diagnostics of `OFP-openspec-failure-policy.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveCliFailure {
    /// The `code` of the first `status` entry of an answer whose `archive` is
    /// null. It is a fixed identifier of the CLI (OAR-FR-NEEI). The
    /// diagnostics are every `status` entry (OFP-FR-NJSO).
    Status {
        code: String,
        diagnostics: Vec<CliDiagnostic>,
    },
    /// OFP-FR-WNVO: with the kept standard error.
    TimedOut {
        diagnostics: Vec<CliDiagnostic>,
    },
    /// OFP-FR-WNVO: with the kept standard error.
    Failed {
        diagnostics: Vec<CliDiagnostic>,
    },
    /// Exit 1 with CLI JSON and no `status` code (OAR-FR-EUXZ).
    Rejected {
        diagnostics: Vec<CliDiagnostic>,
    },
    OutputUnreadable,
}

impl ArchiveCliFailure {
    pub fn code(&self) -> String {
        match self {
            ArchiveCliFailure::Status { code, .. } => code.clone(),
            ArchiveCliFailure::TimedOut { .. } => failure::CODE_TIMED_OUT.to_string(),
            ArchiveCliFailure::Failed { .. } => failure::CODE_FAILED.to_string(),
            ArchiveCliFailure::Rejected { .. } => failure::CODE_REJECTED.to_string(),
            ArchiveCliFailure::OutputUnreadable => failure::CODE_OUTPUT_UNREADABLE.to_string(),
        }
    }

    /// OFP-FR-OFLU: the diagnostics of the failure.
    pub fn diagnostics(&self) -> &[CliDiagnostic] {
        match self {
            ArchiveCliFailure::Status { diagnostics, .. }
            | ArchiveCliFailure::TimedOut { diagnostics }
            | ArchiveCliFailure::Failed { diagnostics }
            | ArchiveCliFailure::Rejected { diagnostics } => diagnostics,
            ArchiveCliFailure::OutputUnreadable => &[],
        }
    }

    /// OFP-FR-IJYA: an outcome of the class `transient`.
    pub fn is_transient(&self) -> bool {
        matches!(self, ArchiveCliFailure::TimedOut { .. } | ArchiveCliFailure::Failed { .. })
    }

    /// A `cli_failed` with no diagnostic.
    pub fn failed() -> Self {
        ArchiveCliFailure::Failed { diagnostics: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// The CLI fields read (OAR contract surface, "CLI fields read")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ArchiveOutput {
    #[serde(default)]
    archive: Option<ArchiveObject>,
    #[serde(default)]
    status: Vec<StatusEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveObject {
    change: String,
    archived_as: String,
}

#[derive(Deserialize)]
struct StatusEntry {
    #[serde(default)]
    code: Option<String>,
}

/// OAR-FR-NEEI: a status code reaches a refusal and a log record only where it
/// has the shape of a fixed identifier. Anything else is read as output that
/// does not decode.
fn identifier(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// OAR-FR-VAGH, OAR-FR-XTLB, OAR-FR-EUXZ: the answer of `archive <change>
/// --json --yes` with exit status `status`. The process seam gives an exit 1
/// here only with CLI JSON.
pub fn parse_archive(
    change: &str,
    status: i32,
    bytes: &[u8],
) -> Result<ArchiveAnswer, ArchiveCliFailure> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Err(if status == 0 {
            ArchiveCliFailure::OutputUnreadable
        } else {
            ArchiveCliFailure::failed()
        });
    };
    let diagnostics = failure::status_diagnostics(&json);
    // Exit 1 with CLI JSON that names no status code is a rejection.
    let rejected = || {
        if status == 0 {
            ArchiveCliFailure::OutputUnreadable
        } else {
            ArchiveCliFailure::Rejected { diagnostics: diagnostics.clone() }
        }
    };
    let Ok(out) = serde_json::from_value::<ArchiveOutput>(json.clone()) else {
        return Err(rejected());
    };
    match out.archive {
        None => match out.status.into_iter().next().and_then(|entry| entry.code) {
            Some(code) if identifier(&code) => {
                Err(ArchiveCliFailure::Status { code, diagnostics: diagnostics.clone() })
            }
            Some(_) => Err(ArchiveCliFailure::OutputUnreadable),
            None => Err(rejected()),
        },
        Some(_) if status != 0 => Err(rejected()),
        Some(archive) => {
            if archive.change != change || !change::archive_folder_of(&archive.archived_as, change)
            {
                return Err(ArchiveCliFailure::OutputUnreadable);
            }
            Ok(ArchiveAnswer { archived_as: archive.archived_as })
        }
    }
}

impl OpenSpec {
    /// OAR-FR-IAKL: `openspec archive <change> --json --yes` in the root of
    /// the working copy `cwd`, with the CLI `executable` that the check
    /// resolved. A status of 1 is read, because the CLI answers a refused
    /// archive with JSON (OAR-FR-EUXZ).
    pub fn archive_change<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        executable: &Path,
        cwd: &Path,
        change: &str,
    ) -> Result<ArchiveAnswer, ArchiveCliFailure> {
        if !change::valid_change_name(change) {
            return Err(ArchiveCliFailure::failed());
        }
        let args = ["archive", change, "--json", "--yes"];
        // OFP-FR-IJYA: one process alone. The step runs it again, once the
        // working copy is restored.
        match self.run_once(sink, buffer, executable, "archive", &args, cwd, OnExitOne::Raw) {
            Ok((status, stdout)) => parse_archive(change, status, &stdout),
            Err(failure) => Err(match OpenSpecReadError::from(failure) {
                OpenSpecReadError::CliTimedOut { diagnostics } => {
                    ArchiveCliFailure::TimedOut { diagnostics }
                }
                OpenSpecReadError::CliRejected { diagnostics } => {
                    ArchiveCliFailure::Rejected { diagnostics }
                }
                other => ArchiveCliFailure::Failed { diagnostics: other.diagnostics().to_vec() },
            }),
        }
    }

    /// OFP-FR-IJYA, OFP-FR-ZQNU: the record and the wait before an archive
    /// runs once more.
    pub fn pause_before_archive_retry<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        failure: &ArchiveCliFailure,
    ) {
        let code = match failure {
            ArchiveCliFailure::TimedOut { .. } => failure::CODE_TIMED_OUT,
            _ => failure::CODE_FAILED,
        };
        self.pause_before_retry(sink, buffer, "archive", code);
    }
}
