//! The sandbox runtime (EAC-FR-SNMS through EAC-FR-VPNW).
//!
//! Where the author's isolation is Microsandbox and a launch belongs to a
//! graduation run, the agent runs as a process inside a micro VM that the run's
//! queue shares. This module holds what is the same under test and in a
//! shipped build: the seam a VM is reached through ([`SandboxRuntime`]), the
//! one-VM-per-scope pool with its reuse rule (EAC-FR-DWBS, EAC-FR-UEFI), and the
//! image sourcing with its export fallback (EAC-FR-QULL, EAC-FR-ASNS,
//! EAC-FR-IGBY). The production binding of the seam is `micro`, compiled for
//! Linux and macOS alone (EAC-FR-HMBJ).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::runtime::{
    BoxFuture, CancellationToken, RunEnd, RuntimeError, RuntimeOutcome, StreamObserver,
};
use super::TurnKind;
use crate::agentic::SecretString;
use crate::fs::FsAccess;
use crate::isolation::{AllowedHost, SandboxSettings};
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

pub mod images;
// Compiled under test too, because the real-runtime probe of EAC-FR-RVQE binds
// it. A test build still uses `NoSandbox` as its production runtime.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod micro;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod network;

/// The domains every record of this module carries (EAC-FR-FUNB).
const DOMAINS: [Domain; 2] = [Domain::Ai, Domain::Backend];

/// EAC-FR-IGBY: the subdirectory of `app_data_dir()` exports are kept in.
pub const EXPORT_CACHE_DIRNAME: &str = "sandbox-images";

/// EAC-FR-MNEH: the bound on one boot, from its first pull to a running VM.
/// Longer than the bound of one export (`runtime::export::EXPORT_TIMEOUT`).
pub const BOOT_BOUND: Duration = Duration::from_secs(45 * 60);
const _: () = assert!(BOOT_BOUND.as_secs() > super::runtime::export::EXPORT_TIMEOUT.as_secs());

/// EAC-FR-MIQG: how often a wait for the VM or a boot looks at the
/// cancellation token.
const CANCEL_POLL: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// The shapes (EAC contract surface, "The sandbox runtime")
// ---------------------------------------------------------------------------

/// EAC-FR-SNMS: the run queue whose VM a sandbox launch shares.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SandboxScope {
    /// A stream id, or `worktree:<path>` for a direct run outside a stream.
    pub queue_key: String,
}

/// One directory a VM mounts (EAC-FR-BJLI).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SandboxMount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

/// What a VM's egress is (EAC-FR-CGCE, EAC-FR-DMYB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxNetwork {
    pub block_local_network: bool,
    /// Sorted and without repeats.
    pub allowed: Vec<AllowedHost>,
}

/// Everything a VM boots with (EAC-FR-UEFI). Two launches whose specs are
/// equal can share one VM; any difference boots a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxSpec {
    /// The project's configured reference (EAC-FR-38).
    pub image: String,
    /// EAC-FR-KTWO: `<uid>:<gid>`, the host user's. Every mount shows the real
    /// host owner, and the process runs as this user.
    pub owner: String,
    pub cpus: u32,
    pub memory_gib: u32,
    /// EAC-FR-QDVN: the root disk, and the write cap of each writable mount.
    pub disk_gib: u32,
    /// Sorted, so an order of composition does not count as a difference.
    pub mounts: Vec<SandboxMount>,
    pub network: SandboxNetwork,
}

impl SandboxSpec {
    pub fn new(
        image: String,
        owner: String,
        settings: &SandboxSettings,
        mut mounts: Vec<SandboxMount>,
    ) -> SandboxSpec {
        mounts.sort();
        mounts.dedup();
        SandboxSpec {
            image,
            owner,
            cpus: settings.cpus,
            memory_gib: settings.memory_gib,
            disk_gib: settings.disk_gib,
            mounts,
            network: SandboxNetwork {
                block_local_network: settings.block_local_network,
                allowed: settings.allowed.clone(),
            },
        }
    }
}

/// Which image a boot uses (EAC-FR-QULL, EAC-FR-ASNS).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootImage {
    /// The configured reference, pulled through a registry where the sandbox
    /// runtime does not hold it.
    Registry(String),
    /// A loaded export, under the reference [`images::export_reference`]
    /// derives from its image ID. Never pulled.
    Loaded(String),
}

/// Why a boot failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootError {
    /// The image could not be found or fetched. The export fallback applies.
    ImageNotFound(String),
    /// Any other failure, with the cause it names (EAC-FR-NCSA).
    Failed(SandboxFailureCause, String),
}

/// EAC-FR-NCSA: why a VM could not be established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxFailureCause {
    UnsupportedHost,
    RuntimeUnavailable,
    ImageUnavailable,
    ImageExportFailed,
    InsufficientMemory,
    BootFailed,
}

impl SandboxFailureCause {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxFailureCause::UnsupportedHost => "unsupported_host",
            SandboxFailureCause::RuntimeUnavailable => "runtime_unavailable",
            SandboxFailureCause::ImageUnavailable => "image_unavailable",
            SandboxFailureCause::ImageExportFailed => "image_export_failed",
            SandboxFailureCause::InsufficientMemory => "insufficient_memory",
            SandboxFailureCause::BootFailed => "boot_failed",
        }
    }

    /// GXD-FR-QUCP: the cause in words, for the run's detail.
    pub fn sentence(self) -> &'static str {
        match self {
            SandboxFailureCause::UnsupportedHost => {
                "this machine cannot run Microsandbox, because it has no usable hardware virtualisation"
            }
            SandboxFailureCause::RuntimeUnavailable => {
                "the sandbox runtime could not be installed or started"
            }
            SandboxFailureCause::ImageUnavailable => {
                "the image is in no registry and in no verified Docker backend"
            }
            SandboxFailureCause::ImageExportFailed => {
                "the image could not be exported from Docker"
            }
            SandboxFailureCause::InsufficientMemory => {
                "there is not enough memory for the configured RAM"
            }
            SandboxFailureCause::BootFailed => "the VM did not boot",
        }
    }
}

/// EAC-FR-NCSA: a VM that could not be established, with the runtime's own
/// message. The message goes to the log alone (GXD-FR-WJOW).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxFailure {
    pub cause: SandboxFailureCause,
    pub message: String,
}

/// One process in a VM (EAC-FR-ZIYG).
pub struct SandboxProcess<'a> {
    /// The vendor's own argument vector. The image's entrypoint runs it.
    pub vendor_args: &'a [String],
    pub workdir: &'a str,
    /// `<uid>:<gid>`, the host user's.
    pub user: String,
    /// Every variable, with its value, on this process alone.
    pub env: Vec<(String, SecretString)>,
    pub stdin: &'a [u8],
    pub timeout: Duration,
    pub cancel: CancellationToken,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
    pub observer: Option<&'a dyn StreamObserver>,
}

/// EAC-FR-PWLA: the environment of a sandbox process. The image's `Env` comes
/// first, and a variable of the launch replaces an image variable of the same
/// name, as `docker run` composes them. An image entry with no `=` names no
/// value and is left out.
pub fn compose_env(
    image_env: &[String],
    process_env: &[(String, SecretString)],
) -> Vec<(String, SecretString)> {
    let mut composed: Vec<(String, SecretString)> = Vec::new();
    for entry in image_env {
        let Some((name, value)) = entry.split_once('=') else {
            continue;
        };
        composed.retain(|(existing, _)| existing != name);
        composed.push((name.to_string(), SecretString::new(value.to_string())));
    }
    for (name, value) in process_env {
        composed.retain(|(existing, _)| existing != name);
        composed.push((name.clone(), SecretString::new(value.expose().to_string())));
    }
    composed
}

/// EAC-FR-VPNW: every VM operation goes through this seam.
pub trait SandboxRuntime: Send + Sync {
    /// Boot `vm` with `spec` from `image`, replacing a VM left under that name.
    fn boot<'a>(
        &'a self,
        vm: &'a str,
        spec: &'a SandboxSpec,
        image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>>;

    /// Load an export under `reference`, where the sandbox runtime does not
    /// hold that reference already.
    fn load_export<'a>(
        &'a self,
        archive: &'a Path,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<(), String>>;

    /// Run one process in the running `vm` (EAC-FR-ZIYG).
    fn run<'a>(
        &'a self,
        vm: &'a str,
        process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>>;

    /// EAC-FR-JRVY: drop the guest's file caches of the running `vm`, so the
    /// next process sees every change the host made to a mount.
    fn refresh<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, Result<(), String>>;

    /// Stop and remove `vm`. A VM that is not there is not an error.
    fn stop<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, ()>;
}

/// EAC-FR-QULL: the verified Docker backend, as the export fallback reads it.
pub trait ImageExporter: Send + Sync {
    /// The image ID (`sha256:<hex>`) the backend holds for `reference`.
    fn image_id<'a>(&'a self, reference: &'a str) -> BoxFuture<'a, Result<String, String>>;

    /// Write the `docker save` archive of `reference` to `destination`. The
    /// cache names the image by its ID, so the archive carries no tag
    /// (EAC-FR-ASNS).
    fn export<'a>(
        &'a self,
        reference: &'a str,
        destination: &'a Path,
        fs: &'a FsAccess,
    ) -> BoxFuture<'a, Result<(), String>>;
}

// ---------------------------------------------------------------------------
// The pool (EAC-FR-DWBS, EAC-FR-UEFI)
// ---------------------------------------------------------------------------

/// What a booted VM booted with.
#[derive(Clone)]
struct Booted {
    spec: SandboxSpec,
    /// EAC-FR-HXDN: the device and the inode of each read/write mount source,
    /// in the order of `spec.mounts`.
    identities: Vec<Option<(u64, u64)>>,
    /// The stop epoch it booted in (EAC-FR-ABHG).
    epoch: u64,
}

/// One VM's slot. Held for the whole of a launch, so launches of one scope run
/// one at a time (EAC-FR-DWBS).
type Slot = Arc<tokio::sync::Mutex<Option<Booted>>>;

/// EAC-FR-DWBS: at most one VM per scope, and the record of what it booted
/// with.
pub struct SandboxPool {
    runtime: Arc<dyn SandboxRuntime>,
    slots: Mutex<HashMap<String, Slot>>,
    /// EAC-FR-ABHG: advanced by every stop of all VMs. A VM booted in an
    /// earlier epoch was stopped, also where a launch held its slot then, so
    /// it is never reused.
    epoch: std::sync::atomic::AtomicU64,
    /// EAC-FR-MNEH: the bound on one boot.
    boot_bound: Duration,
}

/// What one sandbox launch needs beyond the process.
pub struct SandboxLaunch<'a> {
    pub vm: &'a str,
    pub spec: &'a SandboxSpec,
    /// EAC-FR-HXDN: a `review` or `merge_review` launch never reuses a VM.
    pub turn_kind: TurnKind,
    pub settings: &'a SandboxSettings,
    /// The verified Docker backend, where there is one (EAC-FR-QULL).
    pub exporter: Option<Arc<dyn ImageExporter>>,
    pub fs: &'a FsAccess,
    /// EAC-FR-IGBY: where exports are kept.
    pub export_root: PathBuf,
}

/// How a sandbox launch failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxLaunchError {
    /// The VM could not be established (EAC-FR-NCSA).
    Sandbox(SandboxFailure),
    /// The VM is there and the process could not run in it.
    Runtime(RuntimeError),
}

impl SandboxPool {
    pub fn new(runtime: Arc<dyn SandboxRuntime>) -> SandboxPool {
        SandboxPool {
            runtime,
            slots: Mutex::new(HashMap::new()),
            epoch: std::sync::atomic::AtomicU64::new(0),
            boot_bound: BOOT_BOUND,
        }
    }

    /// A pool whose boots have a bound other than [`BOOT_BOUND`], for a test.
    #[cfg(test)]
    pub fn with_boot_bound(mut self, bound: Duration) -> SandboxPool {
        self.boot_bound = bound;
        self
    }

    fn slot(&self, vm: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        slots.entry(vm.to_string()).or_default().clone()
    }

    /// EAC-FR-UEFI / EAC-FR-ZIYG: make sure the scope's VM runs with `spec`,
    /// and run one process in it.
    pub async fn launch<S>(
        &self,
        sink: &S,
        launch: SandboxLaunch<'_>,
        process: SandboxProcess<'_>,
    ) -> Result<RuntimeOutcome, SandboxLaunchError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        if let Some(reason) = launch.settings.unsupported_reason {
            return Err(self.failed(
                sink,
                launch.vm,
                &launch.spec.image,
                SandboxFailureCause::UnsupportedHost,
                reason.as_str().to_string(),
            ));
        }
        // EAC-FR-MIQG: the wait for the scope's VM ends when the run is
        // cancelled. A launch that holds the slot ends on its own cancellation.
        let cancel = process.cancel.clone();
        let slot = self.slot(launch.vm);
        let mut booted = tokio::select! {
            biased;
            () = until_cancelled(&cancel) => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox launch cancelled while it waited for the VM",
                    log_fields! { "vm" => launch.vm },
                );
                return Ok(cancelled_outcome());
            }
            booted = slot.lock() => booted,
        };
        let epoch = self.epoch.load(std::sync::atomic::Ordering::SeqCst);
        let identities = source_identities(launch.spec);
        let mut decision = reuse(booted.as_ref(), &launch, &identities, epoch);
        if matches!(decision, Reuse::Yes) {
            // EAC-FR-JRVY: a reused VM sees what the host changed since the
            // previous launch, or it is rebooted.
            if let Err(message) = self.runtime.refresh(launch.vm).await {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM caches could not be dropped",
                    log_fields! { "vm" => launch.vm, "message" => message.as_str() },
                );
                decision = Reuse::No("refresh_failed");
            }
        }
        match decision {
            Reuse::Yes => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM reused",
                    log_fields! { "vm" => launch.vm, "image" => launch.spec.image.as_str() },
                );
            }
            decision => {
                if let Reuse::No(reason) = decision {
                    logging::log_info(
                        sink,
                        &BUFFER,
                        &DOMAINS,
                        "sandbox VM rebooted for a changed launch",
                        log_fields! {
                            "vm" => launch.vm,
                            "image" => launch.spec.image.as_str(),
                            "reason" => reason,
                            "turn_kind" => launch.turn_kind.as_str(),
                        },
                    );
                    self.runtime.stop(launch.vm).await;
                }
                *booted = None;
                self.bounded_boot(sink, &launch, &cancel).await?;
                if cancel.is_cancelled() {
                    // EAC-FR-MIQG: the boot ended as the run was cancelled.
                    // The VM is not recorded as booted, so it is stopped and
                    // the next launch boots a new one.
                    logging::log_info(
                        sink,
                        &BUFFER,
                        &DOMAINS,
                        "sandbox VM stopped, because the run was cancelled as it booted",
                        log_fields! { "vm" => launch.vm, "image" => launch.spec.image.as_str() },
                    );
                    self.runtime.stop(launch.vm).await;
                    return Ok(cancelled_outcome());
                }
                *booted = Some(Booted { spec: launch.spec.clone(), identities, epoch });
            }
        }
        let outcome = self.runtime.run(launch.vm, process).await;
        if matches!(outcome, Err(RuntimeError::LaunchFailed(_) | RuntimeError::Unavailable)) {
            // The process could not start in the VM, so the VM is not trusted
            // again: the next launch of this scope boots a new one.
            logging::log_warn(
                sink,
                &BUFFER,
                &DOMAINS,
                "sandbox VM could not start the process and will be rebooted",
                log_fields! { "vm" => launch.vm },
            );
            *booted = None;
            self.runtime.stop(launch.vm).await;
        }
        drop(booted);
        outcome.map_err(SandboxLaunchError::Runtime)
    }

    /// EAC-FR-MIQG / EAC-FR-MNEH: [`Self::boot`], abandoned when the run is
    /// cancelled or when it does not end within the boot bound. An abandoned
    /// boot stops what it left of the VM. A cancelled boot returns `Ok`, and
    /// the caller reads the token.
    async fn bounded_boot<S>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
        cancel: &CancellationToken,
    ) -> Result<(), SandboxLaunchError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let bounded = tokio::time::timeout(self.boot_bound, self.boot(sink, launch));
        tokio::select! {
            biased;
            () = until_cancelled(cancel) => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM boot cancelled",
                    log_fields! { "vm" => launch.vm, "image" => launch.spec.image.as_str() },
                );
                Ok(())
            }
            result = bounded => match result {
                Ok(result) => result,
                Err(_) => {
                    self.runtime.stop(launch.vm).await;
                    Err(self.failed(
                        sink,
                        launch.vm,
                        &launch.spec.image,
                        SandboxFailureCause::BootFailed,
                        format!(
                            "the VM did not boot in {} minutes",
                            self.boot_bound.as_secs() / 60
                        ),
                    ))
                }
            },
        }
    }

    /// EAC-FR-QULL / EAC-FR-ASNS / EAC-FR-IGBY: boot from the export of the
    /// image the verified Docker backend holds, and from a registry where no
    /// backend holds it.
    async fn boot<S>(&self, sink: &S, launch: &SandboxLaunch<'_>) -> Result<(), SandboxLaunchError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let image = launch.spec.image.as_str();
        let local = match launch.exporter.as_ref() {
            Some(exporter) => match exporter.image_id(image).await {
                Ok(id) => Some((exporter, id)),
                Err(message) => {
                    logging::log_info(
                        sink,
                        &BUFFER,
                        &DOMAINS,
                        "sandbox image is not in the Docker backend, booting from a registry",
                        log_fields! {
                            "vm" => launch.vm,
                            "image" => image,
                            "message" => message.as_str(),
                        },
                    );
                    None
                }
            },
            None => None,
        };
        let Some((exporter, id)) = local else {
            return self.boot_from_registry(sink, launch).await;
        };
        let cache = images::ExportCache::new(launch.fs, launch.export_root.clone());
        let (archive, hit) =
            cache.ensure(image, &id, exporter.as_ref()).await.map_err(|message| {
                self.failed(sink, launch.vm, image, SandboxFailureCause::ImageExportFailed, message)
            })?;
        logging::log_info(
            sink,
            &BUFFER,
            &DOMAINS,
            if hit {
                "sandbox image export reused from the cache"
            } else {
                "sandbox image exported"
            },
            log_fields! { "vm" => launch.vm, "image" => image, "image_id" => id.as_str() },
        );
        let loaded = images::export_reference(image, &id);
        self.runtime.load_export(&archive, &loaded).await.map_err(|message| {
            self.failed(sink, launch.vm, image, SandboxFailureCause::ImageExportFailed, message)
        })?;
        match self.runtime.boot(launch.vm, launch.spec, &BootImage::Loaded(loaded)).await {
            Ok(()) => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM booted from the exported image",
                    log_fields! {
                        "vm" => launch.vm,
                        "image" => image,
                        "image_id" => id.as_str(),
                        "cpus" => launch.spec.cpus,
                        "memory_gib" => launch.spec.memory_gib,
                    },
                );
                Ok(())
            }
            Err(BootError::ImageNotFound(message)) => Err(self.failed(
                sink,
                launch.vm,
                image,
                SandboxFailureCause::ImageExportFailed,
                message,
            )),
            Err(BootError::Failed(cause, message)) => {
                Err(self.failed(sink, launch.vm, image, cause, message))
            }
        }
    }

    /// EAC-FR-QULL: boot from the configured reference, which the sandbox
    /// runtime pulls through a registry where it does not hold it.
    async fn boot_from_registry<S>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
    ) -> Result<(), SandboxLaunchError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let image = launch.spec.image.as_str();
        let registry = BootImage::Registry(image.to_string());
        match self.runtime.boot(launch.vm, launch.spec, &registry).await {
            Ok(()) => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM booted from the registry image",
                    log_fields! {
                        "vm" => launch.vm,
                        "image" => image,
                        "cpus" => launch.spec.cpus,
                        "memory_gib" => launch.spec.memory_gib,
                        "block_local_network" => launch.spec.network.block_local_network,
                    },
                );
                Ok(())
            }
            Err(BootError::Failed(cause, message)) => {
                Err(self.failed(sink, launch.vm, image, cause, message))
            }
            // EAC-FR-NCSA: in no registry and in no verified Docker backend.
            Err(BootError::ImageNotFound(message)) => Err(self.failed(
                sink,
                launch.vm,
                image,
                SandboxFailureCause::ImageUnavailable,
                message,
            )),
        }
    }

    /// EAC-FR-FUNB: every failure is an `ERROR` naming the cause and the
    /// runtime's own message.
    fn failed<S>(
        &self,
        sink: &S,
        vm: &str,
        image: &str,
        cause: SandboxFailureCause,
        message: String,
    ) -> SandboxLaunchError
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        logging::log_error(
            sink,
            &BUFFER,
            &DOMAINS,
            "sandbox VM could not be established",
            log_fields! {
                "vm" => vm,
                "image" => image,
                "cause" => cause.as_str(),
                "message" => message.as_str(),
            },
        );
        SandboxLaunchError::Sandbox(SandboxFailure { cause, message })
    }

    /// EAC-FR-ABHG: stop the VM of one scope, waiting for a launch that runs in
    /// it to end first.
    pub async fn stop(&self, vm: &str) {
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            slots.get(vm).cloned()
        };
        let Some(slot) = slot else {
            return;
        };
        let mut booted = slot.lock().await;
        if booted.take().is_some() {
            self.runtime.stop(vm).await;
        }
    }

    /// EAC-FR-ABHG: stop every VM this pool booted. A launch that runs is not
    /// waited for: the caller stops the runs before it stops the VMs, and a
    /// shutdown must not wait on a turn.
    pub async fn stop_all(&self) {
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let slots: Vec<(String, Slot)> = {
            let slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            slots.iter().map(|(vm, slot)| (vm.clone(), slot.clone())).collect()
        };
        for (vm, slot) in slots {
            let booted = match slot.try_lock() {
                Ok(mut booted) => booted.take().is_some(),
                // A launch holds the slot; its VM is running.
                Err(_) => true,
            };
            if booted {
                self.runtime.stop(&vm).await;
            }
        }
    }

    /// The VMs this pool holds as booted, for a test to read.
    #[cfg(test)]
    pub fn booted(&self, vm: &str) -> Option<SandboxSpec> {
        let slot = self.slot(vm);
        let booted = slot.try_lock().ok()?.clone();
        booted.map(|booted| booted.spec)
    }
}

/// Whether a launch can run in the VM its scope has booted.
enum Reuse {
    Yes,
    /// The scope holds no VM.
    Empty,
    /// The scope holds a VM that this launch cannot use, for the reason named.
    No(&'static str),
}

/// EAC-FR-UEFI / EAC-FR-HXDN: a launch reuses the VM only where it booted in
/// this epoch with an equal spec, every read/write source is still the same
/// directory, and the turn is not a review kind. An identity that cannot be
/// read is never the same.
fn reuse(
    booted: Option<&Booted>,
    launch: &SandboxLaunch<'_>,
    identities: &[Option<(u64, u64)>],
    epoch: u64,
) -> Reuse {
    let Some(booted) = booted else {
        return Reuse::Empty;
    };
    if booted.epoch != epoch || &booted.spec != launch.spec {
        return Reuse::No("changed_spec");
    }
    if matches!(launch.turn_kind, TurnKind::Review | TurnKind::MergeReview) {
        return Reuse::No("fresh_checkout");
    }
    let same = booted.identities.len() == identities.len()
        && booted.identities.iter().zip(identities).all(|(a, b)| a.is_some() && a == b);
    if !same {
        return Reuse::No("changed_directory");
    }
    Reuse::Yes
}

/// EAC-FR-HXDN: the identity of each read/write mount source, in the order of
/// the spec's mounts.
fn source_identities(spec: &SandboxSpec) -> Vec<Option<(u64, u64)>> {
    spec.mounts
        .iter()
        .filter(|mount| !mount.read_only)
        .map(|mount| crate::fs::probe_directory_identity(Path::new(&mount.source)))
        .collect()
}

/// EAC-FR-MIQG: ends when `cancel` is cancelled, and never before.
async fn until_cancelled(cancel: &CancellationToken) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(CANCEL_POLL).await;
    }
}

/// EAC-FR-MIQG: the outcome of a launch cancelled before its process started.
/// It carries no output, so its `response` is null (EAC-FR-25).
fn cancelled_outcome() -> RuntimeOutcome {
    RuntimeOutcome {
        end: RunEnd::Cancelled,
        exit_code: None,
        stdout: Default::default(),
        stderr: Default::default(),
    }
}

/// EAC-FR-DWBS: the VM name of one scope of one project. A name identifies one
/// queue of one project, and holds nothing that names a path.
pub fn vm_name(project_key: &str, scope: &SandboxScope) -> String {
    let digest = crate::fs::sha256_bytes(format!("{project_key}\0{}", scope.queue_key).as_bytes());
    format!("synthesis-{}", &digest[..20])
}

// ---------------------------------------------------------------------------
// The production pool (EAC-FR-HMBJ, EAC-FR-ABHG)
// ---------------------------------------------------------------------------

/// The one pool a shipped build launches through, created on first use.
pub fn production_pool() -> &'static SandboxPool {
    static POOL: OnceLock<SandboxPool> = OnceLock::new();
    POOL.get_or_init(|| SandboxPool::new(production_runtime()))
}

/// Linux and macOS: the embedded microsandbox runtime.
#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
fn production_runtime() -> Arc<dyn SandboxRuntime> {
    Arc::new(micro::MicrosandboxRuntime::default())
}

/// Every other build, and every test build: no VM can be booted, so every
/// sandbox launch reports `unsupported_host` (EAC-FR-HMBJ). A test binds its own
/// double through [`super::AgentCliExecutor::with_sandbox_pool`].
#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
fn production_runtime() -> Arc<dyn SandboxRuntime> {
    Arc::new(NoSandbox)
}

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
struct NoSandbox;

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
impl SandboxRuntime for NoSandbox {
    fn boot<'a>(
        &'a self,
        _vm: &'a str,
        _spec: &'a SandboxSpec,
        _image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            Err(BootError::Failed(
                SandboxFailureCause::UnsupportedHost,
                "this build holds no microsandbox runtime".to_string(),
            ))
        })
    }

    fn load_export<'a>(
        &'a self,
        _archive: &'a Path,
        _reference: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Err("this build holds no microsandbox runtime".to_string()) })
    }

    fn run<'a>(
        &'a self,
        _vm: &'a str,
        _process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }

    fn refresh<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Err("this build holds no microsandbox runtime".to_string()) })
    }

    fn stop<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {})
    }
}

/// EAC-FR-ABHG: stop the VM of one stream of one project. Called when the
/// stream is deleted.
pub async fn stop_scope<S>(sink: &S, project_key: &str, queue_key: &str)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    let vm = vm_name(project_key, &SandboxScope { queue_key: queue_key.to_string() });
    production_pool().stop(&vm).await;
    logging::log_info(
        sink,
        &BUFFER,
        &DOMAINS,
        "sandbox VM of a stream stopped",
        log_fields! {
            "vm" => vm.as_str()
        },
    );
}

/// EAC-FR-ABHG: stop every VM. Called when the project closes and when the
/// application exits.
pub async fn stop_all<S>(sink: &S)
where
    S: LogSink + Clone + Send + Sync + 'static,
{
    production_pool().stop_all().await;
    logging::log_info(sink, &BUFFER, &DOMAINS, "every sandbox VM stopped", log_fields! {});
}

/// EAC-FR-ABHG: how long an exit waits for the VMs to stop. A VM that does not
/// stop in that time still ends with the application process.
const EXIT_STOP_BOUND: Duration = Duration::from_secs(10);

/// EAC-FR-ABHG: stop the VM of a deleted stream, without holding the command
/// that deleted it.
pub fn stop_scope_in_background<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: String,
    queue_key: String,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        stop_scope(&app, &project_key, &queue_key).await;
    });
}

/// EAC-FR-ABHG: stop every VM when the project closes, without holding the
/// close.
pub fn stop_all_in_background<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        stop_all(&app).await;
    });
}

/// EAC-FR-ABHG: stop every VM before the application exits, bounded so a VM
/// that does not answer cannot hold the exit. Runs on a thread of its own, so
/// it never blocks inside an asynchronous runtime.
pub fn stop_all_before_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    let stopped = std::thread::spawn(move || {
        tauri::async_runtime::block_on(async move {
            let _ = tokio::time::timeout(EXIT_STOP_BOUND, stop_all(&app)).await;
        });
    })
    .join();
    if stopped.is_err() {
        // The thread panicked. The VMs still end with the process.
        eprintln!("synthesis: the sandbox VMs could not be stopped before exit");
    }
}

#[cfg(test)]
mod tests;
