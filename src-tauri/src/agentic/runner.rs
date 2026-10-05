//! The outside dependencies — the filesystem and another program — behind
//! traits, and the real implementations of them.

use super::*;

// ---------------------------------------------------------------------------
// The outside dependencies, behind traits
// ---------------------------------------------------------------------------

/// The filesystem, narrowed to the two questions this module asks of it.
pub trait FileProbe: Send + Sync {
    /// Does a regular file exist at this path?
    fn exists(&self, path: &Path) -> bool;
    /// Is it executable by this process? Unix-only in practice; on Windows
    /// existence is the whole test.
    fn is_executable(&self, path: &Path) -> bool;

    /// Does a directory exist at this path? Used only to decide whether a
    /// vendor's own login directory is on this machine (AIC-FR-30); nothing
    /// here opens it or reads a byte of it.
    ///
    /// Defaulted rather than required so the module's existing probe stubs stay
    /// as they are — a test that cares points this at a real scratch directory,
    /// which is a truer fixture than a stubbed boolean anyway.
    fn dir_exists(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

/// What a finished child process said.
#[derive(Clone, Debug, Default)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl CliOutput {
    /// Version banners land on stdout for some CLIs and stderr for others, so
    /// every inspection reads both.
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// Why running a binary did not produce output.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    NotFound,
    NotExecutable,
    /// Exceeded the bounded timeout; the child was terminated (AIC-FR-04).
    TimedOut,
    Failed(String),
}

/// Running a child process, narrowed to the one shape this module needs: a
/// bounded, argument-fixed invocation whose output is captured.
pub trait CliRunner: Send + Sync {
    fn run(&self, path: &Path, args: &[&str], timeout: Duration) -> Result<CliOutput, RunError>;
}

/// Production filesystem probe.
pub struct RealFileProbe;

impl FileProbe for RealFileProbe {
    fn exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    /// FSA-FR-19 exception, deliberate: `path` is a CLI binary the *user*
    /// named (AII-FR-05) or one detection found on `PATH` — `/usr/local/bin/claude`,
    /// somewhere in a Homebrew prefix, anywhere on the machine. It is not
    /// project content and lies outside every allowlisted root by construction,
    /// so routing it through the shared instance would refuse every real
    /// binary. Nothing is read: this asks the OS whether a path is an
    /// executable file and discards everything else.
    #[cfg(unix)]
    fn is_executable(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_executable(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// Production child-process runner.
///
/// The timeout is enforced by polling rather than by blocking on the child:
/// `wait_with_output` consumes the handle, which would leave nothing to kill
/// when the deadline passed. Output is drained on threads so a binary that fills
/// a pipe buffer cannot deadlock against our own wait.
pub struct RealCliRunner;

impl CliRunner for RealCliRunner {
    fn run(&self, path: &Path, args: &[&str], timeout: Duration) -> Result<CliOutput, RunError> {
        use std::io::Read;
        use std::process::{Command, Stdio};

        let mut child = match Command::new(path)
            .args(args)
            // AIC-FR-04: a working directory this module chooses. The open
            // project's path is deliberately not used — nothing about the
            // project reaches a CLI from here.
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return Err(match e.kind() {
                    std::io::ErrorKind::NotFound => RunError::NotFound,
                    std::io::ErrorKind::PermissionDenied => RunError::NotExecutable,
                    _ => RunError::Failed(e.to_string()),
                })
            }
        };

        let mut out_pipe = child.stdout.take();
        let mut err_pipe = child.stderr.take();
        let out_handle = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(pipe) = out_pipe.as_mut() {
                let _ = pipe.read_to_string(&mut buf);
            }
            buf
        });
        let err_handle = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(pipe) = err_pipe.as_mut() {
                let _ = pipe.read_to_string(&mut buf);
            }
            buf
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // AIC-FR-04: terminate rather than wait indefinitely. The
                        // reaping `wait` is what keeps a zombie from being left
                        // behind (AIC-FR-04).
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(RunError::TimedOut);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(RunError::Failed(e.to_string())),
            }
        };

        Ok(CliOutput {
            stdout: out_handle.join().unwrap_or_default(),
            stderr: err_handle.join().unwrap_or_default(),
            success: status.success(),
        })
    }
}

