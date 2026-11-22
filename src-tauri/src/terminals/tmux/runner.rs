//! The two seams to tmux: one command list that runs to its end, and one
//! client on a pseudo-terminal (TRS-FR-ZOZO, TRS-FR-BHCB).

use std::io;
use std::time::Duration;

use crate::terminals::process::{terminal_env, HostCommand, ProcessHandle};

/// Why a tmux call failed. It names the cause and never holds output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmuxError {
    /// tmux did not start.
    Spawn,
    /// tmux did not end in time.
    Timeout,
    /// tmux ended with a failure status, for example for a session that is gone.
    Status,
    /// tmux answered with output that the module cannot read.
    Output,
}

impl TmuxError {
    pub fn cause(self) -> &'static str {
        match self {
            TmuxError::Spawn => "tmux_spawn",
            TmuxError::Timeout => "tmux_timeout",
            TmuxError::Status => "tmux_status",
            TmuxError::Output => "tmux_output",
        }
    }
}

/// Runs one tmux command list on the application's socket.
pub trait TmuxRunner: Send + Sync {
    /// Runs `args` (commands joined by `;` arguments) and returns the standard
    /// output.
    fn run(&self, args: &[String]) -> Result<String, TmuxError>;
}

/// Starts a tmux client attached to one session, on a new pseudo-terminal.
pub trait ClientSpawner: Send + Sync {
    fn spawn(&self, session: &str, cols: u16, rows: u16) -> io::Result<ProcessHandle>;
}

/// The longest time one tmux call runs.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Variables that make tmux think it runs inside another tmux.
const NESTING_VARIABLES: [&str; 2] = ["TMUX", "TMUX_PANE"];

/// The real tmux executable on one socket.
pub struct RealTmux {
    pub program: String,
    pub socket: String,
}

/// TRS-FR-ZOZO, TRS-FR-ANJK: the arguments of every tmux call: UTF-8 also
/// where the application has no locale, the application's socket, and no
/// configuration file of the user, also for a server that a client starts.
pub fn base_args(socket: &str) -> Vec<String> {
    vec!["-u".into(), "-L".into(), socket.into(), "-f".into(), "/dev/null".into()]
}

/// TRS-FR-BHCB: the command of a client attached to exactly one session.
pub fn client_command(program: &str, socket: &str, session: &str) -> HostCommand {
    let mut args = base_args(socket);
    args.extend(["attach-session".into(), "-t".into(), super::commands::exact(session)]);
    HostCommand {
        program: program.into(),
        args,
        cwd: "/".into(),
        env: terminal_env(),
        env_remove: NESTING_VARIABLES.iter().map(|s| s.to_string()).collect(),
    }
}

impl TmuxRunner for RealTmux {
    fn run(&self, args: &[String]) -> Result<String, TmuxError> {
        let mut command = std::process::Command::new(&self.program);
        command.args(base_args(&self.socket)).args(args);
        for name in NESTING_VARIABLES {
            command.env_remove(name);
        }
        run_bounded(command, CALL_TIMEOUT)
    }
}

/// Runs `command` with no input, and returns its standard output where it ends
/// with success inside `timeout`.
pub fn run_bounded(
    mut command: std::process::Command,
    timeout: Duration,
) -> Result<String, TmuxError> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| TmuxError::Spawn)?;
    let mut stdout = child.stdout.take().ok_or(TmuxError::Spawn)?;
    // The output goes to a thread of its own, so a large capture never fills
    // the pipe while this thread waits for the exit.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TmuxError::Timeout);
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(TmuxError::Status);
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// The real tmux client, on a host pseudo-terminal.
pub struct RealClients {
    pub program: String,
    pub socket: String,
}

#[cfg(unix)]
impl ClientSpawner for RealClients {
    fn spawn(&self, session: &str, cols: u16, rows: u16) -> io::Result<ProcessHandle> {
        let command = client_command(&self.program, &self.socket, session);
        crate::terminals::process::spawn_host(&command, cols, rows, None, None)
    }
}

#[cfg(not(unix))]
impl ClientSpawner for RealClients {
    fn spawn(&self, _session: &str, _cols: u16, _rows: u16) -> io::Result<ProcessHandle> {
        Err(io::Error::other("platform_unsupported"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-ZOZO, TRS-FR-ANJK, TRS-FR-BHCB: every call and every client uses
    // UTF-8, the module's socket, no user configuration, and an exact target,
    // and a client never sees the variables of an outer tmux.
    #[test]
    fn calls_and_clients_use_the_module_socket() {
        let base = base_args("sock");
        assert_eq!(base, ["-u", "-L", "sock", "-f", "/dev/null"]);
        let client = client_command("/bin/tmux", "sock", "syn-a-t1");
        assert_eq!(client.program, "/bin/tmux");
        assert!(client.args.starts_with(&base));
        assert!(client.args.ends_with(&["attach-session".into(), "-t".into(), "=syn-a-t1".into()]));
        assert_eq!(client.env_remove, ["TMUX", "TMUX_PANE"]);
        assert_eq!(client.env, terminal_env());
    }

    // TRS-FR-TCBS: an output larger than a pipe buffer, as a long capture, is
    // read whole.
    #[test]
    fn a_large_output_is_read_whole() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "yes 0123456789 | head -c 300000"]);
        assert_eq!(run_bounded(command, CALL_TIMEOUT).unwrap().len(), 300_000);
    }

    // TRS-FR-OMYR: a call that does not end in time is stopped and reported.
    #[test]
    fn a_call_past_its_time_is_stopped() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 5"]);
        assert_eq!(run_bounded(command, Duration::from_millis(50)), Err(TmuxError::Timeout));
    }

    // TRS-FR-OMYR: a failure status and a missing program are typed errors.
    #[test]
    fn failures_are_typed() {
        let mut fails = std::process::Command::new("/bin/sh");
        fails.args(["-c", "exit 1"]);
        assert_eq!(run_bounded(fails, CALL_TIMEOUT), Err(TmuxError::Status));
        let missing = std::process::Command::new("/no/such/tmux");
        assert_eq!(run_bounded(missing, CALL_TIMEOUT), Err(TmuxError::Spawn));
        let mut ok = std::process::Command::new("/bin/sh");
        ok.args(["-c", "printf out"]);
        assert_eq!(run_bounded(ok, CALL_TIMEOUT).unwrap(), "out");
    }
}
