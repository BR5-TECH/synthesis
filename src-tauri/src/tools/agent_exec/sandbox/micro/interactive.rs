//! EAC-FR-QQMA / EAC-FR-INWE: one process with a terminal in a running VM, as
//! the terminal sessions of `TRS-terminal-sessions.md` hold it.
//!
//! The VM map is locked only while the process starts. The process holds no
//! lock afterwards, so a stop of the VM never waits for it, and the stop ends
//! it.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use microsandbox::ExecEvent;
use tokio::sync::{mpsc, oneshot};

use super::super::InteractiveVmProcess;
use super::BootedVm;
use crate::terminals::process::{ProcessControl, ProcessHandle, OUTPUT_QUEUE_CHUNKS};

/// What the session asks of the process, handed to the task that owns the
/// process's stdin and control.
enum Request {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Kill,
}

struct VmControl {
    requests: mpsc::UnboundedSender<Request>,
}

impl ProcessControl for VmControl {
    fn write(&self, data: &[u8]) -> io::Result<()> {
        self.requests
            .send(Request::Input(data.to_vec()))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the VM process has ended"))
    }

    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.requests
            .send(Request::Resize { cols, rows })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the VM process has ended"))
    }

    fn terminate(&self) {
        let _ = self.requests.send(Request::Kill);
    }
}

/// Starts `process` in `vm` with a terminal and a stdin pipe. Refused with
/// [`super::super::NO_VM_RUNNING`] where the runtime holds no VM of the name.
pub(super) async fn attach(
    vms: &tokio::sync::Mutex<HashMap<String, BootedVm>>,
    vm: &str,
    process: InteractiveVmProcess,
) -> Result<ProcessHandle, String> {
    let guard = vms.lock().await;
    let Some(booted) = guard.get(vm) else {
        return Err(super::super::NO_VM_RUNNING.to_string());
    };
    let mut argv = process.argv.clone();
    if argv.is_empty() {
        return Err("the interactive process names no program".to_string());
    }
    let program = argv.remove(0);
    // EAC-FR-PWLA: the image's `Env` first, and the process's own over it.
    let own: Vec<(String, crate::agentic::SecretString)> = process
        .env
        .iter()
        .map(|(name, value)| (name.clone(), crate::agentic::SecretString::new(value.clone())))
        .collect();
    let env: Vec<(String, String)> = super::super::compose_env(&booted.image_env, &own)
        .into_iter()
        .map(|(name, value)| (name, value.expose().to_string()))
        .collect();
    let workdir = process.workdir.clone();
    let user = process.user.clone();
    let mut handle = booted
        .sandbox
        .exec_stream_with(program, move |o| {
            o.args(argv).cwd(workdir).user(user).envs(env).stdin_pipe().tty(true)
        })
        .await
        .map_err(|e| e.to_string())?;
    drop(guard);

    let stdin = handle.take_stdin();
    let control = handle.control();
    let _ = control.resize(process.rows, process.cols).await;

    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel::<Request>();
    tokio::spawn(async move {
        while let Some(request) = requests_rx.recv().await {
            match request {
                Request::Input(data) => {
                    if let Some(sink) = &stdin {
                        let _ = sink.write(&data).await;
                    }
                }
                Request::Resize { cols, rows } => {
                    let _ = control.resize(rows, cols).await;
                }
                Request::Kill => {
                    let _ = control.kill().await;
                }
            }
        }
    });

    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
    let (exit_tx, exit_rx) = oneshot::channel::<Option<i32>>();
    tokio::spawn(async move {
        let mut code = None;
        while let Some(event) = handle.recv().await {
            let bytes = match event {
                ExecEvent::Stdout(bytes) | ExecEvent::Stderr(bytes) => bytes,
                ExecEvent::Exited { code: exited } => {
                    code = Some(exited);
                    break;
                }
                ExecEvent::Failed(_) => break,
                _ => continue,
            };
            if output_tx.send(bytes.to_vec()).await.is_err() {
                let _ = handle.kill().await;
                break;
            }
        }
        drop(output_tx);
        let _ = exit_tx.send(code);
    });

    Ok(ProcessHandle {
        output: output_rx,
        exit: exit_rx,
        control: Arc::new(VmControl { requests: requests_tx }),
        environment_name: Some(vm.to_string()),
    })
}
