//! The digest of a change folder (`OFP-openspec-failure-policy.md`
//! OFP-FR-KYXI), which tells a merge notice whether the change files moved
//! since the last notice (OFP-FR-PSVR).

use sha2::{Digest, Sha256};

use crate::fs::{EntryKind, RootFs};

/// OFP-FR-KYXI: the sha256 over the change-relative path and the bytes of each
/// regular file below `openspec/changes/<change>/` of the working copy `at`,
/// in path order, as lowercase hex. Each part is preceded by its length, so
/// two different folders cannot give one byte sequence. `None` where the
/// folder or a file of it cannot be read.
pub fn change_folder_digest(at: &RootFs, change: &str) -> Option<String> {
    let folder = at.path().join("openspec/changes").join(change);
    let mut files = Vec::new();
    collect(at, &folder, "", &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in files {
        let bytes = at.read_bytes(folder.join(&rel)).ok()?;
        hasher.update((rel.len() as u64).to_be_bytes());
        hasher.update(rel.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The change-relative path of each regular file below `dir`. A symbolic
/// link and a special file are not followed and add nothing.
fn collect(at: &RootFs, dir: &std::path::Path, rel: &str, out: &mut Vec<String>) -> Option<()> {
    for entry in at.list_dir(dir).ok()? {
        let path =
            if rel.is_empty() { entry.name.clone() } else { format!("{rel}/{}", entry.name) };
        match entry.kind {
            EntryKind::File => out.push(path),
            EntryKind::Dir => collect(at, &dir.join(&entry.name), &path, out)?,
            EntryKind::Symlink | EntryKind::Other => {}
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> (tempfile::TempDir, RootFs) {
        let dir = tempfile::TempDir::new().unwrap();
        let root = RootFs::for_root(dir.path());
        (dir, root)
    }

    fn write(dir: &tempfile::TempDir, rel: &str, body: &str) {
        let path = dir.path().join("openspec/changes/add-logout").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    // OFP-FR-KYXI: a change folder that does not stand has no digest.
    #[test]
    fn a_missing_folder_has_no_digest() {
        let (_dir, root) = folder();
        assert_eq!(change_folder_digest(&root, "add-logout"), None);
    }

    // OFP-FR-KYXI: the digest is lowercase hex of sha256, and it does not
    // depend on the order in which the files were written.
    #[test]
    fn the_digest_is_stable_over_the_order_of_the_files() {
        let (one, first) = folder();
        write(&one, "proposal.md", "# Why\n");
        write(&one, "specs/auth/spec.md", "## ADDED\n");
        write(&one, "tasks.md", "- [ ] Do it\n");
        let (two, second) = folder();
        write(&two, "tasks.md", "- [ ] Do it\n");
        write(&two, "specs/auth/spec.md", "## ADDED\n");
        write(&two, "proposal.md", "# Why\n");
        let digest = change_folder_digest(&first, "add-logout").expect("a digest");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        assert_eq!(change_folder_digest(&second, "add-logout"), Some(digest.clone()));
        assert_eq!(change_folder_digest(&first, "add-logout"), Some(digest), "the same again");
    }

    // OFP-FR-KYXI: the change-relative path is part of the digest, so the same
    // bytes under another name, or in another folder, give another digest;
    // and so do other bytes.
    #[test]
    fn the_path_and_the_bytes_are_in_the_digest() {
        let (one, first) = folder();
        write(&one, "tasks.md", "- [ ] Do it\n");
        let (two, second) = folder();
        write(&two, "notes.md", "- [ ] Do it\n");
        let (three, third) = folder();
        write(&three, "specs/tasks.md", "- [ ] Do it\n");
        let (four, fourth) = folder();
        write(&four, "tasks.md", "- [x] Do it\n");
        let digests: Vec<_> = [&first, &second, &third, &fourth]
            .iter()
            .map(|root| change_folder_digest(root, "add-logout").unwrap())
            .collect();
        let mut unique = digests.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 4, "{digests:?}");
    }

    // OFP-FR-KYXI: a symbolic link adds nothing to the digest.
    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_skipped() {
        let (dir, root) = folder();
        write(&dir, "tasks.md", "- [ ] Do it\n");
        let before = change_folder_digest(&root, "add-logout").unwrap();
        let folder = dir.path().join("openspec/changes/add-logout");
        std::os::unix::fs::symlink(folder.join("tasks.md"), folder.join("link.md")).unwrap();
        assert_eq!(change_folder_digest(&root, "add-logout"), Some(before));
    }
}
