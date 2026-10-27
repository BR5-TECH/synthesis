//! Read-only access to the Git objects of a branch tip, with no checkout
//! (`GTC-git.md` GTC-FR-FWYK, GTC-FR-SHLL, GTC-FR-GUSU).
//!
//! `EMB-embeddings-index.md` builds its root layer from these (EMB-FR-LGLI):
//! the files of the default branch tip are read from the object database, so
//! no worktree, Git index, or ref changes, whatever branch the author has out.

use git2::{ObjectType, Oid, Repository, TreeWalkMode, TreeWalkResult};

/// GTC-FR-FWYK: neither a local branch nor a remote-tracking branch of the
/// primary remote has the requested name.
pub const ERR_BRANCH_MISSING: &str = "branch_missing";

/// GTC-FR-SHLL / GTC-FR-GUSU: the commit or the blob is not in the object
/// database.
pub const ERR_OBJECT_MISSING: &str = "object_missing";

/// The file mode Git records for a symbolic link.
const MODE_LINK: i32 = 0o120000;

/// One file of a tree (GTC-FR-SHLL).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeFile {
    /// Repository-relative, forward-slash path.
    pub path: String,
    pub blob: Oid,
}

/// GTC-FR-FWYK: the commit at the tip of the branch `name` — the local branch
/// when there is one, else the remote-tracking branch of the primary remote.
pub fn branch_tip(repo: &Repository, name: &str) -> Result<Oid, String> {
    let local = repo.find_branch(name, git2::BranchType::Local).ok();
    let remote = || {
        let primary = crate::changes::primary_remote_name(repo)?;
        repo.find_branch(&format!("{primary}/{name}"), git2::BranchType::Remote).ok()
    };
    let branch = local.or_else(remote).ok_or_else(|| ERR_BRANCH_MISSING.to_string())?;
    branch.get().peel_to_commit().map(|c| c.id()).map_err(|_| ERR_BRANCH_MISSING.to_string())
}

/// GTC-FR-SHLL: every file of the tree of `commit` below the repository-relative
/// folder `prefix` (empty for the whole tree). A submodule and a symbolic link
/// are not files (GTC-FR-GUSU). The paths keep the prefix; the caller makes
/// them project-relative.
pub fn list_files_at(
    repo: &Repository,
    commit: Oid,
    prefix: &str,
) -> Result<Vec<TreeFile>, String> {
    let commit = repo.find_commit(commit).map_err(|_| ERR_OBJECT_MISSING.to_string())?;
    let tree = commit.tree().map_err(|_| ERR_OBJECT_MISSING.to_string())?;
    let prefix = prefix.trim_matches('/');
    let mut files = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() != Some(ObjectType::Blob) || entry.filemode() == MODE_LINK {
            return TreeWalkResult::Ok;
        }
        let Ok(name) = entry.name() else {
            return TreeWalkResult::Ok;
        };
        let path = format!("{dir}{name}");
        let inside = prefix.is_empty()
            || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'));
        if inside {
            files.push(TreeFile { path, blob: entry.id() });
        }
        TreeWalkResult::Ok
    })
    .map_err(|_| ERR_OBJECT_MISSING.to_string())?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// GTC-FR-GUSU: the bytes of one blob.
pub fn read_blob(repo: &Repository, blob: Oid) -> Result<Vec<u8>, String> {
    repo.find_blob(blob).map(|b| b.content().to_vec()).map_err(|_| ERR_OBJECT_MISSING.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn commit_files(repo: &Repository, files: &[(&str, &str)], message: &str) -> Oid {
        let workdir = repo.workdir().unwrap().to_path_buf();
        let mut index = repo.index().unwrap();
        for (path, text) in files {
            let full = workdir.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, text).unwrap();
            index.add_path(Path::new(path)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let parents: Vec<git2::Commit<'_>> =
            repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap()
    }

    // GTC-FR-SHLL, GTC-FR-GUSU: the files of a commit come from the object
    // database, filtered by the prefix, and the working copy does not matter.
    #[test]
    fn lists_and_reads_the_files_of_a_commit_without_a_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = crate::embeddings::test_support::init(dir.path());
        let first = commit_files(
            &repo,
            &[("proj/specifications/a.md", "# A\n\nalpha"), ("other/b.md", "# B")],
            "one",
        );
        // The working copy moves on; the commit keeps what it recorded.
        std::fs::write(dir.path().join("proj/specifications/a.md"), "changed").unwrap();

        let files = list_files_at(&repo, first, "proj").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "proj/specifications/a.md");
        assert_eq!(read_blob(&repo, files[0].blob).unwrap(), b"# A\n\nalpha");

        let all = list_files_at(&repo, first, "").unwrap();
        assert_eq!(all.len(), 2);
        // A prefix that only shares leading characters is not the folder.
        assert!(list_files_at(&repo, first, "pro").unwrap().is_empty());
    }

    // GTC-FR-SHLL, GTC-FR-GUSU: unknown objects are typed errors.
    #[test]
    fn unknown_objects_are_object_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = crate::embeddings::test_support::init(dir.path());
        let missing = Oid::from_str("0123456789012345678901234567890123456789").unwrap();
        assert_eq!(list_files_at(&repo, missing, "").unwrap_err(), ERR_OBJECT_MISSING);
        assert_eq!(read_blob(&repo, missing).unwrap_err(), ERR_OBJECT_MISSING);
    }

    // GTC-FR-FWYK: a local branch is the tip; a missing name is a typed error.
    #[test]
    fn branch_tip_reads_the_local_branch_and_types_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let repo = crate::embeddings::test_support::init(dir.path());
        let first = commit_files(&repo, &[("a.md", "a")], "one");
        let head = repo.head().unwrap().shorthand().unwrap().to_string();
        assert_eq!(branch_tip(&repo, &head).unwrap(), first);
        assert_eq!(branch_tip(&repo, "no-such-branch").unwrap_err(), ERR_BRANCH_MISSING);
    }

    // GTC-FR-FWYK: with no local branch of the name, the remote-tracking branch
    // of the primary remote is the tip.
    #[test]
    fn branch_tip_falls_back_to_the_primary_remote() {
        let dir = tempfile::tempdir().unwrap();
        let repo = crate::embeddings::test_support::init(dir.path());
        let first = commit_files(&repo, &[("a.md", "a")], "one");
        repo.remote("origin", "https://example.invalid/r.git").unwrap();
        repo.reference("refs/remotes/origin/trunk", first, true, "test").unwrap();
        assert_eq!(branch_tip(&repo, "trunk").unwrap(), first);
    }
}
