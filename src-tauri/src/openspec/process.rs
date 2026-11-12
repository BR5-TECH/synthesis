//! The process seam: one CLI process in a free slot, what its outcome means,
//! and the one more try of a transient outcome (`OSM-openspec-mode.md`
//! OSM-FR-HHUD, OSM-FR-ZIMO, OSM-FR-WWWK, OSM-FR-QHRV, OSM-FR-IFWM;
//! `OFP-openspec-failure-policy.md` OFP-FR-UPDO, OFP-FR-PQNS, OFP-FR-AZIV,
//! OFP-FR-MJZU, OFP-FR-ZQNU, OFP-FR-NJSO, OFP-FR-WNVO).

use std::path::Path;
use std::time::Instant;

use super::failure::{self, CliDiagnostic};
use super::{cli, OpenSpec, OpenSpecReadError, ProcessError, ProcessOutput};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

/// How one CLI process failed, before a read decides what that means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunFailure {
    /// The executable could not be started.
    Spawn,
    /// OSM-FR-ZIMO, with the kept standard error.
    TimedOut { stderr: Vec<u8> },
    /// OSM-FR-WWWK: an exit by a signal (`None`), a status other than 0 and 1,
    /// or exit 1 whose output is not CLI JSON.
    Exit { status: Option<i32>, stderr: Vec<u8> },
    /// OSM-FR-WWWK, OFP-FR-NJSO: exit 1 with CLI JSON.
    Rejected { diagnostics: Vec<CliDiagnostic> },
}

impl RunFailure {
    /// The code of the outcome, for a log record. OFP-FR-DGSC: a process that
    /// did not start is `cli_failed`, so the record of its retry names the code
    /// of the read (OFP-FR-ZQNU).
    pub(crate) fn code(&self) -> &'static str {
        match self {
            RunFailure::Spawn => failure::CODE_FAILED,
            RunFailure::TimedOut { .. } => failure::CODE_TIMED_OUT,
            RunFailure::Exit { .. } => failure::CODE_FAILED,
            RunFailure::Rejected { .. } => failure::CODE_REJECTED,
        }
    }

    /// OFP-FR-DGSC, OFP-FR-UPDO: an outcome of the class `transient`. A
    /// process that did not start reads as `cli_failed` and is one too.
    pub(crate) fn is_transient(&self) -> bool {
        !matches!(self, RunFailure::Rejected { .. })
    }

    /// OSM-FR-WWWK: the exit status a record names, `-1` for none.
    fn exit_status(&self) -> i32 {
        match self {
            RunFailure::Exit { status: Some(status), .. } => *status,
            RunFailure::Rejected { .. } => 1,
            _ => -1,
        }
    }
}

/// OFP-FR-OFLU, OFP-FR-WNVO: a transient outcome carries one diagnostic with
/// its standard error, and a rejection its `status` entries.
impl From<RunFailure> for OpenSpecReadError {
    fn from(failure: RunFailure) -> Self {
        let tail = |stderr: &[u8]| failure::stderr_diagnostic(stderr).into_iter().collect();
        match failure {
            RunFailure::TimedOut { stderr } => {
                OpenSpecReadError::CliTimedOut { diagnostics: tail(&stderr) }
            }
            RunFailure::Spawn => {
                OpenSpecReadError::CliFailed { exit_status: None, diagnostics: Vec::new() }
            }
            RunFailure::Exit { status, stderr } => {
                OpenSpecReadError::CliFailed { exit_status: status, diagnostics: tail(&stderr) }
            }
            RunFailure::Rejected { diagnostics } => OpenSpecReadError::CliRejected { diagnostics },
        }
    }
}

/// How an exit status of 1 reads for one subcommand (OSM-FR-WWWK,
/// OSM-FR-XDRN, OAR-FR-EUXZ).
#[derive(Clone, Copy)]
pub(crate) enum OnExitOne {
    /// The version check: every status other than zero is a failure.
    Failed,
    /// CLI JSON is `cli_rejected`; other output is `cli_failed`.
    Rejected,
    /// `validate`: output that the function accepts is an answer, and other
    /// output reads as `Rejected` does.
    AnswerWhen(fn(&[u8]) -> bool),
    /// `archive`: CLI JSON goes to the caller, which reads its `status`;
    /// other output is `cli_failed`.
    Raw,
}

/// OSM-FR-WWWK: output that decodes as a JSON object is CLI JSON.
fn cli_json(stdout: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice::<serde_json::Value>(stdout).ok().filter(serde_json::Value::is_object)
}

impl OpenSpec {
    /// OSM-FR-HHUD, OSM-FR-ZIMO, OSM-FR-WWWK, OSM-FR-QHRV, OSM-FR-IFWM: one
    /// CLI process in a free slot, logged by subcommand, exit status,
    /// duration, and output size. No argument value, no output, and no
    /// diagnostic reaches a record. The slot is given back when the process
    /// ends.
    // Each argument is one fact of one process; a struct would be read here alone.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_once<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        executable: &Path,
        subcommand: &'static str,
        args: &[&str],
        cwd: &Path,
        on_one: OnExitOne,
    ) -> Result<(i32, Vec<u8>), RunFailure> {
        let started = Instant::now();
        let outcome = {
            let _slot = self.slots.acquire();
            self.runner.run(executable, args, cwd, cli::CLI_TIMEOUT)
        };
        let millis = started.elapsed().as_millis() as u64;
        let read = match outcome {
            Err(ProcessError::TimedOut { stderr }) => Err(RunFailure::TimedOut { stderr }),
            Err(ProcessError::NotExecutable) => Err(RunFailure::Spawn),
            Ok(ProcessOutput { exit_status: Some(0), stdout, .. }) => Ok((0, stdout)),
            Ok(ProcessOutput { exit_status: Some(1), stdout, stderr }) => {
                Self::exit_one(on_one, stdout, stderr)
            }
            Ok(ProcessOutput { exit_status, stderr, .. }) => {
                Err(RunFailure::Exit { status: exit_status, stderr })
            }
        };
        match &read {
            Ok((status, stdout)) => logging::log_info(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec cli ran",
                log_fields! {
                    "subcommand" => subcommand,
                    "exitStatus" => *status,
                    "durationMs" => millis,
                    "outputBytes" => stdout.len(),
                },
            ),
            Err(failure) => logging::log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "openspec cli failed",
                log_fields! {
                    "subcommand" => subcommand,
                    "code" => failure.code(),
                    "exitStatus" => failure.exit_status(),
                    "durationMs" => millis,
                },
            ),
        }
        read
    }

    /// OSM-FR-WWWK, OSM-FR-XDRN: what an exit status of 1 means for `on_one`.
    fn exit_one(
        on_one: OnExitOne,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> Result<(i32, Vec<u8>), RunFailure> {
        let failed = |stderr| Err(RunFailure::Exit { status: Some(1), stderr });
        match on_one {
            OnExitOne::Failed => failed(stderr),
            OnExitOne::AnswerWhen(answers) if answers(&stdout) => Ok((1, stdout)),
            OnExitOne::Raw => match cli_json(&stdout) {
                Some(_) => Ok((1, stdout)),
                None => failed(stderr),
            },
            OnExitOne::Rejected | OnExitOne::AnswerWhen(_) => match cli_json(&stdout) {
                Some(json) => {
                    Err(RunFailure::Rejected { diagnostics: failure::status_diagnostics(&json) })
                }
                None => failed(stderr),
            },
        }
    }

    /// OFP-FR-PQNS, OFP-FR-AZIV: one process of a read, run once more after
    /// the wait where its outcome is transient. The outcome of the second
    /// process is the outcome of the read.
    // Each argument is one fact of one process; a struct would be read here alone.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_read<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        executable: &Path,
        subcommand: &'static str,
        args: &[&str],
        cwd: &Path,
        on_one: OnExitOne,
    ) -> Result<(i32, Vec<u8>), RunFailure> {
        match self.run_once(sink, buffer, executable, subcommand, args, cwd, on_one) {
            Err(first) if first.is_transient() => {
                self.pause_before_retry(sink, buffer, subcommand, first.code());
                self.run_once(sink, buffer, executable, subcommand, args, cwd, on_one)
            }
            other => other,
        }
    }

    /// OSM-FR-WWWK: a read whose exit 1 is no answer, with its output.
    pub(crate) fn run_logged<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        executable: &Path,
        subcommand: &'static str,
        args: &[&str],
        cwd: &Path,
    ) -> Result<Vec<u8>, RunFailure> {
        self.run_read(sink, buffer, executable, subcommand, args, cwd, OnExitOne::Rejected)
            .map(|(_, stdout)| stdout)
    }

    /// OFP-FR-MJZU, OFP-FR-ZQNU: one `WARN` record of the retry, then the
    /// wait. No slot is held: the first process gave its slot back.
    pub(crate) fn pause_before_retry<S: LogSink + Clone + Send + 'static>(
        &self,
        sink: &S,
        buffer: &'static LogBuffer,
        subcommand: &'static str,
        first_code: &'static str,
    ) {
        logging::log_warn(
            sink,
            buffer,
            &[Domain::Backend],
            "openspec cli runs again",
            log_fields! {
                "subcommand" => subcommand,
                "code" => first_code,
                "attempt" => 2,
                "delayMs" => failure::RETRY_DELAY_MS,
            },
        );
        self.wait.wait(failure::RETRY_DELAY_MS);
    }
}
