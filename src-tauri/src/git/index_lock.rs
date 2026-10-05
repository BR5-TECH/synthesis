//! The repository index, held for the duration of a write (`GTC-git.md`
//! GTC-FR-19).
//!
//! Two writers inside this application build a commit against a worktree: the
//! draft-event commits of application-owned storage (`PST-project-storage.md`
//! PST-FR-DQZT), and the commit a graduation run makes onto its stream's branch
//! (`GRD-graduation.md` GRD-FR-ARLT). A third is outside it entirely: the author
//! running their own `git` command. This is the one thing that excludes all
//! three from each other, and it takes two conditions because neither check
//! catches the other.
//!
//! Git's own `index.lock` is what a concurrent `git` process holds while it
//! writes, so its presence is a refusal — and this cannot take that lock
//! itself, because libgit2's own `index.write` creates it. The second file is
//! this application's, and excludes its own second writer.
//!
//! The lock lives in the repository's **git directory**, which for a linked
//! worktree is `<primary>/.git/worktrees/<name>/`. That is per-worktree, which
//! is exactly right: so is the index.

use std::path::PathBuf;

use crate::fs as fsa;

/// The lock file this application creates. Named for the index rather than for
/// any one of its writers, because it excludes all of them.
const OWN_LOCK: &str = "synthesis-index.lock";

/// A held index. Released when it is dropped.
pub struct IndexLock {
    fs: fsa::FsAccess,
    path: PathBuf,
}

impl IndexLock {
    /// Take the lock, or report that something already holds it.
    ///
    /// `Ok(None)` is "somebody else is writing this index", which every caller
    /// answers in its own vocabulary: a publication refuses and is retried by
    /// the author, and a draft-event commit is dropped for that event.
    /// `Err` is a filesystem instance that could not be built for the git
    /// directory at all, which is a different thing and is not a busy index.
    pub fn try_acquire(repo: &git2::Repository) -> Result<Option<IndexLock>, String> {
        let git_dir = repo.path().to_path_buf();
        // GRB-FR-QIHE: the lock file sits outside the worktree root, so it is
        // held through an instance whose one allowed root is the git directory
        // rather than by a bare path call (FSA-FR-18).
        let fs = fsa::FsAccess::builder()
            .allow_root(&git_dir)
            .build()
            .map_err(|e| e.to_string())?;
        if git_dir.join("index.lock").exists() {
            return Ok(None);
        }
        let path = git_dir.join(OWN_LOCK);
        // `Exclusive` is the whole of what a lock needs: it creates the file or
        // reports that something already occupies the path, in one operation
        // the filesystem performs rather than two this would race between
        // (FSA-FR-05).
        match fs.create_file(&path, fsa::CreateMode::Exclusive) {
            Ok(()) => Ok(Some(IndexLock { fs, path })),
            Err(_) => Ok(None),
        }
    }

    /// Where this lock's file stands, for a caller that reports on it.
    #[cfg(test)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        let _ = self.fs.delete_path(&self.path, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_at(dir: &Path) -> git2::Repository {
        git2::Repository::init(dir).expect("the repository initialises")
    }

    #[test]
    fn one_holder_at_a_time_and_the_release_is_the_drop() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = repo_at(dir.path());

        let held = IndexLock::try_acquire(&repo)
            .expect("the instance builds")
            .expect("the lock is free");
        assert!(held.path().exists(), "the lock file is created");

        assert!(
            IndexLock::try_acquire(&repo)
                .expect("the instance builds")
                .is_none(),
            "a second holder is refused rather than blocked"
        );

        let path = held.path().to_path_buf();
        drop(held);
        assert!(!path.exists(), "the release is the drop");
        assert!(
            IndexLock::try_acquire(&repo)
                .expect("the instance builds")
                .is_some(),
            "the lock is free again"
        );
    }

    #[test]
    fn a_concurrent_git_process_is_excluded_by_gits_own_lock() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = repo_at(dir.path());
        std::fs::write(repo.path().join("index.lock"), b"").unwrap();

        assert!(
            IndexLock::try_acquire(&repo)
                .expect("the instance builds")
                .is_none(),
            "Git's own lock is a refusal, and this never takes it itself"
        );
    }
}
