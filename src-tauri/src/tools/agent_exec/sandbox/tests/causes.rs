//! The causes a VM that cannot be established names (EAC-FR-NCSA), and the
//! second try through a new catalog connection (EAC-FR-SQWO).

use super::*;

/// A launch of `vm` whose image the Docker backend holds as `sha256:abc123`.
fn exported_launch(
    runtime: &Arc<RecordingSandbox>,
    vm: &str,
) -> (Result<RuntimeOutcome, SandboxLaunchError>, String) {
    let fx = fixture();
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("local/agent:dev", "/w/one");
    let exporter: Arc<dyn ImageExporter> = StubExporter::new("sha256:abc123");
    let result =
        block_on(pool.launch(&Silent, launch(&fx, vm, &spec, &s, Some(exporter)), process(&[])));
    (result, images::export_reference("local/agent:dev", "sha256:abc123"))
}

fn cause_of(result: Result<RuntimeOutcome, SandboxLaunchError>) -> SandboxFailure {
    match result {
        Err(SandboxLaunchError::Sandbox(failure)) => failure,
        other => panic!("expected a sandbox failure, found {other:?}"),
    }
}

fn catalog(message: &str) -> BootError {
    BootError::Catalog(message.to_string())
}

/// The records the log buffer holds for `vm`. Every test logs under a VM name
/// of its own, so no other test's record is read.
fn records_of(vm: &str) -> Vec<crate::logging::LogRecord> {
    let filter: crate::logging::LogFilter =
        serde_json::from_value(serde_json::json!({ "query": vm })).unwrap();
    crate::logging::BUFFER.query(&filter, None, 5000).expect("a page").records
}

fn record<'r>(
    records: &'r [crate::logging::LogRecord],
    message: &str,
) -> &'r crate::logging::LogRecord {
    records
        .iter()
        .find(|r| r.message == message)
        .unwrap_or_else(|| panic!("no record {message:?} in {records:?}"))
}

fn run(vm: &str) -> Call {
    Call::Run(vm.into(), Vec::new())
}

fn reopens(runtime: &RecordingSandbox) -> usize {
    runtime.calls().iter().filter(|c| matches!(c, Call::Reopen)).count()
}

fn boots(runtime: &RecordingSandbox) -> usize {
    runtime.calls().iter().filter(|c| matches!(c, Call::Boot(..))).count()
}

/// A registry launch of `vm`: no Docker backend holds the image.
fn registry_launch(
    runtime: &Arc<RecordingSandbox>,
    vm: &str,
) -> Result<RuntimeOutcome, SandboxLaunchError> {
    let fx = fixture();
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("ghcr.io/agent:1", "/w/one");
    block_on(pool.launch(&Silent, launch(&fx, vm, &spec, &s, None), process(&[])))
}

// EAC-FR-NCSA: no verified Docker backend, an image Docker does not hold, and a
// failed export are each their own cause; nothing falls back.
#[test]
fn export_failures_are_typed() {
    let fx = fixture();
    let s = settings();
    let spec = spec("local/agent:dev", "/w/one");
    let args: Vec<String> = Vec::new();
    let not_found = || vec![Err(BootError::ImageNotFound("not found".into()))];

    let pool = SandboxPool::new(RecordingSandbox::with_boots(not_found()));
    let error = block_on(pool.launch(&Silent, launch(&fx, "vm", &spec, &s, None), process(&args)))
        .unwrap_err();
    assert!(matches!(
        error,
        SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::ImageUnavailable,
            ..
        })
    ));

    let missing: Arc<dyn ImageExporter> = Arc::new(StubExporter {
        id: Err("no such image".into()),
        exports: StdMutex::new(0),
        exported: StdMutex::new(Vec::new()),
        fail_export: false,
    });
    let pool = SandboxPool::new(RecordingSandbox::with_boots(not_found()));
    let error =
        block_on(pool.launch(&Silent, launch(&fx, "vm", &spec, &s, Some(missing)), process(&args)))
            .unwrap_err();
    assert!(matches!(
        error,
        SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::ImageUnavailable,
            ..
        })
    ));

    let failing: Arc<dyn ImageExporter> = Arc::new(StubExporter {
        id: Ok("sha256:bb".into()),
        exports: StdMutex::new(0),
        exported: StdMutex::new(Vec::new()),
        fail_export: true,
    });
    let after_export = RecordingSandbox::with_boots(not_found());
    let pool = SandboxPool::new(after_export.clone());
    let error =
        block_on(pool.launch(&Silent, launch(&fx, "vm", &spec, &s, Some(failing)), process(&args)))
            .unwrap_err();
    assert!(matches!(
        error,
        SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::ImageExportFailed,
            ..
        })
    ));
    // A failed export falls back to nothing: no boot of any kind follows it.
    assert!(after_export.calls().iter().all(|call| !matches!(call, Call::Boot(..))));
}

// EAC-FR-NCSA: a load that fails names `image_load_failed` with the runtime's
// message, and no boot follows it.
#[test]
fn a_failed_load_is_a_load_failure() {
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.loads.lock().unwrap() =
        vec![Err(BootError::Failed(SandboxFailureCause::ImageLoadFailed, "bad layer".into()))];
    let (result, loaded) = exported_launch(&runtime, "vm-load");
    let failure = cause_of(result);
    assert_eq!(failure.cause, SandboxFailureCause::ImageLoadFailed);
    assert_eq!(failure.message, "bad layer");
    assert_eq!(runtime.calls(), vec![Call::Load(loaded)], "no reopen and no boot");
}

// EAC-FR-NCSA: a load that reports a missing image is a load failure, and a
// runtime that is not installed keeps its own cause.
#[test]
fn other_load_answers_keep_their_meaning() {
    let missing = RecordingSandbox::with_boots(Vec::new());
    *missing.loads.lock().unwrap() = vec![Err(BootError::ImageNotFound("gone".into()))];
    let failure = cause_of(exported_launch(&missing, "vm-load-missing").0);
    assert_eq!(failure.cause, SandboxFailureCause::ImageLoadFailed);
    assert_eq!(failure.message, "gone");

    let uninstalled = RecordingSandbox::with_boots(Vec::new());
    *uninstalled.loads.lock().unwrap() =
        vec![Err(BootError::Failed(SandboxFailureCause::RuntimeUnavailable, "no msb".into()))];
    let failure = cause_of(exported_launch(&uninstalled, "vm-load-runtime").0);
    assert_eq!(failure.cause, SandboxFailureCause::RuntimeUnavailable);
    assert_eq!(reopens(&uninstalled), 0);
}

// EAC-FR-NCSA: a loaded export that the runtime does not find at the boot is a
// load failure, never an export failure, and is not tried again.
#[test]
fn a_loaded_export_the_boot_does_not_find_is_a_load_failure() {
    let runtime =
        RecordingSandbox::with_boots(vec![Err(BootError::ImageNotFound("no such image".into()))]);
    let (result, loaded) = exported_launch(&runtime, "vm-missing");
    let failure = cause_of(result);
    assert_eq!(failure.cause, SandboxFailureCause::ImageLoadFailed);
    assert_eq!(failure.message, "no such image");
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Load(loaded.clone()),
            Call::Boot("vm-missing".into(), BootImage::Loaded(loaded))
        ]
    );
}

// EAC-FR-SQWO, EAC-FR-FUNB: a load that fails on the catalog opens a new
// catalog connection and loads once more, and the boot and the turn follow.
// The failure and the new connection are recorded with the VM, the image, and
// the image ID.
#[test]
fn a_catalog_failure_of_the_load_is_tried_again_through_a_new_connection() {
    use crate::logging::{Domain, LogLevel};
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.loads.lock().unwrap() = vec![Err(catalog("database error: not a database"))];
    let vm = "vm-sqwo-load-7f3c";
    let (result, loaded) = exported_launch(&runtime, vm);
    result.expect("the second load and the boot succeed");
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Load(loaded.clone()),
            Call::Reopen,
            Call::Load(loaded.clone()),
            Call::Boot(vm.into(), BootImage::Loaded(loaded)),
            run(vm),
        ]
    );
    let records = records_of(vm);
    let failed =
        record(&records, "sandbox runtime could not read its catalog, opening a new connection");
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(failed.fields["message"], "database error: not a database");
    let opened = record(&records, "sandbox runtime catalog opened again");
    assert_eq!(opened.level, LogLevel::Info);
    for r in [failed, opened] {
        assert_eq!(r.domains, vec![Domain::Ai, Domain::Backend]);
        assert_eq!(r.fields["vm"], vm);
        assert_eq!(r.fields["image"], "local/agent:dev");
        assert_eq!(r.fields["image_id"], "sha256:abc123");
    }
}

// EAC-FR-SQWO: a boot from the loaded export that fails on the catalog is
// tried once more after a new connection, and so is a registry boot.
#[test]
fn a_catalog_failure_of_a_boot_is_tried_again_through_a_new_connection() {
    let runtime = RecordingSandbox::with_boots(vec![Err(catalog("database error"))]);
    let (result, loaded) = exported_launch(&runtime, "vm-boot");
    result.expect("the second boot succeeds");
    let from = BootImage::Loaded(loaded.clone());
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Load(loaded),
            Call::Boot("vm-boot".into(), from.clone()),
            Call::Reopen,
            Call::Boot("vm-boot".into(), from),
            run("vm-boot"),
        ]
    );

    let registry = RecordingSandbox::with_boots(vec![Err(catalog("database error"))]);
    registry_launch(&registry, "vm-reg").expect("the second registry boot succeeds");
    let image = BootImage::Registry("ghcr.io/agent:1".into());
    assert_eq!(
        registry.calls(),
        vec![
            Call::Boot("vm-reg".into(), image.clone()),
            Call::Reopen,
            Call::Boot("vm-reg".into(), image),
            run("vm-reg"),
        ]
    );
}

// EAC-FR-SQWO: the load and the boot are each a step of their own, so each
// gets its own new connection and second try.
#[test]
fn the_load_and_the_boot_each_get_their_own_second_try() {
    let runtime = RecordingSandbox::with_boots(vec![Err(catalog("boot"))]);
    *runtime.loads.lock().unwrap() = vec![Err(catalog("load"))];
    let (result, loaded) = exported_launch(&runtime, "vm-both");
    result.expect("both second tries succeed");
    let from = BootImage::Loaded(loaded.clone());
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Load(loaded.clone()),
            Call::Reopen,
            Call::Load(loaded),
            Call::Boot("vm-both".into(), from.clone()),
            Call::Reopen,
            Call::Boot("vm-both".into(), from),
            run("vm-both"),
        ]
    );
}

// EAC-FR-SQWO, EAC-FR-NCSA, EAC-FR-FUNB: a second catalog failure is
// `runtime_catalog_failed` with the second message, on the export path, the
// load, and the registry path. The step is tried twice in all, after one new
// connection, and the failure is an ERROR that names the cause and message.
#[test]
fn a_second_catalog_failure_is_a_catalog_failure() {
    use crate::logging::LogLevel;
    let vm = "vm-sqwo-twice-2b9e";
    let runtime = RecordingSandbox::with_boots(vec![
        Err(catalog("database error: first")),
        Err(catalog("database error: second")),
    ]);
    let failure = cause_of(exported_launch(&runtime, vm).0);
    assert_eq!(failure.cause, SandboxFailureCause::RuntimeCatalogFailed);
    assert_eq!(failure.message, "database error: second");
    assert_eq!((reopens(&runtime), boots(&runtime)), (1, 2));
    let records = records_of(vm);
    let error = record(&records, "sandbox VM could not be established");
    assert_eq!(error.level, LogLevel::Error);
    assert_eq!(error.fields["cause"], "runtime_catalog_failed");
    assert_eq!(error.fields["message"], "database error: second");

    let load = RecordingSandbox::with_boots(Vec::new());
    *load.loads.lock().unwrap() = vec![Err(catalog("first")), Err(catalog("second"))];
    let failure = cause_of(exported_launch(&load, "vm-load-twice").0);
    assert_eq!(failure.cause, SandboxFailureCause::RuntimeCatalogFailed);
    assert_eq!(failure.message, "second");
    assert_eq!((reopens(&load), boots(&load)), (1, 0), "no boot follows");

    let registry =
        RecordingSandbox::with_boots(vec![Err(catalog("first")), Err(catalog("second"))]);
    let failure = cause_of(registry_launch(&registry, "vm-reg-twice"));
    assert_eq!(failure.cause, SandboxFailureCause::RuntimeCatalogFailed);
    assert_eq!(failure.message, "second");
    assert_eq!((reopens(&registry), boots(&registry)), (1, 2));
}

// EAC-FR-SQWO, EAC-FR-NCSA: where the second try fails for another reason,
// that reason is the cause.
#[test]
fn a_second_try_that_fails_otherwise_keeps_its_own_cause() {
    let missing = RecordingSandbox::with_boots(vec![
        Err(catalog("c")),
        Err(BootError::ImageNotFound("x".into())),
    ]);
    assert_eq!(
        cause_of(exported_launch(&missing, "vm-then-missing").0).cause,
        SandboxFailureCause::ImageLoadFailed
    );

    let memory = RecordingSandbox::with_boots(vec![
        Err(catalog("c")),
        Err(BootError::Failed(SandboxFailureCause::InsufficientMemory, "oom".into())),
    ]);
    let failure = cause_of(exported_launch(&memory, "vm-then-memory").0);
    assert_eq!(failure.cause, SandboxFailureCause::InsufficientMemory);
    assert_eq!(failure.message, "oom");

    let registry = RecordingSandbox::with_boots(vec![
        Err(catalog("c")),
        Err(BootError::ImageNotFound("x".into())),
    ]);
    assert_eq!(
        cause_of(registry_launch(&registry, "vm-reg-then-missing")).cause,
        SandboxFailureCause::ImageUnavailable
    );
}

// EAC-FR-SQWO, EAC-FR-NCSA: a new connection that cannot be opened is
// `runtime_catalog_failed` with its own message, and the step is not tried
// again.
#[test]
fn a_connection_that_cannot_be_opened_is_a_catalog_failure() {
    let runtime = RecordingSandbox::with_boots(vec![Err(catalog("database error"))]);
    *runtime.reopens.lock().unwrap() = vec![Err("catalog locked".into())];
    let failure = cause_of(exported_launch(&runtime, "vm-reopen").0);
    assert_eq!(failure.cause, SandboxFailureCause::RuntimeCatalogFailed);
    assert_eq!(failure.message, "catalog locked");
    assert_eq!((reopens(&runtime), boots(&runtime)), (1, 1));
}

// EAC-FR-SQWO, EAC-FR-HMBJ: a launch after a catalog failure, as Continue makes
// it, remembers nothing of the failure: it opens a new connection again where
// it needs one, boots, and completes.
#[test]
fn a_launch_after_a_catalog_failure_tries_again() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(vec![
        Err(catalog("first")),
        Err(catalog("second")),
        Err(catalog("third")),
    ]);
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("ghcr.io/agent:1", "/w/one");
    let first =
        block_on(pool.launch(&Silent, launch(&fx, "vm-again", &spec, &s, None), process(&[])));
    assert_eq!(cause_of(first).cause, SandboxFailureCause::RuntimeCatalogFailed);
    assert!(pool.booted("vm-again").is_none(), "nothing is booted after the failure");

    block_on(pool.launch(&Silent, launch(&fx, "vm-again", &spec, &s, None), process(&[])))
        .expect("the next launch boots");
    assert_eq!((reopens(&runtime), boots(&runtime)), (2, 4));
    assert!(pool.booted("vm-again").is_some());
}

// EAC-FR-SQWO: a failure that is not a catalog failure opens no new connection.
#[test]
fn other_failures_open_no_new_connection() {
    let runtime = RecordingSandbox::with_boots(vec![Err(BootError::Failed(
        SandboxFailureCause::InsufficientMemory,
        "cannot allocate memory".into(),
    ))]);
    let failure = cause_of(exported_launch(&runtime, "vm-memory").0);
    assert_eq!(failure.cause, SandboxFailureCause::InsufficientMemory);
    assert_eq!(reopens(&runtime), 0);
}

/// Every cause. The match has no wildcard, so a new cause fails to compile
/// until it is listed here.
fn every_cause() -> Vec<SandboxFailureCause> {
    use SandboxFailureCause::*;
    let all = vec![
        UnsupportedHost,
        RuntimeUnavailable,
        ImageUnavailable,
        ImageExportFailed,
        ImageLoadFailed,
        RuntimeCatalogFailed,
        InsufficientMemory,
        BootFailed,
    ];
    for cause in &all {
        match cause {
            UnsupportedHost | RuntimeUnavailable | ImageUnavailable | ImageExportFailed
            | ImageLoadFailed | RuntimeCatalogFailed | InsufficientMemory | BootFailed => {}
        }
    }
    all
}

// EAC-FR-NCSA: every cause has its own name and sentence, and no sentence has
// a full stop, because the detail reads the cause up to one. The two load
// causes name a fault apart from the Docker export.
#[test]
fn each_cause_has_its_own_name_and_sentence() {
    use SandboxFailureCause::*;
    let all = every_cause();
    let names: std::collections::BTreeSet<_> = all.iter().map(|c| c.as_str()).collect();
    let sentences: std::collections::BTreeSet<_> = all.iter().map(|c| c.sentence()).collect();
    assert_eq!(names.len(), all.len());
    assert_eq!(sentences.len(), all.len());
    assert_eq!(ImageLoadFailed.as_str(), "image_load_failed");
    assert_eq!(RuntimeCatalogFailed.as_str(), "runtime_catalog_failed");
    for cause in &all {
        assert!(!cause.sentence().contains('.'), "{}", cause.sentence());
    }
    for cause in [ImageLoadFailed, RuntimeCatalogFailed] {
        assert!(!cause.sentence().contains("Docker"), "{}", cause.sentence());
    }
}

/// A guard that started and holds its lock on `catalog`.
fn started(pid: u32) -> GuardReport {
    GuardReport {
        ended: None,
        outcome: GuardOutcome::Started { pid, catalog: "/h/db/msb.db".into() },
    }
}

// EAC-FR-NDDZ, EAC-FR-FUNB: a boot opened a catalog connection, so the pool asks
// for a guard beside it, and records the guard's start and its lock with the VM
// and the image.
#[test]
fn a_boot_asks_for_a_catalog_guard_and_records_its_start_and_lock() {
    use crate::logging::{Domain, LogLevel};
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.guards.lock().unwrap() = vec![started(4242)];
    let vm = "vm-nddz-boot-5b1e";
    registry_launch(&runtime, vm).expect("the launch succeeds");
    assert_eq!(*runtime.guard_requests.lock().unwrap(), vec![false], "one request, no renewal");
    let records = records_of(vm);
    for message in ["sandbox catalog guard started", "sandbox catalog guard holds its lock"] {
        let r = record(&records, message);
        assert_eq!(r.level, LogLevel::Info);
        assert_eq!(r.domains, vec![Domain::Ai, Domain::Backend]);
        assert_eq!(r.fields["vm"], vm);
        assert_eq!(r.fields["image"], "ghcr.io/agent:1");
        assert_eq!(r.fields["pid"], 4242);
        assert_eq!(r.fields["catalog"], "/h/db/msb.db");
    }
}

// EAC-FR-NDDZ, EAC-FR-SQWO, EAC-FR-FUNB: a new catalog connection ends the
// earlier guard and starts a new one, and the end is recorded. The step tried
// again goes through that new guard, so it asks for no other.
#[test]
fn a_new_catalog_connection_renews_the_guard() {
    let runtime = RecordingSandbox::with_boots(vec![Err(catalog("database error"))]);
    let renewed = GuardReport {
        ended: Some(GuardEnd { pid: 7, cause: "replaced", exit_code: None }),
        ..started(8)
    };
    *runtime.guards.lock().unwrap() = vec![renewed];
    let vm = "vm-nddz-renew-91c2";
    registry_launch(&runtime, vm).expect("the second boot succeeds");
    assert_eq!(*runtime.guard_requests.lock().unwrap(), vec![true]);
    let records = records_of(vm);
    let ends: Vec<_> =
        records.iter().filter(|r| r.message == "sandbox catalog guard ended").collect();
    assert_eq!(ends.len(), 1, "one guard ended: {ends:?}");
    let ended = ends[0];
    assert_eq!(ended.fields["pid"], 7);
    assert_eq!(ended.fields["cause"], "replaced");
    assert_eq!(ended.fields["exit_code"], serde_json::Value::Null);
    let starts =
        records.iter().filter(|r| r.message == "sandbox catalog guard started").collect::<Vec<_>>();
    assert_eq!(starts.len(), 1, "one guard started: {starts:?}");
    assert_eq!(starts[0].fields["pid"], 8);
}

// EAC-FR-PIHS: a guard that cannot start is a warning that names the stage,
// and the launch goes on.
#[test]
fn a_guard_that_cannot_start_is_a_warning_and_the_launch_goes_on() {
    use crate::logging::LogLevel;
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.guards.lock().unwrap() = vec![GuardReport {
        ended: None,
        outcome: GuardOutcome::Failed {
            stage: "lock_wait",
            message: "the guard did not take its lock in time".into(),
        },
    }];
    let vm = "vm-pihs-warn-3d70";
    registry_launch(&runtime, vm).expect("the launch goes on");
    assert_eq!(runtime.calls().last(), Some(&run(vm)), "the turn ran");
    let records = records_of(vm);
    let warned = record(&records, "sandbox catalog guard could not start");
    assert_eq!(warned.level, LogLevel::Warn);
    assert_eq!(warned.domains, vec![crate::logging::Domain::Ai, crate::logging::Domain::Backend]);
    assert_eq!(warned.fields["vm"], vm);
    assert_eq!(warned.fields["stage"], "lock_wait");
    assert_eq!(warned.fields["message"], "the guard did not take its lock in time");
    assert!(!records.iter().any(|r| r.message == "sandbox catalog guard started"));
}

// EAC-FR-NDDZ: a launch whose catalog steps all failed has no connection to
// guard, and a host that needs no guard records nothing.
#[test]
fn a_failed_boot_asks_for_no_guard_and_a_host_without_one_records_nothing() {
    let failed = RecordingSandbox::with_boots(vec![Err(BootError::Failed(
        SandboxFailureCause::BootFailed,
        "no".into(),
    ))]);
    registry_launch(&failed, "vm-nddz-failed-0a4f").expect_err("the boot fails");
    assert!(failed.guard_requests.lock().unwrap().is_empty());

    let plain = RecordingSandbox::with_boots(Vec::new());
    let vm = "vm-nddz-plain-6e2b";
    registry_launch(&plain, vm).expect("the launch succeeds");
    assert_eq!(*plain.guard_requests.lock().unwrap(), vec![false]);
    assert!(!records_of(vm).iter().any(|r| r.message.starts_with("sandbox catalog guard")));
}

// EAC-FR-NDDZ, EAC-FR-FUNB: each catalog step that succeeds asks for the guard.
// On the export path the image ID is known, so the guard records carry it, and
// a guard that ended on its own is recorded with its exit code.
#[test]
fn guard_records_of_an_exported_boot_carry_the_image_id() {
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.guards.lock().unwrap() = vec![
        GuardReport {
            ended: Some(GuardEnd { pid: 3, cause: "exited", exit_code: Some(0) }),
            ..started(5)
        },
        GuardReport { ended: None, outcome: GuardOutcome::Running { pid: 5 } },
    ];
    let vm = "vm-nddz-export-c81d";
    let (result, _) = exported_launch(&runtime, vm);
    result.expect("the load and the boot succeed");
    assert_eq!(*runtime.guard_requests.lock().unwrap(), vec![false, false], "the load, the boot");
    let records = records_of(vm);
    let ended = record(&records, "sandbox catalog guard ended");
    assert_eq!(ended.fields["cause"], "exited");
    assert_eq!(ended.fields["exit_code"], 0);
    for message in [
        "sandbox catalog guard ended",
        "sandbox catalog guard started",
        "sandbox catalog guard holds its lock",
    ] {
        assert_eq!(record(&records, message).fields["image_id"], "sha256:abc123", "{message}");
    }
}
