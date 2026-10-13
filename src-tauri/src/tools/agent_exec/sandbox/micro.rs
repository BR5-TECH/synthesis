//! The production binding of the sandbox seam: the embedded microsandbox
//! runtime (EAC-FR-HMBJ). Compiled for Linux and macOS alone, and never under
//! test: a test binds a recording double through the same seam (EAC-FR-VPNW).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use microsandbox::sandbox::PullPolicy;
use microsandbox::{ExecEvent, Image, MicrosandboxError, Sandbox};

use super::super::runtime::{
    BoxFuture, RunEnd, RuntimeError, RuntimeOutcome, StreamCapture, StreamChannel,
};
use super::{
    BootError, BootImage, SandboxFailureCause, SandboxProcess, SandboxRuntime, SandboxSpec,
};

/// How often a running process looks at its cancellation token.
const POLL_SLICE: Duration = Duration::from_millis(100);
/// How long a killed process is given to report its end.
const KILL_GRACE: Duration = Duration::from_secs(10);

/// One booted VM: its handle and the entrypoint of the image it booted from.
struct BootedVm {
    sandbox: Sandbox,
    entrypoint: Vec<String>,
}

#[derive(Default)]
pub struct MicrosandboxRuntime {
    vms: tokio::sync::Mutex<HashMap<String, BootedVm>>,
    /// EAC-FR-HMBJ: set once the runtime is installed from the embedded
    /// archive. A failed install leaves it unset, so the next boot tries again.
    installed: tokio::sync::OnceCell<()>,
}

impl MicrosandboxRuntime {
    /// EAC-FR-HMBJ: install `msb` and `libkrunfw` from the archive compiled
    /// into this executable, where they are not installed already. Nothing is
    /// downloaded at run time. Only a success is kept: an error goes back to
    /// the pool, which logs it each time, and the next call installs again.
    async fn ensure_installed(&self) -> Result<(), String> {
        self.installed
            .get_or_try_init(|| async {
                let config = microsandbox::config::load_persisted_config_or_default()
                    .map_err(|e| e.to_string())?;
                let options = microsandbox::setup::InstallOptions {
                    source: microsandbox::setup::InstallSource::EmbeddedArchive,
                    ..Default::default()
                };
                microsandbox::setup::ensure_runtime(&config, options)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .await
            .map(|_| ())
    }

    /// EAC-FR-HUXS: stop a VM whose boot could not finish, and remove it.
    async fn discard(vm: &str, sandbox: Sandbox) {
        let _ = sandbox.stop().await;
        let _ = Sandbox::remove(vm).await;
    }
}

/// EAC-FR-NCSA: the cause a microsandbox error names.
fn boot_error(error: MicrosandboxError) -> BootError {
    let message = error.to_string();
    match error {
        MicrosandboxError::ImageNotFound(_) | MicrosandboxError::Image(_) => {
            BootError::ImageNotFound(message)
        }
        MicrosandboxError::RuntimeNotInstalled(_)
        | MicrosandboxError::RuntimeIncomplete(_)
        | MicrosandboxError::LibkrunfwNotFound(_) => {
            BootError::Failed(SandboxFailureCause::RuntimeUnavailable, message)
        }
        _ => {
            let lower = message.to_ascii_lowercase();
            let cause = if lower.contains("kvm")
                || lower.contains("hypervisor")
                || lower.contains("virtualization")
                || lower.contains("virtualisation")
            {
                SandboxFailureCause::UnsupportedHost
            } else if lower.contains("memory") || lower.contains("enomem") {
                SandboxFailureCause::InsufficientMemory
            } else {
                SandboxFailureCause::BootFailed
            };
            BootError::Failed(cause, message)
        }
    }
}

impl SandboxRuntime for MicrosandboxRuntime {
    fn boot<'a>(
        &'a self,
        vm: &'a str,
        spec: &'a SandboxSpec,
        image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            self.ensure_installed().await.map_err(|message| {
                BootError::Failed(SandboxFailureCause::RuntimeUnavailable, message)
            })?;
            let (reference, pull) = match image {
                BootImage::Registry(reference) => (reference.as_str(), PullPolicy::IfMissing),
                BootImage::Loaded(reference) => (reference.as_str(), PullPolicy::Never),
            };
            let policy = super::network::policy(spec)
                .map_err(|message| BootError::Failed(SandboxFailureCause::BootFailed, message))?;
            let cpus = u8::try_from(spec.cpus).unwrap_or(u8::MAX).max(1);
            let memory_mib = spec.memory_gib.saturating_mul(1024).max(1024);
            let mut builder = Sandbox::builder(vm)
                .image(reference)
                .pull_policy(pull)
                .cpus(cpus)
                .memory(memory_mib)
                .quiet_logs()
                // EAC-FR-ABHG: a VM left under this name by an earlier process
                // is replaced.
                .replace()
                .network(move |n| n.policy(policy));
            // EAC-FR-BJLI: the directories the launch names, and nothing else.
            for mount in &spec.mounts {
                let source = mount.source.clone();
                let read_only = mount.read_only;
                builder = builder.volume(mount.target.as_str(), move |m| {
                    let m = m.bind(source);
                    if read_only {
                        m.readonly()
                    } else {
                        m
                    }
                });
            }
            let sandbox = builder.create().await.map_err(boot_error)?;
            // EAC-FR-HUXS: the image's entrypoint is its vendor CLI
            // (AVI-FR-04), and the vendor arguments follow it, as `docker run`
            // places them. A boot that cannot read it fails, so a vendor
            // argument is never run as the program. The pool logs the cause.
            let entrypoint = match Image::inspect(reference).await {
                Ok(detail) => detail.config.and_then(|c| c.entrypoint).unwrap_or_default(),
                Err(error) => {
                    Self::discard(vm, sandbox).await;
                    return Err(BootError::Failed(
                        SandboxFailureCause::BootFailed,
                        format!("the image entrypoint could not be read: {error}"),
                    ));
                }
            };
            if entrypoint.is_empty() {
                Self::discard(vm, sandbox).await;
                return Err(BootError::Failed(
                    SandboxFailureCause::BootFailed,
                    "the image names no entrypoint".to_string(),
                ));
            }
            let previous =
                self.vms.lock().await.insert(vm.to_string(), BootedVm { sandbox, entrypoint });
            if let Some(previous) = previous {
                let _ = previous.sandbox.stop().await;
            }
            Ok(())
        })
    }

    fn load_export<'a>(
        &'a self,
        archive: &'a Path,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Err(message) = self.ensure_installed().await {
                return Err(format!("the sandbox runtime is not installed: {message}"));
            }
            if Image::get(reference).await.is_ok() {
                return Ok(());
            }
            Image::load(archive, vec![reference.to_string()])
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn run<'a>(
        &'a self,
        vm: &'a str,
        process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            let vms = self.vms.lock().await;
            let Some(booted) = vms.get(vm) else {
                return Err(RuntimeError::LaunchFailed("the sandbox VM is not running".into()));
            };
            let mut argv: Vec<String> = booted
                .entrypoint
                .iter()
                .cloned()
                .chain(process.vendor_args.iter().cloned())
                .collect();
            if argv.is_empty() {
                return Err(RuntimeError::LaunchFailed("the image names no program".into()));
            }
            let program = argv.remove(0);
            let env: Vec<(String, String)> = process
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.expose().to_string()))
                .collect();
            let workdir = process.workdir.to_string();
            let user = process.user.clone();
            let stdin = process.stdin.to_vec();
            let mut handle = booted
                .sandbox
                .exec_stream_with(program, move |o| {
                    o.args(argv).cwd(workdir).user(user).envs(env).stdin_bytes(stdin).tty(false)
                })
                .await
                .map_err(|e| RuntimeError::LaunchFailed(e.to_string()))?;
            drop(vms);

            let mut stdout = StreamCapture::new(process.stdout_limit, StreamChannel::Stdout);
            let mut stderr = StreamCapture::new(process.stderr_limit, StreamChannel::Stderr);
            let deadline = tokio::time::Instant::now() + process.timeout;
            let mut end = RunEnd::Exited;
            let mut exit_code = None;
            loop {
                if process.cancel.is_cancelled() {
                    end = RunEnd::Cancelled;
                    break;
                }
                let next = tokio::time::timeout_at(
                    deadline.min(tokio::time::Instant::now() + POLL_SLICE),
                    handle.recv(),
                )
                .await;
                match next {
                    Err(_) if tokio::time::Instant::now() >= deadline => {
                        end = RunEnd::TimedOut;
                        break;
                    }
                    Err(_) => continue,
                    Ok(None) => {
                        end = RunEnd::Terminated;
                        break;
                    }
                    Ok(Some(event)) => match event {
                        ExecEvent::Stdout(data) => stdout.push(&data, process.observer),
                        ExecEvent::Stderr(data) => stderr.push(&data, process.observer),
                        ExecEvent::Exited { code } => {
                            exit_code = Some(code);
                            break;
                        }
                        ExecEvent::Failed(error) => {
                            return Err(RuntimeError::LaunchFailed(format!("{error:?}")));
                        }
                        _ => {}
                    },
                }
            }
            if matches!(end, RunEnd::TimedOut | RunEnd::Cancelled) {
                // EAC-FR-ZIYG: the process is killed, and its end is waited for.
                let _ = handle.kill().await;
                let _ = tokio::time::timeout(KILL_GRACE, async {
                    while let Some(event) = handle.recv().await {
                        if matches!(event, ExecEvent::Exited { .. } | ExecEvent::Failed(_)) {
                            break;
                        }
                    }
                })
                .await;
            }
            Ok(RuntimeOutcome {
                end,
                exit_code,
                stdout: stdout.finish(process.observer),
                stderr: stderr.finish(process.observer),
            })
        })
    }

    fn stop<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let booted = self.vms.lock().await.remove(vm);
            if let Some(booted) = booted {
                let _ = booted.sandbox.stop().await;
            }
            let _ = Sandbox::remove(vm).await;
        })
    }
}
