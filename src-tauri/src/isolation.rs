//! The user-global **isolation settings** of graduation agent launches.
//!
//! Specification: `specifications/core/GSS-global-settings-storage.md`
//! (GSS-FR-YDKS, GSS-FR-DPWS, GSS-FR-WHYC, GSS-FR-OAKR, GSS-FR-BVHS,
//! GSS-FR-NCAQ), surfaced by `specifications/ui/GLS-global-settings.md`
//! GLS-FR-YAEA through GLS-FR-GHYV, and read by
//! `specifications/tools/EAC-execute-agent-cli.md` EAC-FR-SNMS.
//!
//! This module persists the mode, the CPU count, the RAM, the local network
//! switch, and the allowed local hosts. It also reads the facts of this
//! machine — whether it can run Microsandbox, its logical CPU count, and its
//! total RAM — at each load, and never stores them. It boots no VM and reaches
//! no network.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

mod host;
pub use host::{disk_gib_of, read_host, HostFacts};

/// GSS-FR-WHYC: the CPU count an unset record uses.
pub const DEFAULT_CPUS: u32 = 4;
/// GSS-FR-WHYC: the RAM, in GiB, an unset record uses.
pub const DEFAULT_MEMORY_GIB: u32 = 8;
/// GSS-FR-JZMH: the disk size, in GiB, an unset record uses.
pub const DEFAULT_DISK_GIB: u32 = 64;
/// GSS-FR-JZMH / GSS-FR-TLXE: the smallest disk size, in GiB.
pub const MIN_DISK_GIB: u32 = 8;
/// GSS-FR-BVHS: the most entries the allowed local hosts list holds.
pub const MAX_ALLOWED_HOSTS: usize = 64;
/// GSS-FR-BVHS: the longest host name an entry may be.
pub const MAX_HOST_NAME_LEN: usize = 253;

/// The typed refusals of `save_isolation_settings` (GSS contract surface).
pub const ERR_ISOLATION_UNSUPPORTED: &str = "isolation_unsupported";
pub const ERR_CPUS_OUT_OF_RANGE: &str = "cpus_out_of_range";
pub const ERR_MEMORY_OUT_OF_RANGE: &str = "memory_out_of_range";
pub const ERR_DISK_OUT_OF_RANGE: &str = "disk_out_of_range";
pub const ERR_ALLOWED_HOST_INVALID: &str = "allowed_host_invalid";
pub const ERR_ALLOWED_HOST_DUPLICATE: &str = "allowed_host_duplicate";
pub const ERR_ALLOWED_HOSTS_TOO_MANY: &str = "allowed_hosts_too_many";

// ---------------------------------------------------------------------------
// The shapes (GSS contract surface)
// ---------------------------------------------------------------------------

/// Which isolation graduation agent launches run under (GLS-FR-YAEA).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    #[default]
    None,
    Microsandbox,
}

impl IsolationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationMode::None => "none",
            IsolationMode::Microsandbox => "microsandbox",
        }
    }
}

/// The record as it travels to and from the frontend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationSettings {
    pub mode: IsolationMode,
    pub cpus: Option<u32>,
    pub memory_gib: Option<u32>,
    pub disk_gib: Option<u32>,
    pub block_local_network: bool,
    pub allowed_local_hosts: Vec<String>,
}

impl Default for IsolationSettings {
    fn default() -> Self {
        IsolationRecord::default().into()
    }
}

/// Why this machine cannot run Microsandbox (GSS-FR-DPWS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    UnsupportedPlatform,
    AppleSiliconRequired,
    KvmMissing,
    KvmInaccessible,
}

impl UnsupportedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UnsupportedReason::UnsupportedPlatform => "unsupported_platform",
            UnsupportedReason::AppleSiliconRequired => "apple_silicon_required",
            UnsupportedReason::KvmMissing => "kvm_missing",
            UnsupportedReason::KvmInaccessible => "kvm_inaccessible",
        }
    }
}

/// The facts of this machine, as the frontend reads them (GSS-FR-DPWS).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationHost {
    pub microsandbox_supported: bool,
    pub unsupported_reason: Option<UnsupportedReason>,
    pub logical_cpus: u32,
    pub total_memory_gib: u32,
    pub disk_gib: u32,
}

/// What `load_isolation_settings` and `save_isolation_settings` return.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolationState {
    pub settings: IsolationSettings,
    pub host: IsolationHost,
    pub effective_cpus: u32,
    pub effective_memory_gib: u32,
    pub effective_disk_gib: u32,
}

// ---------------------------------------------------------------------------
// The persisted record (GSS-FR-YDKS)
// ---------------------------------------------------------------------------

/// The isolation settings as they sit in `synthesis.toml` (GSS-FR-YDKS).
///
/// A store that holds no record reads as mode `none`, CPU count, RAM, and disk
/// size unset,
/// the local network switch on, and an empty list. The switch defaults to on
/// for a record written without it as well, so a partial record never opens
/// the local network by accident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IsolationRecord {
    pub mode: IsolationMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_gib: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_gib: Option<u32>,
    pub block_local_network: bool,
    pub allowed_local_hosts: Vec<String>,
}

impl Default for IsolationRecord {
    fn default() -> Self {
        IsolationRecord {
            mode: IsolationMode::None,
            cpus: None,
            memory_gib: None,
            disk_gib: None,
            block_local_network: true,
            allowed_local_hosts: Vec::new(),
        }
    }
}

impl From<IsolationRecord> for IsolationSettings {
    fn from(record: IsolationRecord) -> Self {
        IsolationSettings {
            mode: record.mode,
            cpus: record.cpus,
            memory_gib: record.memory_gib,
            disk_gib: record.disk_gib,
            block_local_network: record.block_local_network,
            allowed_local_hosts: record.allowed_local_hosts,
        }
    }
}

// ---------------------------------------------------------------------------
// Effective values and validation
// ---------------------------------------------------------------------------

impl HostFacts {
    /// The outbound shape of these facts.
    pub fn outbound(&self) -> IsolationHost {
        IsolationHost {
            microsandbox_supported: self.unsupported_reason.is_none(),
            unsupported_reason: self.unsupported_reason,
            logical_cpus: self.logical_cpus.max(1),
            total_memory_gib: self.total_memory_gib.max(1),
            disk_gib: self.disk_gib.max(1),
        }
    }
}

/// GSS-FR-WHYC: the CPU count a launch uses — the stored count or the default,
/// never above the logical CPU count and never below 1.
pub fn effective_cpus(stored: Option<u32>, host: &HostFacts) -> u32 {
    stored.unwrap_or(DEFAULT_CPUS).min(host.logical_cpus.max(1)).max(1)
}

/// GSS-FR-WHYC: the RAM, in GiB, a launch uses — the stored RAM or the
/// default, never above the total RAM and never below 1.
pub fn effective_memory_gib(stored: Option<u32>, host: &HostFacts) -> u32 {
    stored.unwrap_or(DEFAULT_MEMORY_GIB).min(host.total_memory_gib.max(1)).max(1)
}

/// GSS-FR-JZMH: the disk size, in GiB, a launch uses — the stored size or the
/// default, never above the host disk and never below 8 GiB, except on a host
/// disk smaller than that, where it is the host disk size.
pub fn effective_disk_gib(stored: Option<u32>, host: &HostFacts) -> u32 {
    let host_disk = host.disk_gib.max(1);
    stored.unwrap_or(DEFAULT_DISK_GIB).max(MIN_DISK_GIB).min(host_disk)
}

/// The state a load and a save both return.
pub fn state_of(record: &IsolationRecord, host: &HostFacts) -> IsolationState {
    IsolationState {
        settings: record.clone().into(),
        host: host.outbound(),
        effective_cpus: effective_cpus(record.cpus, host),
        effective_memory_gib: effective_memory_gib(record.memory_gib, host),
        effective_disk_gib: effective_disk_gib(record.disk_gib, host),
    }
}

/// What one allowed local host lets a sandbox reach (EAC-FR-DMYB).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AllowedHost {
    /// An address or a CIDR range, in CIDR form.
    Network(String),
    /// A host name, which allows the addresses it resolves to.
    HostName(String),
    /// `localhost` or a loopback address: this machine, through the sandbox
    /// gateway. Inside the VM, loopback is the VM itself.
    HostMachine,
}

/// GSS-FR-BVHS: read one entry, or `None` where it has no valid form.
pub fn classify_allowed_host(entry: &str) -> Option<AllowedHost> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    if let Ok(address) = entry.parse::<IpAddr>() {
        if address.is_loopback() {
            return Some(AllowedHost::HostMachine);
        }
        let prefix = if address.is_ipv4() { 32 } else { 128 };
        return Some(AllowedHost::Network(format!("{address}/{prefix}")));
    }
    if let Some((address, prefix)) = entry.split_once('/') {
        let address = address.parse::<IpAddr>().ok()?;
        let prefix: u8 = prefix.parse().ok()?;
        let max = if address.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return None;
        }
        if address.is_loopback() {
            return Some(AllowedHost::HostMachine);
        }
        return Some(AllowedHost::Network(format!("{address}/{prefix}")));
    }
    if !is_host_name(entry) {
        return None;
    }
    let name = entry.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") {
        return Some(AllowedHost::HostMachine);
    }
    Some(AllowedHost::HostName(name))
}

/// GSS-FR-BVHS: at most 253 characters, made of letters, digits, hyphens, and
/// dots, each label 1 to 63 characters that neither starts nor ends with a
/// hyphen.
fn is_host_name(entry: &str) -> bool {
    let name = entry.strip_suffix('.').unwrap_or(entry);
    if name.is_empty() || name.len() > MAX_HOST_NAME_LEN {
        return false;
    }
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// GSS-FR-OAKR / GSS-FR-BVHS: validate a record before anything is written,
/// and return it with every entry trimmed.
pub fn validate(settings: &IsolationSettings, host: &HostFacts) -> Result<IsolationRecord, String> {
    if settings.mode == IsolationMode::Microsandbox && host.unsupported_reason.is_some() {
        return Err(ERR_ISOLATION_UNSUPPORTED.to_string());
    }
    if let Some(cpus) = settings.cpus {
        if cpus < 1 || cpus > host.logical_cpus.max(1) {
            return Err(ERR_CPUS_OUT_OF_RANGE.to_string());
        }
    }
    if let Some(memory) = settings.memory_gib {
        if memory < 1 || memory > host.total_memory_gib.max(1) {
            return Err(ERR_MEMORY_OUT_OF_RANGE.to_string());
        }
    }
    // GSS-FR-TLXE: from 8 GiB, or from the host disk size on a smaller disk,
    // to the host disk size. An unset disk size is valid.
    if let Some(disk) = settings.disk_gib {
        let host_disk = host.disk_gib.max(1);
        if disk < MIN_DISK_GIB.min(host_disk) || disk > host_disk {
            return Err(ERR_DISK_OUT_OF_RANGE.to_string());
        }
    }
    if settings.allowed_local_hosts.len() > MAX_ALLOWED_HOSTS {
        return Err(ERR_ALLOWED_HOSTS_TOO_MANY.to_string());
    }
    let mut seen: Vec<String> = Vec::new();
    let mut entries = Vec::with_capacity(settings.allowed_local_hosts.len());
    for raw in &settings.allowed_local_hosts {
        let entry = raw.trim().to_string();
        if classify_allowed_host(&entry).is_none() {
            return Err(format!("{ERR_ALLOWED_HOST_INVALID}: {entry}"));
        }
        let folded = entry.to_ascii_lowercase();
        if seen.contains(&folded) {
            return Err(format!("{ERR_ALLOWED_HOST_DUPLICATE}: {entry}"));
        }
        seen.push(folded);
        entries.push(entry);
    }
    Ok(IsolationRecord {
        mode: settings.mode,
        cpus: settings.cpus,
        memory_gib: settings.memory_gib,
        disk_gib: settings.disk_gib,
        block_local_network: settings.block_local_network,
        allowed_local_hosts: entries,
    })
}

// ---------------------------------------------------------------------------
// The internal read path (GSS-FR-NCAQ)
// ---------------------------------------------------------------------------

/// What a launch reads the isolation as (GSS-FR-NCAQ).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedIsolation {
    None,
    Microsandbox(SandboxSettings),
}

/// The bounded values a sandbox boots with (GSS-FR-WHYC, EAC-FR-CGCE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxSettings {
    pub cpus: u32,
    pub memory_gib: u32,
    /// EAC-FR-QDVN: the root disk and the write cap of each writable mount.
    pub disk_gib: u32,
    pub block_local_network: bool,
    /// Sorted and without repeats, so two records that allow the same set
    /// compare equal (EAC-FR-UEFI).
    pub allowed: Vec<AllowedHost>,
    /// Where the machine cannot run Microsandbox, why. The launch reports it
    /// rather than falling back to `none` (GSS-FR-NCAQ).
    pub unsupported_reason: Option<UnsupportedReason>,
}

/// GSS-FR-NCAQ: the isolation the stored record and these facts resolve to.
/// It probes nothing beyond what the facts already hold.
pub fn resolve_with(record: &IsolationRecord, host: &HostFacts) -> ResolvedIsolation {
    match record.mode {
        IsolationMode::None => ResolvedIsolation::None,
        IsolationMode::Microsandbox => {
            let mut allowed: Vec<AllowedHost> = record
                .allowed_local_hosts
                .iter()
                .filter_map(|e| classify_allowed_host(e))
                .collect();
            allowed.sort();
            allowed.dedup();
            ResolvedIsolation::Microsandbox(SandboxSettings {
                cpus: effective_cpus(record.cpus, host),
                memory_gib: effective_memory_gib(record.memory_gib, host),
                disk_gib: effective_disk_gib(record.disk_gib, host),
                block_local_network: record.block_local_network,
                allowed,
                unsupported_reason: host.unsupported_reason,
            })
        }
    }
}

/// GSS-FR-NCAQ: the isolation a launch uses, read from the store and the facts
/// of this machine. Not a Tauri command.
pub fn resolve_isolation(store: &GlobalSettingsStore) -> Result<ResolvedIsolation, String> {
    let record = store.load_isolation()?;
    if record.mode == IsolationMode::None {
        // The facts are not read for a mode that does not use them.
        return Ok(ResolvedIsolation::None);
    }
    Ok(resolve_with(&record, &read_host()))
}

// ---------------------------------------------------------------------------
// Tauri commands (GSS contract surface)
// ---------------------------------------------------------------------------

use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink, BUFFER};
use tauri::State;

const DOMAINS: [Domain; 1] = [Domain::Backend];

/// GSS-FR-DPWS: the stored record with the facts of this machine. Boots no VM,
/// runs no executable, and reaches no network.
#[tauri::command]
pub fn load_isolation_settings(
    store: State<'_, GlobalSettingsStore>,
) -> Result<IsolationState, String> {
    Ok(state_of(&store.load_isolation()?, &read_host()))
}

/// GSS-FR-OAKR: validate and persist the record, and return the new state.
#[tauri::command]
pub fn save_isolation_settings(
    settings: IsolationSettings,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
) -> Result<IsolationState, String> {
    save_isolation_settings_with(&app, &BUFFER, &store, &read_host(), settings)
}

pub fn save_isolation_settings_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &GlobalSettingsStore,
    host: &HostFacts,
    settings: IsolationSettings,
) -> Result<IsolationState, String> {
    let record = match validate(&settings, host) {
        Ok(record) => record,
        Err(error) => {
            // The refusal code alone: an entry the author typed is theirs and
            // can name a host on their network.
            let code = error.split(':').next().unwrap_or_default().to_string();
            logging::log_warn(
                sink,
                buffer,
                &DOMAINS,
                "isolation settings refused",
                log_fields! { "reason" => code, "mode" => settings.mode.as_str() },
            );
            return Err(error);
        }
    };
    store.save_isolation(&record)?;
    logging::log_info(
        sink,
        buffer,
        &DOMAINS,
        "isolation settings saved",
        log_fields! {
            "mode" => record.mode.as_str(),
            "cpus" => effective_cpus(record.cpus, host),
            "memory_gib" => effective_memory_gib(record.memory_gib, host),
            "disk_gib" => effective_disk_gib(record.disk_gib, host),
            "block_local_network" => record.block_local_network,
            "allowed_hosts" => record.allowed_local_hosts.len()
        },
    );
    Ok(state_of(&record, host))
}

#[cfg(test)]
mod tests;
