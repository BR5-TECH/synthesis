//! GSS-FR-DPWS: `probe_kvm_device`, the one question about `/dev/kvm`.
//!
//! The KVM device lies outside every root an `FsAccess` instance holds, as the
//! Docker socket of `socket_probe` does. This function asks the operating
//! system whether the user can open the device for reading and writing. It
//! reads no content and creates no VM: the device is opened and closed at once.

/// What the user can do with the KVM device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvmDevice {
    /// The device exists and opens for reading and writing.
    Accessible,
    /// Nothing exists at the device path.
    Missing,
    /// The device exists and the user cannot open it for reading and writing.
    Inaccessible,
}

/// The path Linux exposes KVM at.
pub const KVM_DEVICE_PATH: &str = "/dev/kvm";

/// GSS-FR-DPWS: whether the user can open the KVM device for reading and
/// writing.
#[cfg(target_os = "linux")]
pub fn probe_kvm_device() -> KvmDevice {
    probe_device(std::path::Path::new(KVM_DEVICE_PATH))
}

/// A platform without KVM has no device to find.
#[cfg(not(target_os = "linux"))]
pub fn probe_kvm_device() -> KvmDevice {
    KvmDevice::Missing
}

#[cfg(target_os = "linux")]
fn probe_device(path: &std::path::Path) -> KvmDevice {
    // `symlink_metadata` would refuse a link the distribution placed there, so
    // the entry is followed like the open below follows it.
    if std::fs::metadata(path).is_err() {
        return KvmDevice::Missing;
    }
    match std::fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(_) => KvmDevice::Accessible,
        Err(_) => KvmDevice::Inaccessible,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    // GSS-FR-DPWS: a missing device and a device the user can open are told
    // apart. A plain file stands in for the device, as the test cannot create
    // one.
    #[test]
    fn a_missing_device_and_an_openable_one_are_told_apart() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(probe_device(&dir.path().join("kvm")), KvmDevice::Missing);
        let file = dir.path().join("kvm-file");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(probe_device(&file), KvmDevice::Accessible);
    }
}
