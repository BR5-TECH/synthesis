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

use super::{change, OpenSpec, RunFailure};
use crate::logging::{LogBuffer, LogSink};

/// OAR-FR-VAGH: what an archive that succeeded gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveAnswer {
    /// `<YYYY-MM-DD>-<change>`, one path segment (OAR-FR-XTLB).
    pub archived_as: String,
}

/// OAR-FR-EUXZ: why an archive did not succeed, as the code the refusal
/// carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveCliFailure {
    /// The `code` of the first `status` entry of an answer whose `archive` is
    /// null. It is a fixed identifier of the CLI (OAR-FR-NEEI).
    Status(String),
    TimedOut,
    Failed,
    OutputUnreadable,
}

impl ArchiveCliFailure {
    pub fn code(&self) -> String {
        match self {
            ArchiveCliFailure::Status(code) => code.clone(),
            ArchiveCliFailure::TimedOut => "cli_timed_out".to_string(),
            ArchiveCliFailure::Failed => "cli_failed".to_string(),
            ArchiveCliFailure::OutputUnreadable => "cli_output_unreadable".to_string(),
        }
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
/// --json --yes` with exit status `status`.
pub fn parse_archive(
    change: &str,
    status: i32,
    bytes: &[u8],
) -> Result<ArchiveAnswer, ArchiveCliFailure> {
    let unreadable =
        if status == 0 { ArchiveCliFailure::OutputUnreadable } else { ArchiveCliFailure::Failed };
    let Ok(out) = serde_json::from_slice::<ArchiveOutput>(bytes) else {
        return Err(unreadable);
    };
    match out.archive {
        None => match out.status.into_iter().next().and_then(|entry| entry.code) {
            Some(code) if identifier(&code) => Err(ArchiveCliFailure::Status(code)),
            Some(_) => Err(ArchiveCliFailure::OutputUnreadable),
            None => Err(unreadable),
        },
        Some(_) if status != 0 => Err(ArchiveCliFailure::Failed),
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
            return Err(ArchiveCliFailure::Failed);
        }
        let args = ["archive", change, "--json", "--yes"];
        match self.run_with_status(sink, buffer, executable, "archive", &args, cwd, &[0, 1]) {
            Ok((status, stdout)) => parse_archive(change, status, &stdout),
            Err(RunFailure::TimedOut) => Err(ArchiveCliFailure::TimedOut),
            Err(RunFailure::Spawn | RunFailure::Exit(_)) => Err(ArchiveCliFailure::Failed),
        }
    }
}
