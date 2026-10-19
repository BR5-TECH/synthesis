//! The production binding of the sandbox seam: the embedded microsandbox
//! runtime (EAC-FR-HMBJ). Compiled for Linux and macOS alone. A unit test binds
//! a recording double through the same seam (EAC-FR-VPNW); only the ignored
//! real-runtime probe of EAC-FR-RVQE binds this one under test.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use microsandbox::sandbox::{PullPolicy, StatVirtualization};
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
/// EAC-FR-JRVY: the command that drops the guest's file caches.
const DROP_CACHES: &str = "sync; echo 2 > /proc/sys/vm/drop_caches";
/// EAC-FR-JRVY: how long a cache drop may take before the VM is rebooted.
const REFRESH_BOUND: Duration = Duration::from_secs(30);
/// EAC-FR-SQWO: how long the earlier catalog connections may take to close.
const CLOSE_BOUND: Duration = Duration::from_secs(10);

/// One booted VM: its handle, and the entrypoint and the `Env` of the image it
/// booted from.
struct BootedVm {
    sandbox: Sandbox,
    entrypoint: Vec<String>,
    image_env: Vec<String>,
}

#[derive(Default)]
pub struct MicrosandboxRuntime {
    vms: tokio::sync::Mutex<HashMap<String, BootedVm>>,
    /// EAC-FR-HMBJ: set once the runtime is installed from the embedded
    /// archive. A failed install leaves it unset, so the next boot tries again.
    installed: tokio::sync::OnceCell<()>,
    /// EAC-FR-SQWO: held for a whole reopen, so two launches that fail on the
    /// catalog at one time open one new connection.
    reopening: tokio::sync::Mutex<()>,
    /// EAC-FR-NDDZ: the catalog guard, and how it is started.
    #[cfg(target_os = "macos")]
    guard: tokio::sync::Mutex<Option<super::catalog_guard::CatalogGuard>>,
    #[cfg(target_os = "macos")]
    guard_command: GuardCommand,
}

/// EAC-FR-NDDZ: the command that starts a guard of a catalog. A shipped build
/// starts its own executable, and a real-runtime probe starts the test binary.
#[cfg(target_os = "macos")]
struct GuardCommand(fn(&Path) -> Result<std::process::Command, super::catalog_guard::GuardError>);

#[cfg(target_os = "macos")]
impl Default for GuardCommand {
    fn default() -> Self {
        GuardCommand(super::catalog_guard::CatalogGuard::command)
    }
}

impl MicrosandboxRuntime {
    /// EAC-FR-NDDZ: a runtime whose guard is started by `command`.
    #[cfg(target_os = "macos")]
    pub fn with_guard_command(
        command: fn(&Path) -> Result<std::process::Command, super::catalog_guard::GuardError>,
    ) -> Self {
        MicrosandboxRuntime { guard_command: GuardCommand(command), ..Default::default() }
    }

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
        Self::stop_handle(vm, sandbox).await;
        let _ = Sandbox::remove(vm).await;
    }

    /// Stop the VM of `sandbox`. EAC-FR-SQWO: the handle can hold a catalog
    /// connection that no longer reads, so where its stop fails the VM is
    /// stopped through the current connection.
    async fn stop_handle(vm: &str, sandbox: Sandbox) {
        stop_with_fallback(sandbox.stop(), || async move {
            if let Ok(current) = Sandbox::get(vm).await {
                let _ = current.stop().await;
            }
        })
        .await;
    }
}

#[cfg(target_os = "macos")]
impl MicrosandboxRuntime {
    /// EAC-FR-NDDZ, EAC-FR-PIHS: keep one guard running for the catalog of the
    /// current connection. The lock is held for the whole request, so two
    /// boots at one time start one guard.
    async fn keep_guard(&self, renew: bool) -> super::GuardReport {
        self.keep_guard_of(renew, catalog_path()).await
    }

    /// EAC-FR-NDDZ: keep one guard running for `catalog`, the catalog of the
    /// current connection.
    async fn keep_guard_of(
        &self,
        renew: bool,
        catalog: Option<std::path::PathBuf>,
    ) -> super::GuardReport {
        use super::catalog_guard::{CatalogGuard, LOCK_BOUND};
        use super::{GuardEnd, GuardOutcome, GuardReport};

        let mut slot = self.guard.lock().await;
        let mut ended = None;
        if let Some(guard) = slot.as_mut() {
            let pid = guard.pid();
            match guard.exited() {
                None if !renew => {
                    return GuardReport { ended: None, outcome: GuardOutcome::Running { pid } };
                }
                None => {
                    let exit_code = guard.end();
                    ended = Some(GuardEnd { pid, cause: "replaced", exit_code });
                }
                Some(exit_code) => ended = Some(GuardEnd { pid, cause: "exited", exit_code }),
            }
            *slot = None;
        }
        let failed = |ended, stage, message| GuardReport {
            ended,
            outcome: GuardOutcome::Failed { stage, message },
        };
        let Some(catalog) = catalog else {
            return failed(ended, "catalog", "the runtime has no local catalog".to_string());
        };
        let command = match (self.guard_command.0)(&catalog) {
            Ok(command) => command,
            Err(error) => return failed(ended, error.stage(), error.to_string()),
        };
        match tokio::task::spawn_blocking(move || CatalogGuard::start(command, LOCK_BOUND)).await {
            Ok(Ok(guard)) => {
                let pid = guard.pid();
                *slot = Some(guard);
                GuardReport { ended, outcome: GuardOutcome::Started { pid, catalog } }
            }
            Ok(Err(error)) => failed(ended, error.stage(), error.to_string()),
            Err(error) => failed(ended, "spawn", error.to_string()),
        }
    }
}

/// EAC-FR-NDDZ: the catalog file of the current connection. The runtime keeps
/// it at `db/msb.db` under its home.
#[cfg(target_os = "macos")]
fn catalog_path() -> Option<std::path::PathBuf> {
    let backend = microsandbox::backend::default_backend();
    backend.as_local().map(|local| local.config().home().join("db").join("msb.db"))
}

/// EAC-FR-SQWO: close every pool of the catalog connection of `backend`, so
/// that no connection of this process holds the catalog's shared memory.
/// Returns whether they closed within `bound`. A backend that never opened its
/// catalog fails to open it here, and has nothing to close.
pub(crate) async fn close_catalog(
    backend: &dyn microsandbox::backend::Backend,
    bound: Duration,
) -> bool {
    let Some(local) = backend.as_local() else {
        return true;
    };
    let Ok(pools) = local.db().await else {
        return true;
    };
    tokio::time::timeout(bound, async {
        let _ = pools.read().inner().close_by_ref().await;
        let _ = pools.write().inner().close_by_ref().await;
    })
    .await
    .is_ok()
}

/// EAC-FR-SQWO: whether a runtime message says that the runtime could not
/// read its own catalog. The image and sandbox errors carry a catalog error
/// as text, so the text is what tells it.
pub fn names_catalog_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    // A connection that cannot open reports SQLite's own wording instead.
    lower.contains("database error") || lower.contains("error returned from database")
}

/// EAC-FR-SQWO: run `first`, and where it fails, run `fallback`. Returns
/// whether the fallback ran.
async fn stop_with_fallback<E, G>(
    first: impl std::future::Future<Output = Result<(), E>>,
    fallback: impl FnOnce() -> G,
) -> bool
where
    G: std::future::Future<Output = ()>,
{
    if first.await.is_ok() {
        return false;
    }
    fallback().await;
    true
}

/// EAC-FR-NCSA / EAC-FR-SQWO: the cause of a load that failed.
fn load_error(error: MicrosandboxError) -> BootError {
    if is_catalog_error(&error) {
        BootError::Catalog(error.to_string())
    } else {
        BootError::Failed(SandboxFailureCause::ImageLoadFailed, error.to_string())
    }
}

/// EAC-FR-SQWO: whether a microsandbox error is a catalog error.
fn is_catalog_error(error: &MicrosandboxError) -> bool {
    matches!(error, MicrosandboxError::Database(_)) || names_catalog_failure(&error.to_string())
}

/// EAC-FR-NCSA: the cause a microsandbox error names.
fn boot_error(error: MicrosandboxError) -> BootError {
    let message = error.to_string();
    if is_catalog_error(&error) {
        return BootError::Catalog(message);
    }
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
            // EAC-FR-QDVN: the root disk and the write cap of each writable
            // mount. The runtime otherwise caps both at 4 GiB.
            let disk_mib = spec.disk_gib.saturating_mul(1024).max(1024);
            let mut builder = Sandbox::builder(vm)
                .image(reference)
                .pull_policy(pull)
                .cpus(cpus)
                .memory(memory_mib)
                .root_disk(disk_mib)
                .quiet_logs()
                // EAC-FR-ABHG: a VM left under this name by an earlier process
                // is replaced.
                .replace()
                .network(move |n| n.policy(policy));
            // EAC-FR-BJLI: the mounts the launch names, and nothing else.
            // EAC-FR-KTWO: each shows the real host owner and mode, so the host
            // user the process runs as can write its own working copy, and the
            // runtime writes no ownership attribute on a host file.
            for mount in &spec.mounts {
                let source = mount.source.clone();
                let read_only = mount.read_only;
                builder = builder.volume(mount.target.as_str(), move |m| {
                    let m = m.bind(source).stat_virtualization(StatVirtualization::Off);
                    if read_only {
                        m.readonly()
                    } else {
                        m.quota(disk_mib)
                    }
                });
            }
            let sandbox = builder.create().await.map_err(boot_error)?;
            // EAC-FR-HUXS: the image's entrypoint is its vendor CLI
            // (AVI-FR-04), and the vendor arguments follow it, as `docker run`
            // places them. A boot that cannot read it fails, so a vendor
            // argument is never run as the program. The pool logs the cause.
            let (entrypoint, image_env) = match Image::inspect(reference).await {
                Ok(detail) => detail
                    .config
                    .map(|c| (c.entrypoint.unwrap_or_default(), c.env))
                    .unwrap_or_default(),
                Err(error) => {
                    Self::discard(vm, sandbox).await;
                    let message = format!("the image entrypoint could not be read: {error}");
                    if is_catalog_error(&error) {
                        return Err(BootError::Catalog(message));
                    }
                    return Err(BootError::Failed(SandboxFailureCause::BootFailed, message));
                }
            };
            if entrypoint.is_empty() {
                Self::discard(vm, sandbox).await;
                return Err(BootError::Failed(
                    SandboxFailureCause::BootFailed,
                    "the image names no entrypoint".to_string(),
                ));
            }
            let previous = self
                .vms
                .lock()
                .await
                .insert(vm.to_string(), BootedVm { sandbox, entrypoint, image_env });
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
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            if let Err(message) = self.ensure_installed().await {
                return Err(BootError::Failed(
                    SandboxFailureCause::RuntimeUnavailable,
                    format!("the sandbox runtime is not installed: {message}"),
                ));
            }
            if Image::get(reference).await.is_ok() {
                return Ok(());
            }
            Image::load(archive, vec![reference.to_string()]).await.map(|_| ()).map_err(load_error)
        })
    }

    fn reopen_catalog(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            // EAC-FR-SQWO: where another launch opened a new connection while
            // this one waited, this launch uses that one.
            let failed = microsandbox::backend::default_backend();
            let _reopening = self.reopening.lock().await;
            let earlier = microsandbox::backend::default_backend();
            if !std::sync::Arc::ptr_eq(&failed, &earlier) {
                return Ok(());
            }
            // The earlier connections close first. A new one beside them
            // attaches to the shared memory they hold, and reads the same
            // damage.
            let closed = close_catalog(earlier.as_ref(), CLOSE_BOUND).await;
            // A new backend opens its own connection to the catalog. Every
            // later call of this process goes through it. A running process
            // reaches its VM through the VM's own socket and not through the
            // catalog, and `stop` falls back to the new backend where the VM
            // handle's own one fails.
            let backend = microsandbox::backend::LocalBackend::new().await.map_err(|e| {
                if closed {
                    e.to_string()
                } else {
                    format!(
                        "{e} (the earlier catalog connections did not close in {} seconds)",
                        CLOSE_BOUND.as_secs()
                    )
                }
            })?;
            let backend: std::sync::Arc<dyn microsandbox::backend::Backend> =
                std::sync::Arc::new(backend);
            microsandbox::backend::set_default_backend(backend);
            Ok(())
        })
    }

    #[cfg(target_os = "macos")]
    fn guard_catalog(&self, renew: bool) -> BoxFuture<'_, super::GuardReport> {
        Box::pin(async move { self.keep_guard(renew).await })
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
            // EAC-FR-PWLA: the image's `Env` first, and the launch's over it.
            let env: Vec<(String, String)> = super::compose_env(&booted.image_env, &process.env)
                .into_iter()
                .map(|(name, value)| (name, value.expose().to_string()))
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

    fn refresh<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let vms = self.vms.lock().await;
            let Some(booted) = vms.get(vm) else {
                return Err("the sandbox VM is not running".to_string());
            };
            // EAC-FR-JRVY: as root, a command of this module and never of the
            // agent. It frees the entries the guest keeps for its mounts.
            let mut handle = booted
                .sandbox
                .exec_stream_with("/bin/sh", |o| {
                    o.args(["-c", DROP_CACHES]).user("0:0").stdin_null().tty(false)
                })
                .await
                .map_err(|e| e.to_string())?;
            drop(vms);
            let ended = tokio::time::timeout(REFRESH_BOUND, async {
                while let Some(event) = handle.recv().await {
                    match event {
                        ExecEvent::Exited { code } => return Some(code),
                        ExecEvent::Failed(_) => return None,
                        _ => {}
                    }
                }
                None
            })
            .await;
            match ended {
                Ok(Some(0)) => Ok(()),
                Ok(Some(code)) => Err(format!("the cache drop exited with {code}")),
                Ok(None) => Err("the cache drop did not run".to_string()),
                Err(_) => {
                    let _ = handle.kill().await;
                    Err("the cache drop did not end in time".to_string())
                }
            }
        })
    }

    fn stop<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let booted = self.vms.lock().await.remove(vm);
            if let Some(booted) = booted {
                Self::stop_handle(vm, booted.sandbox).await;
            }
            let _ = Sandbox::remove(vm).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use microsandbox_image::ImageError;

    /// The error the runtime gave when another process replaced its catalog
    /// index: a catalog error inside an image error.
    fn replaced_catalog() -> MicrosandboxError {
        MicrosandboxError::Image(ImageError::Io(std::io::Error::other(
            "database error: Connection Error: error returned from database: (code: 26) file is not a database",
        )))
    }

    // EAC-FR-SQWO: the message of a catalog that cannot be read names a
    // catalog failure, also inside an image error, and no other message does.
    #[test]
    fn a_catalog_message_names_a_catalog_failure() {
        assert!(names_catalog_failure(&replaced_catalog().to_string()));
        assert!(names_catalog_failure("Database Error: locked"));
        assert!(names_catalog_failure(
            "connect to /h/db/msb.db: error returned from database: (code: 26) file is not a database"
        ));
        assert!(!names_catalog_failure("image not cached: claude-code:latest"));
        assert!(!names_catalog_failure("failed to create bind target"));
    }

    // EAC-FR-SQWO, EAC-FR-NCSA: the error of a replaced catalog index is a
    // catalog failure at a boot, though it comes as an image error, and an
    // image the runtime does not hold keeps its own kind.
    #[test]
    fn boot_errors_tell_a_catalog_failure_from_a_missing_image() {
        assert!(matches!(boot_error(replaced_catalog()), BootError::Catalog(_)));
        let custom = MicrosandboxError::Custom("database error: file is not a database".into());
        assert!(matches!(boot_error(custom), BootError::Catalog(_)));
        let missing = MicrosandboxError::ImageNotFound("claude-code:latest".into());
        assert!(matches!(boot_error(missing), BootError::ImageNotFound(_)));
        let not_cached = MicrosandboxError::Image(ImageError::NotCached {
            reference: "claude-code:latest".into(),
        });
        assert!(matches!(boot_error(not_cached), BootError::ImageNotFound(_)));
    }

    // EAC-FR-SQWO, EAC-FR-NCSA: a load that fails on the catalog is a catalog
    // failure, and any other failed load is `image_load_failed`.
    #[test]
    fn load_errors_tell_a_catalog_failure_from_a_load_failure() {
        assert!(matches!(load_error(replaced_catalog()), BootError::Catalog(_)));
        let other = MicrosandboxError::Image(ImageError::ManifestParse("bad".into()));
        match load_error(other) {
            BootError::Failed(cause, message) => {
                assert_eq!(cause, SandboxFailureCause::ImageLoadFailed);
                assert!(message.contains("bad"), "{message}");
            }
            other => panic!("expected a load failure, found {other:?}"),
        }
    }

    // EAC-FR-SQWO: a stop that fails through the earlier connection runs the
    // stop through the current one, and a stop that works runs nothing more.
    #[test]
    fn a_failed_stop_falls_back_to_the_current_connection() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let ran = std::cell::Cell::new(false);
        let fell_back =
            rt.block_on(stop_with_fallback(async { Err::<(), _>("stale") }, || async {
                ran.set(true);
            }));
        assert!(fell_back && ran.get());

        let ran = std::cell::Cell::new(false);
        let fell_back = rt.block_on(stop_with_fallback(async { Ok::<(), ()>(()) }, || async {
            ran.set(true);
        }));
        assert!(!fell_back && !ran.get());
    }

    /// EAC-FR-NDDZ, EAC-FR-PIHS: the runtime keeps one guard per catalog
    /// connection. The guards here are the test binary, which holds the lock
    /// as the shipped executable does.
    #[cfg(target_os = "macos")]
    mod guard {
        use super::super::super::catalog_guard::tests::{catalog, test_command};
        use super::super::super::{GuardEnd, GuardOutcome};
        use super::*;

        fn guarded() -> MicrosandboxRuntime {
            MicrosandboxRuntime::with_guard_command(|path| Ok(test_command(path)))
        }

        fn rt() -> tokio::runtime::Runtime {
            tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime")
        }

        fn started(outcome: &GuardOutcome) -> u32 {
            match outcome {
                GuardOutcome::Started { pid, .. } => *pid,
                other => panic!("a guard started: {other:?}"),
            }
        }

        // EAC-FR-NDDZ: a running guard is kept, and a new catalog connection
        // ends it and starts a new one.
        #[test]
        fn a_running_guard_is_kept_and_a_renewal_replaces_it() {
            let (_dir, path) = catalog();
            let runtime = guarded();
            rt().block_on(async {
                let first = runtime.keep_guard_of(false, Some(path.clone())).await;
                assert_eq!(first.ended, None);
                let earlier = started(&first.outcome);
                let kept = runtime.keep_guard_of(false, Some(path.clone())).await;
                assert_eq!(kept.outcome, GuardOutcome::Running { pid: earlier });
                assert_eq!(kept.ended, None);
                let renewed = runtime.keep_guard_of(true, Some(path.clone())).await;
                let later = started(&renewed.outcome);
                assert_ne!(earlier, later);
                let ended = renewed.ended.expect("the earlier guard ended");
                assert_eq!((ended.pid, ended.cause), (earlier, "replaced"));
            });
        }

        // EAC-FR-NDDZ, EAC-FR-FUNB: a guard that ended on its own is reported
        // with its exit code, and a new one starts.
        #[test]
        fn a_guard_that_ended_on_its_own_is_started_again() {
            let (_dir, path) = catalog();
            let runtime = guarded();
            rt().block_on(async {
                let earlier =
                    started(&runtime.keep_guard_of(false, Some(path.clone())).await.outcome);
                let code = runtime.guard.lock().await.as_mut().expect("a guard").reap();
                assert_eq!(code, Some(0), "a guard whose input closed ends cleanly");
                let next = runtime.keep_guard_of(false, Some(path.clone())).await;
                assert_eq!(
                    next.ended,
                    Some(GuardEnd { pid: earlier, cause: "exited", exit_code: Some(0) })
                );
                started(&next.outcome);
            });
        }

        // EAC-FR-PIHS: a guard that could not start is reported with its
        // stage, and the next request starts one.
        #[test]
        fn a_guard_that_could_not_start_is_started_at_the_next_request() {
            let (dir, path) = catalog();
            let missing = dir.path().canonicalize().expect("canonical").join("absent.db");
            let runtime = guarded();
            rt().block_on(async {
                let failed = runtime.keep_guard_of(false, Some(missing)).await;
                assert!(
                    matches!(failed.outcome, GuardOutcome::Failed { stage: "open", .. }),
                    "{failed:?}"
                );
                let none = runtime.keep_guard_of(false, None).await;
                assert!(
                    matches!(none.outcome, GuardOutcome::Failed { stage: "catalog", .. }),
                    "{none:?}"
                );
                started(&runtime.keep_guard_of(false, Some(path)).await.outcome);
            });
        }

        // EAC-FR-NDDZ: two requests at one time start one guard.
        #[test]
        fn two_requests_at_one_time_start_one_guard() {
            let (_dir, path) = catalog();
            let runtime = guarded();
            let (one, two) = rt().block_on(async {
                tokio::join!(
                    runtime.keep_guard_of(false, Some(path.clone())),
                    runtime.keep_guard_of(false, Some(path.clone()))
                )
            });
            let outcomes = [one.outcome, two.outcome];
            let starts = outcomes.iter().filter(|o| matches!(o, GuardOutcome::Started { .. }));
            let runs = outcomes.iter().filter(|o| matches!(o, GuardOutcome::Running { .. }));
            assert_eq!((starts.count(), runs.count()), (1, 1), "{outcomes:?}");
        }
    }
}
