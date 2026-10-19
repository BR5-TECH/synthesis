//! The tests of the sandbox pool and its export fallback.
//!
//! Specification: `specifications/tools/EAC-execute-agent-cli.md`. Every test
//! binds the seam to a recording double, so none needs virtualisation, an
//! image, or a network (EAC-FR-VPNW).

use std::sync::Mutex as StdMutex;

use super::*;
use crate::isolation::UnsupportedReason;
use crate::logging::BufferState;

#[derive(Clone, Default)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

/// What the double was asked, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Boot(String, BootImage),
    Load(String),
    Reopen,
    Run(String, Vec<String>),
    Refresh(String),
    Stop(String),
}

/// A sandbox runtime that records every call and answers what it is told.
#[derive(Default)]
struct RecordingSandbox {
    calls: StdMutex<Vec<Call>>,
    /// Answers for successive boots; `Ok` once the list is empty.
    boots: StdMutex<Vec<Result<(), BootError>>>,
    /// Answers for successive loads; `Ok` once the list is empty.
    loads: StdMutex<Vec<Result<(), BootError>>>,
    /// Answers for successive catalog reopens; `Ok` once the list is empty.
    reopens: StdMutex<Vec<Result<(), String>>>,
    /// Answers for successive runs; `Ok` once the list is empty.
    runs: StdMutex<Vec<RuntimeError>>,
    /// Whether a boot never ends, as a boot that pulls or exports for a long
    /// time does not.
    hangs: bool,
    /// Notified when a boot starts.
    boot_started: tokio::sync::Notify,
    /// The answer of every cache drop; `None` succeeds (EAC-FR-JRVY).
    refresh_error: StdMutex<Option<String>>,
    /// Answers for successive guard requests; `NotNeeded` once the list is
    /// empty (EAC-FR-NDDZ).
    guards: StdMutex<Vec<GuardReport>>,
    /// The `renew` of every guard request, kept apart from `calls`.
    guard_requests: StdMutex<Vec<bool>>,
}

impl RecordingSandbox {
    fn with_boots(boots: Vec<Result<(), BootError>>) -> Arc<RecordingSandbox> {
        Arc::new(RecordingSandbox { boots: StdMutex::new(boots), ..Default::default() })
    }

    fn hanging() -> Arc<RecordingSandbox> {
        Arc::new(RecordingSandbox { hangs: true, ..Default::default() })
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

impl SandboxRuntime for RecordingSandbox {
    fn boot<'a>(
        &'a self,
        vm: &'a str,
        _spec: &'a SandboxSpec,
        image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(Call::Boot(vm.to_string(), image.clone()));
            self.boot_started.notify_one();
            if self.hangs {
                std::future::pending::<()>().await;
            }
            let mut boots = self.boots.lock().unwrap();
            if boots.is_empty() {
                Ok(())
            } else {
                boots.remove(0)
            }
        })
    }

    fn load_export<'a>(
        &'a self,
        _archive: &'a Path,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(Call::Load(reference.to_string()));
            let mut loads = self.loads.lock().unwrap();
            if loads.is_empty() {
                Ok(())
            } else {
                loads.remove(0)
            }
        })
    }

    fn reopen_catalog(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(Call::Reopen);
            let mut reopens = self.reopens.lock().unwrap();
            if reopens.is_empty() {
                Ok(())
            } else {
                reopens.remove(0)
            }
        })
    }

    fn guard_catalog(&self, renew: bool) -> BoxFuture<'_, GuardReport> {
        Box::pin(async move {
            self.guard_requests.lock().unwrap().push(renew);
            let mut guards = self.guards.lock().unwrap();
            if guards.is_empty() {
                GuardReport::default()
            } else {
                guards.remove(0)
            }
        })
    }

    fn run<'a>(
        &'a self,
        vm: &'a str,
        process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Run(vm.to_string(), process.vendor_args.to_vec()));
            let mut runs = self.runs.lock().unwrap();
            if !runs.is_empty() {
                return Err(runs.remove(0));
            }
            Ok(RuntimeOutcome {
                end: super::super::runtime::RunEnd::Exited,
                exit_code: Some(0),
                stdout: Default::default(),
                stderr: Default::default(),
            })
        })
    }

    fn refresh<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(Call::Refresh(vm.to_string()));
            match self.refresh_error.lock().unwrap().clone() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
    }

    fn stop<'a>(&'a self, vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(Call::Stop(vm.to_string()));
        })
    }
}

/// A Docker backend that answers one image ID and writes a stand-in archive.
struct StubExporter {
    id: Result<String, String>,
    exports: StdMutex<u32>,
    /// What every export was asked to save (EAC-FR-ASNS).
    exported: StdMutex<Vec<String>>,
    fail_export: bool,
}

impl StubExporter {
    fn new(id: &str) -> Arc<StubExporter> {
        Arc::new(StubExporter {
            id: Ok(id.to_string()),
            exports: StdMutex::new(0),
            exported: StdMutex::new(Vec::new()),
            fail_export: false,
        })
    }
}

impl ImageExporter for StubExporter {
    fn image_id<'a>(&'a self, _reference: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move { self.id.clone() })
    }

    fn export<'a>(
        &'a self,
        reference: &'a str,
        destination: &'a Path,
        fs: &'a FsAccess,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            *self.exports.lock().unwrap() += 1;
            self.exported.lock().unwrap().push(reference.to_string());
            if self.fail_export {
                return Err("docker save failed".to_string());
            }
            fs.write_bytes_atomic(destination, b"archive").map_err(|e| e.to_string())
        })
    }
}

fn settings() -> SandboxSettings {
    SandboxSettings {
        cpus: 2,
        memory_gib: 4,
        disk_gib: 16,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: None,
    }
}

/// The host user every spec of this file names (EAC-FR-KTWO).
const OWNER: &str = "1000:1000";

/// A real directory for a mount source named `name`. A reuse compares the
/// identity of each read/write source (EAC-FR-HXDN), and a path that does not
/// exist has none, so a source here must exist.
fn real_dir(name: &str) -> String {
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let root = ROOT.get_or_init(|| tempfile::tempdir().unwrap());
    let path = root.path().canonicalize().unwrap().join(name.trim_start_matches('/'));
    std::fs::create_dir_all(&path).unwrap();
    path.to_string_lossy().into_owned()
}

fn spec(image: &str, workspace: &str) -> SandboxSpec {
    let source = real_dir(workspace);
    SandboxSpec::new(
        image.to_string(),
        OWNER.to_string(),
        &settings(),
        vec![SandboxMount { source: source.clone(), target: source, read_only: false }],
    )
}

/// A process with no arguments that `cancel` cancels.
fn cancellable(cancel: &CancellationToken) -> SandboxProcess<'static> {
    SandboxProcess { cancel: cancel.clone(), ..process(&[]) }
}

/// The bound on a test that must end because of a cancellation. It is far
/// above any time the pool needs, so it fails only a launch that never ends.
const NEVER: Duration = Duration::from_secs(60);

fn is_cancelled(result: &Result<RuntimeOutcome, SandboxLaunchError>) -> bool {
    matches!(result, Ok(outcome) if outcome.end == RunEnd::Cancelled && outcome.exit_code.is_none())
}

fn process(args: &[String]) -> SandboxProcess<'_> {
    SandboxProcess {
        vendor_args: args,
        workdir: "/workspace",
        user: "1000:1000".to_string(),
        env: Vec::new(),
        stdin: b"{}",
        timeout: Duration::from_secs(60),
        cancel: CancellationToken::new(),
        stdout_limit: 1024,
        stderr_limit: 1024,
        observer: None,
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    fs: FsAccess,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let fs = FsAccess::builder().allow_root(&root).build().unwrap();
    Fixture { _dir: dir, root, fs }
}

fn launch<'a>(
    fx: &'a Fixture,
    vm: &'a str,
    spec: &'a SandboxSpec,
    settings: &'a SandboxSettings,
    exporter: Option<Arc<dyn ImageExporter>>,
) -> SandboxLaunch<'a> {
    SandboxLaunch {
        vm,
        spec,
        turn_kind: TurnKind::Work,
        settings,
        exporter,
        fs: &fx.fs,
        export_root: fx.root.join(EXPORT_CACHE_DIRNAME),
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

// EAC-FR-DWBS, EAC-FR-UEFI, EAC-FR-ZIYG, EAC-FR-JRVY: the first launch of a
// scope boots its VM, and a second launch with an equal spec drops the guest's
// caches and reuses the VM.
#[test]
fn an_equal_launch_reuses_the_running_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args = vec!["-p".to_string()];
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Boot("vm-a".into(), BootImage::Registry("agent:1".into())),
            Call::Run("vm-a".into(), args.clone()),
            Call::Refresh("vm-a".into()),
            Call::Run("vm-a".into(), args),
        ]
    );
    assert_eq!(pool.booted("vm-a"), Some(spec));
}

// EAC-FR-UEFI: a launch whose mounts, CPU, RAM, network, or image differ stops
// the VM and boots a new one; a different scope gets its own VM.
#[test]
fn a_changed_launch_reboots_and_scopes_never_share() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let first = spec("agent:1", "/w/one");
    let review = spec("agent:1", "/w/review");
    let mut more_ram = first.clone();
    more_ram.memory_gib = 8;
    let mut open = first.clone();
    open.network.block_local_network = false;
    let args: Vec<String> = Vec::new();
    block_on(async {
        for next in [&first, &review, &more_ram, &open] {
            pool.launch(&Silent, launch(&fx, "vm-a", next, &s, None), process(&args))
                .await
                .unwrap();
        }
        pool.launch(&Silent, launch(&fx, "vm-b", &first, &s, None), process(&args)).await.unwrap();
    });
    let calls = runtime.calls();
    let boots = calls.iter().filter(|c| matches!(c, Call::Boot(vm, _) if vm == "vm-a")).count();
    let stops = calls.iter().filter(|c| matches!(c, Call::Stop(vm) if vm == "vm-a")).count();
    assert_eq!(boots, 4);
    assert_eq!(stops, 3);
    assert!(calls.contains(&Call::Boot("vm-b".into(), BootImage::Registry("agent:1".into()))));
    assert!(!calls.contains(&Call::Stop("vm-b".into())));
}

// EAC-FR-QULL, EAC-FR-ASNS, EAC-FR-IGBY: an image the verified Docker backend
// holds boots from its export, exported by its image ID, and no registry is
// asked.
#[test]
fn a_local_image_boots_from_its_export() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("local/agent:dev", "/w/one");
    let exporter = StubExporter::new("sha256:abc123");
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(
            &Silent,
            launch(&fx, "vm-a", &spec, &s, Some(exporter.clone())),
            process(&args),
        )
        .await
        .unwrap();
    });
    let loaded = images::export_reference("local/agent:dev", "sha256:abc123");
    assert_ne!(loaded, "local/agent:dev");
    assert_eq!(
        runtime.calls()[..2],
        [Call::Load(loaded.clone()), Call::Boot("vm-a".into(), BootImage::Loaded(loaded))]
    );
    assert_eq!(*exporter.exported.lock().unwrap(), vec!["sha256:abc123".to_string()]);
    let file = fx
        .root
        .join(EXPORT_CACHE_DIRNAME)
        .join(images::export_file_name("local/agent:dev", "sha256:abc123"));
    assert!(file.is_file());
    assert_eq!(*exporter.exports.lock().unwrap(), 1);
}

// EAC-FR-IGBY: the export of an ID the cache holds is reused, and a new ID
// exports again and deletes the export of the old one.
#[test]
fn the_cache_reuses_an_id_and_deletes_stale_exports() {
    let fx = fixture();
    let root = fx.root.join(EXPORT_CACHE_DIRNAME);
    let cache = images::ExportCache::new(&fx.fs, root.clone());
    let old = StubExporter::new("sha256:0001");
    let new = StubExporter::new("sha256:0002");
    let other = StubExporter::new("sha256:0003");
    block_on(async {
        let (first, hit) = cache.ensure("agent:dev", "sha256:0001", old.as_ref()).await.unwrap();
        assert!(!hit);
        let (again, hit) = cache.ensure("agent:dev", "sha256:0001", old.as_ref()).await.unwrap();
        assert!(hit);
        assert_eq!(first, again);
        assert_eq!(*old.exports.lock().unwrap(), 1);
        cache.ensure("other:dev", "sha256:0003", other.as_ref()).await.unwrap();
        let (second, hit) = cache.ensure("agent:dev", "sha256:0002", new.as_ref()).await.unwrap();
        assert!(!hit);
        assert!(second.is_file());
        assert!(!first.is_file(), "the stale export of the same reference is deleted");
    });
    assert!(root.join(images::export_file_name("other:dev", "sha256:0003")).is_file());
    assert_eq!(cache.root(), root.as_path());
}

// EAC-FR-IGBY: a failed export leaves no file the cache would read as whole.
#[test]
fn a_failed_export_leaves_nothing_behind() {
    let fx = fixture();
    let root = fx.root.join(EXPORT_CACHE_DIRNAME);
    let cache = images::ExportCache::new(&fx.fs, root.clone());
    let exporter = StubExporter {
        id: Ok("sha256:aa".into()),
        exports: StdMutex::new(0),
        exported: StdMutex::new(Vec::new()),
        fail_export: true,
    };
    let error = block_on(cache.ensure("agent:dev", "sha256:aa", &exporter)).unwrap_err();
    assert_eq!(error, "docker save failed");
    assert!(fx.fs.list_dir(&root).unwrap().is_empty());
}

// EAC-FR-NCSA, GSS-FR-NCAQ: a boot failure keeps its cause, a machine that
// cannot run Microsandbox boots nothing, and a failed boot leaves no VM booted.
#[test]
fn boot_failures_keep_their_cause() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    let runtime = RecordingSandbox::with_boots(vec![Err(BootError::Failed(
        SandboxFailureCause::InsufficientMemory,
        "cannot allocate memory".into(),
    ))]);
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let error = block_on(pool.launch(&Silent, launch(&fx, "vm", &spec, &s, None), process(&args)))
        .unwrap_err();
    assert!(matches!(
        error,
        SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::InsufficientMemory,
            ..
        })
    ));
    assert_eq!(pool.booted("vm"), None);

    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let unsupported =
        SandboxSettings { unsupported_reason: Some(UnsupportedReason::KvmMissing), ..settings() };
    let error = block_on(pool.launch(
        &Silent,
        launch(&fx, "vm", &spec, &unsupported, None),
        process(&args),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::UnsupportedHost,
            ..
        })
    ));
    assert!(runtime.calls().is_empty());
}

// EAC-FR-ABHG: stopping a scope stops its VM once, and stopping all stops every
// booted VM.
#[test]
fn stopping_a_scope_and_all_scopes() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-b", &spec, &s, None), process(&args)).await.unwrap();
        pool.stop("vm-a").await;
        pool.stop("vm-a").await;
        pool.stop("vm-unknown").await;
        pool.stop_all().await;
    });
    let stops: Vec<Call> =
        runtime.calls().into_iter().filter(|c| matches!(c, Call::Stop(_))).collect();
    assert_eq!(stops, vec![Call::Stop("vm-a".into()), Call::Stop("vm-b".into())]);
    assert_eq!(pool.booted("vm-a"), None);
    assert_eq!(pool.booted("vm-b"), None);
}

// EAC-FR-DWBS: a VM name identifies one queue of one project and names no path.
#[test]
fn vm_names_identify_one_queue_of_one_project() {
    let stream = SandboxScope { queue_key: "stream-1".into() };
    let worktree = SandboxScope { queue_key: "worktree:/home/me/project".into() };
    let a = vm_name("project-a", &stream);
    assert_eq!(a, vm_name("project-a", &stream));
    assert_ne!(a, vm_name("project-b", &stream));
    assert_ne!(a, vm_name("project-a", &worktree));
    assert!(!vm_name("project-a", &worktree).contains("home"));
    assert!(a.starts_with("synthesis-"));
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
}

// EAC-FR-ASNS, EAC-FR-IGBY: names derive from the reference and the ID, and an
// ID never names a path.
#[test]
fn export_names_derive_from_the_reference_and_the_id() {
    assert_eq!(images::id_hex("sha256:ABCdef/../"), "abcdef");
    let name = images::export_file_name("ghcr.io/me/agent:1", "sha256:ff");
    assert!(name.ends_with("--ff.tar"));
    assert!(!name.contains('/'));
    assert_ne!(images::reference_slug("a/b"), images::reference_slug("a-b"));
    let reference = images::export_reference("ghcr.io/me/agent:1", "sha256:ff");
    assert!(reference.starts_with("localhost/synthesis-export/"));
    assert!(reference.ends_with(":ff"));
}

// EAC-FR-UEFI: the spec sorts its mounts, so an order of composition is not a
// difference.
#[test]
fn mount_order_is_not_a_difference() {
    let a = SandboxMount { source: "/a".into(), target: "/a".into(), read_only: false };
    let b = SandboxMount { source: "/b".into(), target: "/b".into(), read_only: true };
    let one = SandboxSpec::new("i".into(), OWNER.into(), &settings(), vec![a.clone(), b.clone()]);
    let two = SandboxSpec::new("i".into(), OWNER.into(), &settings(), vec![b, a]);
    assert_eq!(one, two);
}

// EAC-FR-ABHG, EAC-FR-UEFI: a stop of every VM while a launch holds a slot
// still stops that VM, and the next launch boots a new one rather than
// reusing the stopped one.
#[test]
fn a_vm_stopped_while_its_slot_is_held_is_never_reused() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        let slot = pool.slot("vm-a");
        let held = slot.lock().await;
        pool.stop_all().await;
        drop(held);
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    let calls = runtime.calls();
    assert!(calls.contains(&Call::Stop("vm-a".into())));
    let boots = calls.iter().filter(|c| matches!(c, Call::Boot(..))).count();
    assert_eq!(boots, 2, "{calls:?}");
}

// EAC-FR-UEFI: a VM that could not start the process is not trusted again,
// and the next launch boots a new one.
#[test]
fn a_vm_that_could_not_start_the_process_is_rebooted() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    runtime.runs.lock().unwrap().push(RuntimeError::LaunchFailed("agentd gone".into()));
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    block_on(async {
        let first =
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await;
        assert!(matches!(first, Err(SandboxLaunchError::Runtime(RuntimeError::LaunchFailed(_)))));
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    let calls = runtime.calls();
    let boots = calls.iter().filter(|c| matches!(c, Call::Boot(..))).count();
    assert_eq!(boots, 2, "{calls:?}");
    assert!(calls.contains(&Call::Stop("vm-a".into())));
}

// EAC-FR-MIQG: a run cancelled while its VM boots ends at once with a
// `cancelled` outcome, not with a sandbox failure. The VM the boot left is
// stopped and is not recorded as booted.
#[test]
fn a_cancelled_boot_ends_the_launch() {
    let fx = fixture();
    let runtime = RecordingSandbox::hanging();
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let cancel = CancellationToken::new();
    let result = block_on(async {
        let launching =
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), cancellable(&cancel));
        let canceller = async {
            runtime.boot_started.notified().await;
            cancel.cancel();
        };
        tokio::time::timeout(NEVER, async { tokio::join!(launching, canceller).0 }).await
    })
    .expect("a cancelled boot ends the launch");
    assert!(is_cancelled(&result), "{result:?}");
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Boot("vm-a".into(), BootImage::Registry("agent:1".into())),
            Call::Stop("vm-a".into()),
        ]
    );
    assert_eq!(pool.booted("vm-a"), None);
}

// EAC-FR-MIQG, EAC-FR-DWBS: a run cancelled while another launch of its scope
// holds the VM stops waiting at once and boots nothing.
#[test]
fn a_cancelled_wait_for_the_vm_ends_the_launch() {
    let fx = fixture();
    let runtime = RecordingSandbox::hanging();
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let holder = CancellationToken::new();
    let waiter = CancellationToken::new();
    let (held, waited) = block_on(async {
        let holding =
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), cancellable(&holder));
        let waiting = async {
            let result = pool
                .launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), cancellable(&waiter))
                .await;
            // The waiter ended while the holder still boots; now end that too.
            holder.cancel();
            result
        };
        let canceller = async {
            runtime.boot_started.notified().await;
            waiter.cancel();
        };
        tokio::time::timeout(NEVER, async {
            let (held, waited, ()) = tokio::join!(holding, waiting, canceller);
            (held, waited)
        })
        .await
    })
    .expect("a cancelled wait ends the launch");
    assert!(is_cancelled(&waited), "{waited:?}");
    assert!(is_cancelled(&held), "{held:?}");
    let boots = runtime.calls().iter().filter(|c| matches!(c, Call::Boot(..))).count();
    assert_eq!(boots, 1, "the cancelled waiter boots nothing");
}

// EAC-FR-MNEH: a boot that does not end within the bound is abandoned, its VM
// is stopped, and the launch fails with `boot_failed`.
#[test]
fn a_boot_past_its_bound_fails() {
    let fx = fixture();
    let runtime = RecordingSandbox::hanging();
    let pool = SandboxPool::new(runtime.clone()).with_boot_bound(Duration::from_millis(10));
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    let result = block_on(async {
        tokio::time::timeout(
            NEVER,
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)),
        )
        .await
    })
    .expect("the bound ends the boot");
    assert!(
        matches!(
            &result,
            Err(SandboxLaunchError::Sandbox(SandboxFailure {
                cause: SandboxFailureCause::BootFailed,
                ..
            }))
        ),
        "{result:?}"
    );
    assert!(runtime.calls().contains(&Call::Stop("vm-a".into())));
    assert_eq!(pool.booted("vm-a"), None);
}

/// A launch of `turn_kind`, otherwise as [`launch`].
fn launch_kind<'a>(
    fx: &'a Fixture,
    spec: &'a SandboxSpec,
    settings: &'a SandboxSettings,
    turn_kind: TurnKind,
) -> SandboxLaunch<'a> {
    SandboxLaunch { turn_kind, ..launch(fx, "vm-a", spec, settings, None) }
}

/// The boots and the stops of `vm-a`, in order.
fn lifecycle(runtime: &RecordingSandbox) -> Vec<&'static str> {
    runtime
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::Boot(vm, _) if vm == "vm-a" => Some("boot"),
            Call::Stop(vm) if vm == "vm-a" => Some("stop"),
            Call::Refresh(vm) if vm == "vm-a" => Some("refresh"),
            _ => None,
        })
        .collect()
}

// EAC-FR-HXDN: a review and a merge_review launch never reuse a VM, even with
// an equal spec, and the old VM is stopped before the new one boots. A
// merge_work launch reuses it.
#[test]
fn a_review_kind_launch_always_boots_a_new_vm() {
    let fx = fixture();
    let s = settings();
    let spec = spec("agent:1", "/w/kinds");
    let args: Vec<String> = Vec::new();
    for (kind, expected) in [
        (TurnKind::Review, vec!["boot", "stop", "boot"]),
        (TurnKind::MergeReview, vec!["boot", "stop", "boot"]),
        (TurnKind::MergeWork, vec!["boot", "refresh"]),
    ] {
        let runtime = RecordingSandbox::with_boots(Vec::new());
        let pool = SandboxPool::new(runtime.clone());
        block_on(async {
            for _ in 0..2 {
                pool.launch(&Silent, launch_kind(&fx, &spec, &s, kind), process(&args))
                    .await
                    .unwrap();
            }
        });
        assert_eq!(lifecycle(&runtime), expected, "{kind:?}");
    }
}

// EAC-FR-HXDN: a read/write source made again at the same path is another
// directory, so the next launch boots a new VM.
#[test]
fn a_directory_made_again_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/again");
    let source = PathBuf::from(&spec.mounts[0].source);
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        // Renamed aside rather than deleted, so the inode cannot be given back.
        std::fs::rename(&source, source.with_file_name("again-aside")).unwrap();
        std::fs::create_dir(&source).unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-HXDN: a read/write source that is not there has no identity, which is
// never the same, so the launch boots a new VM rather than reusing one.
#[test]
fn a_source_with_no_identity_is_never_reused() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let absent = real_dir("/w/absent-parent").to_string() + "/absent";
    let spec = SandboxSpec::new(
        "agent:1".into(),
        OWNER.into(),
        &s,
        vec![SandboxMount { source: absent.clone(), target: absent, read_only: false }],
    );
    let args: Vec<String> = Vec::new();
    block_on(async {
        for _ in 0..2 {
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args))
                .await
                .unwrap();
        }
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-UEFI, EAC-FR-KTWO: the owner is part of what a VM boots with, so a
// launch for another owner boots a new VM.
#[test]
fn another_owner_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let first = spec("agent:1", "/w/owner");
    let mut other = first.clone();
    other.owner = "1001:1001".into();
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &first, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &other, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
    assert_eq!(pool.booted("vm-a").map(|spec| spec.owner), Some("1001:1001".to_string()));
}

// EAC-FR-JRVY: a VM whose caches cannot be dropped is not reused: it is stopped
// and a new one boots before the launch.
#[test]
fn a_failed_cache_drop_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.refresh_error.lock().unwrap() = Some("no root".into());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/refresh");
    let args = vec!["-p".to_string()];
    block_on(async {
        for _ in 0..2 {
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args))
                .await
                .unwrap();
        }
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "refresh", "stop", "boot"]);
    assert_eq!(runtime.calls().last(), Some(&Call::Run("vm-a".into(), args)));
}

// EAC-FR-QULL: an image the verified Docker backend does not hold boots from
// the configured reference through a registry, and nothing is exported.
#[test]
fn an_image_docker_does_not_hold_boots_from_a_registry() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("registry.example/agent:1", "/w/registry");
    let missing = Arc::new(StubExporter {
        id: Err("no such image".into()),
        exports: StdMutex::new(0),
        exported: StdMutex::new(Vec::new()),
        fail_export: false,
    });
    let args: Vec<String> = Vec::new();
    block_on(async {
        let exporter: Arc<dyn ImageExporter> = missing.clone();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, Some(exporter)), process(&args))
            .await
            .unwrap();
    });
    assert_eq!(
        runtime.calls()[0],
        Call::Boot("vm-a".into(), BootImage::Registry("registry.example/agent:1".into()))
    );
    assert_eq!(*missing.exports.lock().unwrap(), 0);
}

// EAC-FR-PWLA: the image's `Env` comes first, a launch variable replaces an
// image variable of the same name, the last of two equal image names wins, a
// value keeps every `=` after the first, and an entry with no `=` is left out.
#[test]
fn the_process_environment_puts_the_launch_over_the_image() {
    let image = vec![
        "PATH=/usr/local/cargo/bin:/usr/bin".to_string(),
        "HOME=/home/agent".to_string(),
        "GIT_OPTIONAL_LOCKS=1".to_string(),
        "OPTS=a=b".to_string(),
        "EMPTY=".to_string(),
        "NOVALUE".to_string(),
        "HOME=/home/other".to_string(),
    ];
    let launch = vec![
        ("GIT_OPTIONAL_LOCKS".to_string(), SecretString::new("0".to_string())),
        ("TOKEN".to_string(), SecretString::new("secret".to_string())),
    ];
    let composed: Vec<(String, String)> = compose_env(&image, &launch)
        .into_iter()
        .map(|(name, value)| (name, value.expose().to_string()))
        .collect();
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    };
    assert_eq!(
        composed,
        pairs(&[
            ("PATH", "/usr/local/cargo/bin:/usr/bin"),
            ("OPTS", "a=b"),
            ("EMPTY", ""),
            ("HOME", "/home/other"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("TOKEN", "secret"),
        ])
    );
}

// EAC-FR-HXDN: only read/write sources count. A read-only source made again at
// the same path does not reboot the VM.
#[test]
fn a_read_only_source_made_again_does_not_reboot() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let workspace = real_dir("/w/ro-parent");
    let read_only = real_dir("/w/ro-source");
    let spec = SandboxSpec::new(
        "agent:1".into(),
        OWNER.into(),
        &s,
        vec![
            SandboxMount { source: workspace.clone(), target: workspace, read_only: false },
            SandboxMount { source: read_only.clone(), target: read_only.clone(), read_only: true },
        ],
    );
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        std::fs::rename(&read_only, format!("{read_only}-aside")).unwrap();
        std::fs::create_dir(&read_only).unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "refresh"]);
}

// EAC-FR-UEFI, EAC-FR-QDVN: the disk size is part of what a VM boots with, so
// a launch with another disk size boots a new VM.
#[test]
fn another_disk_size_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let first = spec("agent:1", "/w/disk");
    let mut larger = first.clone();
    larger.disk_gib = 128;
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &first, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &larger, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
    assert_eq!(pool.booted("vm-a").map(|spec| spec.disk_gib), Some(128));
}

#[cfg(target_os = "macos")]
mod catalog_lock;
mod causes;
mod idle;
