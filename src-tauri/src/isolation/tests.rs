//! The tests of the isolation settings.
//!
//! Specification: `specifications/core/GSS-global-settings-storage.md`.

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{BufferState, LogBuffer};

#[derive(Clone, Default)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

fn buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

fn supported(cpus: u32, memory: u32) -> HostFacts {
    HostFacts {
        unsupported_reason: None,
        logical_cpus: cpus,
        total_memory_gib: memory,
        disk_gib: 500,
    }
}

/// A supported machine whose disk is `disk` GiB.
fn with_disk(disk: u32) -> HostFacts {
    HostFacts { disk_gib: disk, ..supported(8, 16) }
}

fn unsupported() -> HostFacts {
    HostFacts {
        unsupported_reason: Some(UnsupportedReason::KvmMissing),
        logical_cpus: 8,
        total_memory_gib: 16,
        disk_gib: 500,
    }
}

fn settings(mode: IsolationMode) -> IsolationSettings {
    IsolationSettings { mode, ..IsolationSettings::default() }
}

// GSS-FR-YDKS: a store that holds no record reads as `none`, unset CPU, RAM,
// and disk size, the switch on, and an empty list.
#[test]
fn an_empty_store_reads_the_defaults() {
    let store = GlobalSettingsStore::in_memory();
    let record = store.load_isolation().unwrap();
    assert_eq!(record.mode, IsolationMode::None);
    assert_eq!(record.cpus, None);
    assert_eq!(record.memory_gib, None);
    assert_eq!(record.disk_gib, None);
    assert!(record.block_local_network);
    assert!(record.allowed_local_hosts.is_empty());
}

// GSS-FR-YDKS: a record written without the switch reads it as on.
#[test]
fn a_partial_record_keeps_the_switch_on() {
    let record: IsolationRecord = toml::from_str("mode = \"microsandbox\"\ncpus = 2\n").unwrap();
    assert_eq!(record.mode, IsolationMode::Microsandbox);
    assert_eq!(record.cpus, Some(2));
    // A record written before the disk size existed reads it as unset.
    assert_eq!(record.disk_gib, None);
    assert!(record.block_local_network);
}

// GSS-FR-YDKS: the record survives a write and a read of `synthesis.toml`.
#[test]
fn the_record_persists_in_the_user_global_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let record = IsolationRecord {
        mode: IsolationMode::Microsandbox,
        cpus: Some(3),
        memory_gib: Some(6),
        disk_gib: Some(100),
        block_local_network: false,
        allowed_local_hosts: vec!["10.0.0.5".to_string()],
    };
    store.save_isolation(&record).unwrap();
    let reopened = GlobalSettingsStore::with_path(path);
    assert_eq!(reopened.load_isolation().unwrap(), record);
}

// GSS-FR-WHYC: unset values take the defaults, bounded by the machine.
#[test]
fn the_effective_values_are_the_defaults_bounded_by_the_machine() {
    assert_eq!(effective_cpus(None, &supported(16, 64)), 4);
    assert_eq!(effective_memory_gib(None, &supported(16, 64)), 8);
    assert_eq!(effective_cpus(None, &supported(2, 4)), 2);
    assert_eq!(effective_memory_gib(None, &supported(2, 4)), 4);
    assert_eq!(effective_cpus(Some(12), &supported(8, 4)), 8);
    assert_eq!(effective_memory_gib(Some(32), &supported(8, 16)), 16);
    assert_eq!(effective_cpus(Some(0), &supported(8, 16)), 1);
    assert_eq!(effective_memory_gib(Some(0), &supported(8, 16)), 1);
}

// GSS-FR-DPWS, GSS-FR-WHYC: the state carries the facts and the effective
// values beside the stored record.
#[test]
fn the_state_carries_the_facts_and_the_effective_values() {
    let state = state_of(&IsolationRecord::default(), &supported(6, 12));
    assert!(state.host.microsandbox_supported);
    assert_eq!(state.host.unsupported_reason, None);
    assert_eq!(state.host.logical_cpus, 6);
    assert_eq!(state.host.total_memory_gib, 12);
    assert_eq!(state.effective_cpus, 4);
    assert_eq!(state.effective_memory_gib, 8);
    let state = state_of(&IsolationRecord::default(), &unsupported());
    assert!(!state.host.microsandbox_supported);
    assert_eq!(state.host.unsupported_reason, Some(UnsupportedReason::KvmMissing));
}

// GSS-FR-DPWS: the facts of the machine running the test are well formed.
#[test]
fn the_facts_of_this_machine_are_well_formed() {
    let host = read_host();
    assert!(host.logical_cpus >= 1);
    assert!(host.total_memory_gib >= 1);
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        assert_eq!(host.unsupported_reason, Some(UnsupportedReason::UnsupportedPlatform));
    }
}

// GSS-FR-OAKR: Microsandbox on a machine that cannot run it is refused, and
// nothing is written.
#[test]
fn microsandbox_on_an_unsupported_machine_is_refused() {
    let store = GlobalSettingsStore::in_memory();
    let error = save_isolation_settings_with(
        &Silent,
        buffer(),
        &store,
        &unsupported(),
        settings(IsolationMode::Microsandbox),
    )
    .unwrap_err();
    assert_eq!(error, ERR_ISOLATION_UNSUPPORTED);
    assert_eq!(store.load_isolation().unwrap().mode, IsolationMode::None);
    // `none` stays savable on such a machine.
    assert!(save_isolation_settings_with(
        &Silent,
        buffer(),
        &store,
        &unsupported(),
        settings(IsolationMode::None)
    )
    .is_ok());
}

// GSS-FR-OAKR: a CPU count and a RAM outside the machine are refused.
#[test]
fn values_outside_the_machine_are_refused() {
    let host = supported(4, 8);
    let mut over = settings(IsolationMode::Microsandbox);
    over.cpus = Some(5);
    assert_eq!(validate(&over, &host).unwrap_err(), ERR_CPUS_OUT_OF_RANGE);
    over.cpus = Some(0);
    assert_eq!(validate(&over, &host).unwrap_err(), ERR_CPUS_OUT_OF_RANGE);
    over.cpus = Some(4);
    over.memory_gib = Some(9);
    assert_eq!(validate(&over, &host).unwrap_err(), ERR_MEMORY_OUT_OF_RANGE);
    over.memory_gib = Some(0);
    assert_eq!(validate(&over, &host).unwrap_err(), ERR_MEMORY_OUT_OF_RANGE);
    over.memory_gib = Some(8);
    assert!(validate(&over, &host).is_ok());
}

// GSS-FR-BVHS: addresses, ranges, and host names are accepted, and other
// forms are refused with the entry named.
#[test]
fn allowed_hosts_are_validated() {
    for good in ["10.0.0.5", "192.168.1.0/24", "fd00::/8", "::1", "gateway.lan", "localhost"] {
        assert!(classify_allowed_host(good).is_some(), "{good}");
    }
    for bad in ["", "10.0.0.0/33", "fd00::/129", "-bad.lan", "a b", "host_name", "http://x"] {
        assert!(classify_allowed_host(bad).is_none(), "{bad}");
    }
    assert!(classify_allowed_host(&"a".repeat(254)).is_none());
    let mut record = settings(IsolationMode::None);
    record.allowed_local_hosts = vec!["nope!".to_string()];
    assert_eq!(
        validate(&record, &supported(4, 8)).unwrap_err(),
        format!("{ERR_ALLOWED_HOST_INVALID}: nope!")
    );
}

// GSS-FR-BVHS: a repeat, ignoring case, and more than 64 entries are refused;
// entries are trimmed.
#[test]
fn repeats_and_long_lists_are_refused() {
    let host = supported(4, 8);
    let mut record = settings(IsolationMode::None);
    record.allowed_local_hosts = vec!["Gateway.lan".to_string(), " gateway.LAN ".to_string()];
    assert_eq!(
        validate(&record, &host).unwrap_err(),
        format!("{ERR_ALLOWED_HOST_DUPLICATE}: gateway.LAN")
    );
    record.allowed_local_hosts = (0..65).map(|i| format!("10.0.0.{i}")).collect();
    assert_eq!(validate(&record, &host).unwrap_err(), ERR_ALLOWED_HOSTS_TOO_MANY);
    record.allowed_local_hosts = vec!["  10.0.0.1  ".to_string()];
    assert_eq!(validate(&record, &host).unwrap().allowed_local_hosts, vec!["10.0.0.1"]);
}

// EAC-FR-DMYB: loopback and `localhost` allow this machine; an address and a
// range allow that range; a name allows that name.
#[test]
fn allowed_hosts_are_classified_for_the_sandbox() {
    assert_eq!(classify_allowed_host("localhost"), Some(AllowedHost::HostMachine));
    assert_eq!(classify_allowed_host("127.0.0.1"), Some(AllowedHost::HostMachine));
    assert_eq!(classify_allowed_host("127.0.0.0/8"), Some(AllowedHost::HostMachine));
    assert_eq!(classify_allowed_host("::1"), Some(AllowedHost::HostMachine));
    assert_eq!(
        classify_allowed_host("10.1.2.3"),
        Some(AllowedHost::Network("10.1.2.3/32".to_string()))
    );
    assert_eq!(
        classify_allowed_host("10.1.0.0/16"),
        Some(AllowedHost::Network("10.1.0.0/16".to_string()))
    );
    assert_eq!(
        classify_allowed_host("Gateway.LAN"),
        Some(AllowedHost::HostName("gateway.lan".to_string()))
    );
}

// GSS-FR-NCAQ: `none` resolves to `none`, and `microsandbox` resolves to the
// bounded values even where the machine cannot run it.
#[test]
fn the_record_resolves_with_bounded_values() {
    assert_eq!(
        resolve_with(&IsolationRecord::default(), &supported(8, 16)),
        ResolvedIsolation::None
    );
    let record = IsolationRecord {
        mode: IsolationMode::Microsandbox,
        cpus: Some(32),
        memory_gib: None,
        disk_gib: Some(100),
        block_local_network: true,
        allowed_local_hosts: vec!["localhost".to_string(), "::1".to_string()],
    };
    let ResolvedIsolation::Microsandbox(resolved) = resolve_with(&record, &supported(8, 16)) else {
        panic!("microsandbox resolves to microsandbox");
    };
    assert_eq!(resolved.cpus, 8);
    assert_eq!(resolved.memory_gib, 8);
    assert_eq!(resolved.disk_gib, 100);
    let unset = IsolationRecord { disk_gib: None, ..record.clone() };
    let ResolvedIsolation::Microsandbox(resolved_unset) = resolve_with(&unset, &supported(8, 16))
    else {
        panic!("microsandbox resolves to microsandbox");
    };
    assert_eq!(resolved_unset.disk_gib, 64);
    assert_eq!(resolved.allowed, vec![AllowedHost::HostMachine]);
    assert_eq!(resolved.unsupported_reason, None);
    let ResolvedIsolation::Microsandbox(resolved) = resolve_with(&record, &unsupported()) else {
        panic!("an unsupported machine does not fall back to none");
    };
    assert_eq!(resolved.unsupported_reason, Some(UnsupportedReason::KvmMissing));
}

// GSS-FR-OAKR: a valid save persists and returns the new state, and its log
// record names no entry.
#[test]
fn a_valid_save_persists_and_logs_no_entry() {
    let store = GlobalSettingsStore::in_memory();
    let log = buffer();
    let mut record = settings(IsolationMode::Microsandbox);
    record.allowed_local_hosts = vec!["secret-host.lan".to_string()];
    let state =
        save_isolation_settings_with(&Silent, log, &store, &supported(8, 16), record).unwrap();
    assert_eq!(state.settings.mode, IsolationMode::Microsandbox);
    assert_eq!(store.load_isolation().unwrap().allowed_local_hosts, vec!["secret-host.lan"]);
    let page = log
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..crate::logging::LogFilter::default()
            },
            None,
            100,
        )
        .unwrap();
    let text = format!("{:?}", page.records);
    assert!(text.contains("isolation settings saved"));
    assert!(!text.contains("secret-host"));
}

// GSS contract surface: the wire shape is camelCase.
#[test]
fn the_wire_shape_is_camel_case() {
    let state = state_of(&IsolationRecord::default(), &supported(8, 16));
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["settings"]["mode"], "none");
    assert_eq!(json["settings"]["blockLocalNetwork"], true);
    assert_eq!(json["host"]["microsandboxSupported"], true);
    assert_eq!(json["effectiveMemoryGib"], 8);
    assert_eq!(json["effectiveDiskGib"], 64);
    assert_eq!(json["host"]["diskGib"], 500);
    assert!(json["settings"]["diskGib"].is_null());
}

// GSS-FR-JZMH: an unset disk size is 64 GiB, never above the host disk, never
// below 8 GiB, and the host disk size where that disk is smaller than 8 GiB.
#[test]
fn the_effective_disk_size_is_bounded_by_the_host_disk() {
    assert_eq!(effective_disk_gib(None, &with_disk(500)), 64);
    assert_eq!(effective_disk_gib(None, &with_disk(40)), 40);
    assert_eq!(effective_disk_gib(Some(200), &with_disk(500)), 200);
    assert_eq!(effective_disk_gib(Some(900), &with_disk(500)), 500);
    assert_eq!(effective_disk_gib(Some(2), &with_disk(500)), 8);
    assert_eq!(effective_disk_gib(None, &with_disk(5)), 5);
    assert_eq!(effective_disk_gib(Some(8), &with_disk(5)), 5);
}

// GSS-FR-TLXE: a disk size below 8 GiB or above the host disk is refused and
// nothing is written; the bounds themselves and an unset size are valid. On a
// host disk smaller than 8 GiB the host disk size is the one valid size.
#[test]
fn a_disk_size_outside_the_bounds_is_refused() {
    let host = with_disk(100);
    for (disk, refused) in [(7, true), (101, true), (8, false), (100, false)] {
        let store = GlobalSettingsStore::in_memory();
        let mut record = settings(IsolationMode::Microsandbox);
        record.disk_gib = Some(disk);
        let result = save_isolation_settings_with(&Silent, buffer(), &store, &host, record);
        if refused {
            assert_eq!(result.unwrap_err(), ERR_DISK_OUT_OF_RANGE, "{disk}");
            assert_eq!(store.load_isolation().unwrap(), IsolationRecord::default(), "{disk}");
        } else {
            assert_eq!(result.unwrap().effective_disk_gib, disk, "{disk}");
            assert_eq!(store.load_isolation().unwrap().disk_gib, Some(disk));
        }
    }
    let store = GlobalSettingsStore::in_memory();
    let state = save_isolation_settings_with(
        &Silent,
        buffer(),
        &store,
        &host,
        settings(IsolationMode::Microsandbox),
    )
    .expect("an unset disk size is valid");
    assert_eq!(state.effective_disk_gib, 64);

    let small = with_disk(5);
    for (disk, refused) in [(4, true), (6, true), (5, false)] {
        let store = GlobalSettingsStore::in_memory();
        let mut record = settings(IsolationMode::Microsandbox);
        record.disk_gib = Some(disk);
        let result = save_isolation_settings_with(&Silent, buffer(), &store, &small, record);
        assert_eq!(result.is_err(), refused, "{disk} on a 5 GiB disk");
    }
}

// GSS-FR-RQWB: the disk size is the size of the file system that holds the
// directory, in whole GiB, and 64 GiB where the system reports no size.
#[test]
fn the_host_disk_size_is_the_file_system_of_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = crate::fs::probe_volume_size(dir.path()).expect("a size");
    let expected = u32::try_from(bytes / (1024 * 1024 * 1024)).unwrap().max(1);
    assert_eq!(disk_gib_of(dir.path()), expected);
    assert_eq!(disk_gib_of(&dir.path().join("absent")), DEFAULT_DISK_GIB);
    assert!(read_host().disk_gib >= 1);
}
