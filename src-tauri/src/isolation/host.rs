//! GSS-FR-DPWS: the facts of this machine the Isolation section is bounded by.
//!
//! They are read at each load and never stored, so a store copied to another
//! machine carries no stale claim about what that machine can do.

use super::UnsupportedReason;

/// What this machine can run, and how large it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFacts {
    /// `None` where this machine can run Microsandbox.
    pub unsupported_reason: Option<UnsupportedReason>,
    /// At least 1.
    pub logical_cpus: u32,
    /// Whole GiB, at least 1.
    pub total_memory_gib: u32,
    /// GSS-FR-RQWB: the size of the disk that holds the application data, in
    /// whole GiB, at least 1.
    pub disk_gib: u32,
}

const GIB: u64 = 1024 * 1024 * 1024;

/// GSS-FR-DPWS: read the facts of this machine.
pub fn read_host() -> HostFacts {
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| u32::try_from(n.get()).unwrap_or(u32::MAX))
        .unwrap_or(1)
        .max(1);
    let total_memory_gib =
        u32::try_from(total_memory_bytes().unwrap_or(GIB) / GIB).unwrap_or(u32::MAX).max(1);
    HostFacts {
        unsupported_reason: unsupported_reason(),
        logical_cpus,
        total_memory_gib,
        disk_gib: disk_gib(),
    }
}

/// GSS-FR-RQWB: the size of the file system that holds `app_data_dir()`.
fn disk_gib() -> u32 {
    match crate::fs::app_data_dir() {
        Ok(dir) => disk_gib_of(&dir),
        Err(_) => super::DEFAULT_DISK_GIB,
    }
}

/// GSS-FR-RQWB: the size, in whole GiB, of the file system that holds `dir`.
/// Where the system does not report it, 64 GiB, so the Disk control still
/// offers its default.
pub fn disk_gib_of(dir: &std::path::Path) -> u32 {
    crate::fs::probe_volume_size(dir)
        .map(|bytes| u32::try_from(bytes / GIB).unwrap_or(u32::MAX))
        .unwrap_or(super::DEFAULT_DISK_GIB)
        .max(1)
}

/// Linux: supported where the user can open `/dev/kvm` for reading and writing.
#[cfg(target_os = "linux")]
fn unsupported_reason() -> Option<UnsupportedReason> {
    match crate::fs::probe_kvm_device() {
        crate::fs::KvmDevice::Accessible => None,
        crate::fs::KvmDevice::Missing => Some(UnsupportedReason::KvmMissing),
        crate::fs::KvmDevice::Inaccessible => Some(UnsupportedReason::KvmInaccessible),
    }
}

/// macOS: supported on Apple Silicon alone.
#[cfg(target_os = "macos")]
fn unsupported_reason() -> Option<UnsupportedReason> {
    if cfg!(target_arch = "aarch64") {
        None
    } else {
        Some(UnsupportedReason::AppleSiliconRequired)
    }
}

/// Every other platform holds no microsandbox code (EAC-FR-HMBJ).
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn unsupported_reason() -> Option<UnsupportedReason> {
    Some(UnsupportedReason::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn total_memory_bytes() -> Option<u64> {
    // SAFETY: `sysinfo` writes into the struct it is given and reads nothing
    // else. A zeroed `libc::sysinfo` is a valid value of that plain C struct.
    let mut info: libc::sysinfo = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a valid, writable `sysinfo` for the whole call.
    if unsafe { libc::sysinfo(&mut info) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)] // The field widths differ between targets.
    Some((info.totalram as u64).saturating_mul(info.mem_unit.max(1) as u64))
}

#[cfg(target_os = "macos")]
fn total_memory_bytes() -> Option<u64> {
    let mut size: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name is a NUL-terminated C string, and `size` and `len`
    // describe a writable buffer of exactly `len` bytes.
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut size as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0).then_some(size)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn total_memory_bytes() -> Option<u64> {
    None
}
