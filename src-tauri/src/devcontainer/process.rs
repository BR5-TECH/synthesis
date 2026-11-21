//! One run of a host program for the dev container: the CLI (`build`, `up`,
//! `exec`) and the Docker CLI (`run`, `rm`, `compose down`). Each run streams
//! its lines, has a time limit, can be cancelled, and ends the program and
//! every process it started (DVC-FR-RTBA, DVC-FR-ATEO).

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::tools::agent_exec::runtime::CancellationToken;

/// How often a run looks at its cancellation token.
const CANCEL_POLL: Duration = Duration::from_millis(50);
/// How many of the last lines of standard error a run keeps.
const STDERR_TAIL: usize = 40;

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessEnd {
    Exited,
    TimedOut,
    Cancelled,
}

/// What a run gives back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessResult {
    pub end: ProcessEnd,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr_tail: Vec<String>,
}

impl ProcessResult {
    pub fn succeeded(&self) -> bool {
        self.end == ProcessEnd::Exited && self.exit_code == Some(0)
    }

    /// DVC-FR-ATEO: the last line of standard error that holds text, or of
    /// standard output where standard error has none, cut to `limit`
    /// characters.
    pub fn last_error_line(&self, limit: usize) -> String {
        let holds_text = |line: &&str| !line.trim().is_empty();
        let line = self
            .stderr_tail
            .iter()
            .map(String::as_str)
            .rev()
            .find(holds_text)
            .or_else(|| self.stdout.lines().rev().find(holds_text))
            .unwrap_or_default();
        line.trim().chars().take(limit).collect()
    }
}

/// Run `command` with `stdin` written to it. `on_line` sees each line of
/// standard output and standard error as it comes.
pub async fn run(
    mut command: tokio::process::Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    cancel: &CancellationToken,
    mut on_line: impl FnMut(&str) + Send,
) -> std::io::Result<ProcessResult> {
    command
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;

    if let (Some(input), Some(mut handle)) = (stdin, child.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        tokio::spawn(async move {
            let _ = handle.write_all(&input).await;
            let _ = handle.shutdown().await;
        });
    }
    // Split on line feeds as bytes: a value that is not UTF-8 is read lossily
    // rather than ending the stream, and a carriage return stays as it is.
    let mut stdout_lines = BufReader::new(child.stdout.take().expect("piped")).split(b'\n');
    let mut stderr_lines = BufReader::new(child.stderr.take().expect("piped")).split(b'\n');

    let mut stdout = String::new();
    let mut stderr_tail: Vec<String> = Vec::new();
    let mut stdout_open = true;
    let mut stderr_open = true;
    let deadline = tokio::time::Instant::now() + timeout;
    let mut poll = tokio::time::interval(CANCEL_POLL);

    let end = loop {
        if !stdout_open && !stderr_open {
            break ProcessEnd::Exited;
        }
        tokio::select! {
            line = stdout_lines.next_segment(), if stdout_open => match line {
                Ok(Some(bytes)) => {
                    let line = String::from_utf8_lossy(&bytes);
                    on_line(&line);
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                _ => stdout_open = false,
            },
            line = stderr_lines.next_segment(), if stderr_open => match line {
                Ok(Some(bytes)) => {
                    let line = String::from_utf8_lossy(&bytes).into_owned();
                    on_line(&line);
                    stderr_tail.push(line);
                    if stderr_tail.len() > STDERR_TAIL {
                        stderr_tail.remove(0);
                    }
                }
                _ => stderr_open = false,
            },
            _ = tokio::time::sleep_until(deadline) => break ProcessEnd::TimedOut,
            _ = poll.tick() => {
                if cancel.is_cancelled() {
                    break ProcessEnd::Cancelled;
                }
            }
        }
    };

    let exit_code = match end {
        ProcessEnd::Exited => match tokio::time::timeout_at(deadline, child.wait()).await {
            Ok(status) => status?.code(),
            Err(_) => {
                crate::tools::agent_exec::runtime::kill_tree(&mut child).await;
                return Ok(ProcessResult {
                    end: ProcessEnd::TimedOut,
                    exit_code: None,
                    stdout,
                    stderr_tail,
                });
            }
        },
        ProcessEnd::TimedOut | ProcessEnd::Cancelled => {
            crate::tools::agent_exec::runtime::kill_tree(&mut child).await;
            None
        }
    };
    Ok(ProcessResult { end, exit_code, stdout, stderr_tail })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.arg("-c").arg(script);
        command
    }

    // DVC-FR-RTBA: every line reaches the caller as it comes, and the exit
    // code is kept.
    #[tokio::test]
    async fn lines_stream_and_the_exit_code_is_kept() {
        let mut seen = Vec::new();
        let result = run(
            sh("echo one; echo two >&2; exit 4"),
            None,
            Duration::from_secs(10),
            &CancellationToken::new(),
            |line| seen.push(line.to_string()),
        )
        .await
        .unwrap();
        seen.sort();
        assert_eq!(seen, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(result.exit_code, Some(4));
        assert!(!result.succeeded());
        assert_eq!(result.last_error_line(300), "two");
    }

    // DVC-FR-ATEO: a run over its limit is ended and reported as such.
    #[tokio::test]
    async fn a_run_over_its_limit_is_ended() {
        let result = run(
            sh("sleep 30"),
            None,
            Duration::from_millis(100),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.end, ProcessEnd::TimedOut);
    }

    // DVC-FR-RTBA: a cancelled run ends the program and every process it
    // started.
    #[tokio::test]
    async fn a_cancelled_run_is_ended() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = run(sh("sleep 30 & sleep 30"), None, Duration::from_secs(20), &cancel, |_| {})
            .await
            .unwrap();
        assert_eq!(result.end, ProcessEnd::Cancelled);
    }

    // The input reaches the program on standard input.
    #[tokio::test]
    async fn stdin_reaches_the_program() {
        let result = run(
            sh("cat"),
            Some(b"hello\n".to_vec()),
            Duration::from_secs(10),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.stdout, "hello\n");
        assert!(result.succeeded());
    }

    // DVC-FR-BUXC: bytes that are not UTF-8 do not end the stream, and a
    // carriage return stays in its line.
    #[tokio::test]
    async fn bytes_that_are_not_utf8_do_not_end_the_stream() {
        let result = run(
            sh("printf 'A=\\377\\nB=two\\r\\nC=three\\n'"),
            None,
            Duration::from_secs(10),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        let lines: Vec<&str> = result.stdout.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 3, "{:?}", result.stdout);
        assert!(lines[0].starts_with("A="));
        assert_eq!(lines[1], "B=two\r");
        assert_eq!(lines[2], "C=three");
    }
}
