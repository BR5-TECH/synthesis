//! EAC-FR-HXDN / EAC-FR-TMQS: `probe_directory_identity` and
//! `probe_entry_is_dir`, the two questions about a mount source of a sandbox
//! VM.
//!
//! A mount source can stand outside every root an `FsAccess` instance holds,
//! such as a Codex login directory. This function asks the operating system for
//! the device, the inode, or the kind of the entry and nothing else. It reads
//! no content and mutates nothing.

use std::path::Path;

/// The device and the inode of the entry at `path`, following symbolic links.
/// `None` where the entry is not there or the platform reports no inode.
#[cfg(unix)]
pub fn probe_directory_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()))
}

/// A platform with no inode reports no identity.
#[cfg(not(unix))]
pub fn probe_directory_identity(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// EAC-FR-TMQS: whether the entry at `path` is a directory, following symbolic
/// links. `None` where nothing is there.
pub fn probe_entry_is_dir(path: &Path) -> Option<bool> {
    std::fs::metadata(path).ok().map(|meta| meta.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    // EAC-FR-HXDN: a directory made again at the same path while the old one
    // still exists has another identity, and an absent path has none.
    #[test]
    fn a_directory_made_again_has_another_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("checkout");
        std::fs::create_dir(&path).unwrap();
        let first = probe_directory_identity(&path);
        assert!(first.is_some());
        assert_eq!(probe_directory_identity(&path), first);

        // Renamed aside rather than deleted, so the inode cannot be given back.
        std::fs::rename(&path, root.path().join("aside")).unwrap();
        assert_eq!(probe_directory_identity(&path), None);
        std::fs::create_dir(&path).unwrap();
        assert_ne!(probe_directory_identity(&path), first);
    }

    // EAC-FR-TMQS: the kind of an entry, and nothing for an absent one.
    #[test]
    fn an_entry_reports_its_kind() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"x").unwrap();
        assert_eq!(probe_entry_is_dir(root.path()), Some(true));
        assert_eq!(probe_entry_is_dir(&root.path().join("file")), Some(false));
        assert_eq!(probe_entry_is_dir(&root.path().join("absent")), None);
    }
}
