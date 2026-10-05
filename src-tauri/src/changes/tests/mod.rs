//! The test scenarios of `specifications/core/CHC-changes-command.md`.
//!
//! The commands compose libgit2 against a real repository, so every test builds
//! one in a temporary directory. This module holds the repository fixture and
//! the lookup helpers; the topic modules hold the tests.

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

// -----------------------------------------------------------------------
// Repository fixtures
//
// The commands compose libgit2 against a real repository, so the behaviour
// worth testing (merge-base semantics, rename detection, ignore handling,
// per-file line counts) only exists against one. `git2` builds the
// repositories; commits go through its object API so no `git` binary and no
// user configuration are involved.
// -----------------------------------------------------------------------

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        drop(config);
        drop(repo);
        Fixture { dir }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn repo(&self) -> Repository {
        Repository::open(self.dir.path()).unwrap()
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn write_bytes(&self, rel: &str, contents: &[u8]) {
        let path = self.dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn remove(&self, rel: &str) {
        std::fs::remove_file(self.dir.path().join(rel)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        let dest = self.dir.path().join(to);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::rename(self.dir.path().join(from), dest).unwrap();
    }

    /// Stage every path under the working tree (including deletions).
    fn stage_all(&self) {
        let repo = self.repo();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.update_all(["*"].iter(), None).unwrap();
        index.write().unwrap();
    }

    /// Stage everything and commit it on the current branch.
    fn commit(&self, message: &str) -> Oid {
        self.stage_all();
        let repo = self.repo();
        let mut index = repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
            Some(c) => vec![c],
            None => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .unwrap()
    }

    fn branch(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
    }

    fn checkout(&self, name: &str) {
        let repo = self.repo();
        let reference = repo
            .find_branch(name, BranchType::Local)
            .unwrap()
            .into_reference();
        let target = reference.name().unwrap().to_string();
        repo.set_head(&target).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
    }

    fn detach(&self) {
        let repo = self.repo();
        let oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.set_head_detached(oid).unwrap();
    }

    /// The branch the fixture is on. `git init` picks the initial branch
    /// name from the ambient Git configuration, so tests that need to name
    /// it read it rather than assuming `main` or `master`.
    fn current_branch(&self) -> String {
        let repo = self.repo();
        let head = repo.head().unwrap();
        let name = head.shorthand().unwrap().to_string();
        name
    }
}

fn entry<'a>(set: &'a ChangeSet, path: &str) -> &'a ChangeEntry {
    set.entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("expected an entry for {path:?}; got {:?}", paths(set)))
}

fn paths(set: &ChangeSet) -> Vec<String> {
    set.entries.iter().map(|e| e.path.clone()).collect()
}

mod change_set;
mod renames_and_ignores;
mod head_states;
mod classification;
mod branches;
mod read_only;
mod diff_totals;
mod co_located;
mod watch_surface;
mod wire_shapes;
