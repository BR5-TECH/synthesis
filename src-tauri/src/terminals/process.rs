//! The process of a terminal session, whatever place it runs in.
//!
//! A host process runs on a pseudo-terminal of this machine. A container, a dev
//! container, or a VM process is reached through the interactive launch of EAC
//! (EAC-FR-CKGX), which hands back the same shape.

use std::io;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

/// How many output chunks wait between the reader and the session's pump. A
/// full queue stops the reader, which is how flow control reaches the process
/// (TRS-FR-VYIB).
pub const OUTPUT_QUEUE_CHUNKS: usize = 16;

/// The size of one read from a pseudo-terminal.
pub const READ_CHUNK_BYTES: usize = 16 * 1024;

/// What a session can do to its running process.
pub trait ProcessControl: Send + Sync {
    /// TRS-FR-AVNH: writes bytes to the process's terminal unchanged.
    fn write(&self, data: &[u8]) -> io::Result<()>;
    /// TRS-FR-UNIL: sets the window size of the process's terminal.
    fn resize(&self, cols: u16, rows: u16) -> io::Result<()>;
    /// TRS-FR-CXSV: ends the process and removes what it runs in. It returns at
    /// once; the end is reported on the exit receiver.
    fn terminate(&self);
}

/// A running process, as a session holds it.
pub struct ProcessHandle {
    /// The process's output, in order. It ends when the output ends.
    pub output: mpsc::Receiver<Vec<u8>>,
    /// The exit code, once the process has ended. `None` where no code is known.
    pub exit: oneshot::Receiver<Option<i32>>,
    pub control: Arc<dyn ProcessControl>,
    /// The container name, the dev container id, or the VM name.
    pub environment_name: Option<String>,
}

/// One program to run on a host pseudo-terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// Variables set over the application's own environment.
    pub env: Vec<(String, String)>,
    /// Variables removed from the application's own environment.
    pub env_remove: Vec<String>,
}

/// TRS-FR-KMWO: the variables every host process gets over the application's.
pub fn terminal_env() -> Vec<(String, String)> {
    vec![
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("COLORTERM".to_string(), "truecolor".to_string()),
    ]
}

#[cfg(unix)]
pub use host::spawn_host;

#[cfg(unix)]
mod host {
    use super::*;
    use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
    use std::io::{Read, Write};
    use std::sync::Mutex;
    use std::time::Duration;

    /// TRS-FR-CXSV: the time between `SIGHUP` and `SIGKILL`.
    const KILL_GRACE: Duration = Duration::from_secs(3);

    struct HostControl {
        master: Mutex<Box<dyn MasterPty + Send>>,
        writer: Mutex<Box<dyn Write + Send>>,
        pid: Option<u32>,
        /// Runs once after the process ended, such as a container removal.
        on_terminate: Option<Box<dyn Fn() + Send + Sync>>,
    }

    impl ProcessControl for HostControl {
        fn write(&self, data: &[u8]) -> io::Result<()> {
            let mut writer = self.writer.lock().map_err(|_| io::Error::other("writer lock"))?;
            writer.write_all(data)?;
            writer.flush()
        }

        fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
            let master = self.master.lock().map_err(|_| io::Error::other("pty lock"))?;
            master
                .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(io::Error::other)
        }

        fn terminate(&self) {
            if let Some(hook) = &self.on_terminate {
                hook();
            }
            let Some(pid) = self.pid else { return };
            let group = pid as libc::pid_t;
            // SAFETY: `killpg` takes plain integers and touches no memory of ours.
            unsafe {
                libc::killpg(group, libc::SIGHUP);
            }
            std::thread::spawn(move || {
                std::thread::sleep(KILL_GRACE);
                // SAFETY: as above. A group that has ended returns ESRCH, which
                // is the expected case and is ignored.
                unsafe {
                    libc::killpg(group, libc::SIGKILL);
                }
            });
        }
    }

    /// Starts `command` on a new pseudo-terminal of `cols` by `rows`.
    ///
    /// `on_terminate` runs on `terminate`, before the signals go out. A Docker
    /// CLI launch uses it to remove its container (EAC-FR-LQJC).
    pub fn spawn_host(
        command: &HostCommand,
        cols: u16,
        rows: u16,
        environment_name: Option<String>,
        on_terminate: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> io::Result<ProcessHandle> {
        let pty = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(io::Error::other)?;
        let mut builder = CommandBuilder::new(&command.program);
        builder.args(&command.args);
        builder.cwd(&command.cwd);
        for name in &command.env_remove {
            builder.env_remove(name);
        }
        for (name, value) in &command.env {
            builder.env(name, value);
        }
        let mut child = pty.slave.spawn_command(builder).map_err(io::Error::other)?;
        drop(pty.slave);
        let pid = child.process_id();
        let mut reader = pty.master.try_clone_reader().map_err(io::Error::other)?;
        let writer = pty.master.take_writer().map_err(io::Error::other)?;

        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
        std::thread::Builder::new().name("terminal-read".into()).spawn(move || {
            let mut buf = vec![0u8; READ_CHUNK_BYTES];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })?;

        let (exit_tx, exit_rx) = oneshot::channel();
        std::thread::Builder::new().name("terminal-wait".into()).spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code() as i32);
            let _ = exit_tx.send(code);
        })?;

        let control = Arc::new(HostControl {
            master: Mutex::new(pty.master),
            writer: Mutex::new(writer),
            pid,
            on_terminate,
        });
        Ok(ProcessHandle { output: output_rx, exit: exit_rx, control, environment_name })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sh(script: &str) -> HostCommand {
            HostCommand {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), script.into()],
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                env: terminal_env(),
                env_remove: vec![],
            }
        }

        async fn read_all(handle: &mut ProcessHandle) -> Vec<u8> {
            let mut out = Vec::new();
            while let Some(chunk) = handle.output.recv().await {
                out.extend(chunk);
            }
            out
        }

        // TRS-FR-YYER, TRS-FR-DVBF: a host process runs on a pseudo-terminal and
        // reports its exit code.
        #[tokio::test]
        async fn runs_a_process_on_a_pseudo_terminal() {
            let mut handle = spawn_host(&sh("test -t 1 && echo tty; exit 3"), 80, 24, None, None)
                .expect("spawn");
            let out = read_all(&mut handle).await;
            assert!(String::from_utf8_lossy(&out).contains("tty"));
            assert_eq!(handle.exit.await.unwrap(), Some(3));
        }

        // TRS-FR-KMWO: the process gets TERM and COLORTERM.
        #[tokio::test]
        async fn sets_the_terminal_variables() {
            let mut handle =
                spawn_host(&sh("echo \"$TERM $COLORTERM\""), 80, 24, None, None).expect("spawn");
            let out = read_all(&mut handle).await;
            assert!(String::from_utf8_lossy(&out).contains("xterm-256color truecolor"));
        }

        // TRS-FR-AVNH, TRS-FR-UNIL: input reaches the process unchanged, and a
        // resize sets the terminal size.
        #[tokio::test]
        async fn input_and_resize_reach_the_process() {
            let mut handle =
                spawn_host(&sh("read line; stty size; echo got:$line"), 80, 24, None, None)
                    .expect("spawn");
            handle.control.resize(100, 40).unwrap();
            handle.control.write(b"hello\n").unwrap();
            let out = String::from_utf8_lossy(&read_all(&mut handle).await).into_owned();
            assert!(out.contains("40 100"), "{out}");
            assert!(out.contains("got:hello"), "{out}");
        }

        // TRS-FR-CXSV: terminate ends the process group and runs the hook.
        #[tokio::test]
        async fn terminate_ends_the_process_and_runs_the_hook() {
            let hook_ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = hook_ran.clone();
            let mut handle = spawn_host(
                &sh("echo ready; sleep 60"),
                80,
                24,
                None,
                Some(Box::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))),
            )
            .expect("spawn");
            let first = handle.output.recv().await.expect("output");
            assert!(String::from_utf8_lossy(&first).contains("ready"));
            handle.control.terminate();
            let _ = read_all(&mut handle).await;
            let _ = handle.exit.await;
            assert!(hook_ran.load(std::sync::atomic::Ordering::SeqCst));
        }
    }
}
