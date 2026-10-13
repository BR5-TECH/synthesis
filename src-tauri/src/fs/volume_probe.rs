//! GSS-FR-RQWB: `probe_volume_size`, the one question about the file system
//! that holds a path.
//!
//! The Isolation section bounds its Disk control by the size of the disk that
//! holds the application data. This function asks the operating system for
//! that size and nothing else. It reads no content and mutates nothing.

use std::path::Path;

/// The total size, in bytes, of the file system that holds `path`. `None`
/// where the system does not answer.
#[cfg(unix)]
pub fn probe_volume_size(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;

    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: a zeroed `statvfs` is a valid value of that plain C struct.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is a NUL-terminated C string and `stats` is a valid,
    // writable `statvfs` for the whole call.
    if unsafe { libc::statvfs(name.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)] // The field widths differ between targets.
    Some((stats.f_blocks as u64).saturating_mul(stats.f_frsize as u64))
}

/// A platform with no `statvfs` reports no size.
#[cfg(not(unix))]
pub fn probe_volume_size(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-RQWB: a directory reports the size of its file system, and a path
    // that is not there reports none.
    #[test]
    fn a_directory_reports_the_size_of_its_file_system() {
        let root = tempfile::tempdir().unwrap();
        let size = probe_volume_size(root.path()).expect("a size");
        assert!(size > 0);
        assert_eq!(probe_volume_size(&root.path().join("absent")), None);
    }
}
