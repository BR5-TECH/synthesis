//! A real superproject with real submodules, for the tests of
//! `GSM-git-submodules.md` and of every module that composes it.
//!
//! Each submodule is cloned by libgit2 from an origin repository in the same
//! temporary directory, so its `.git` entry is a file that names
//! `.git/modules/<name>` of the parent, as `git submodule add` makes it.

use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use tempfile::TempDir;

/// Make a repository on `main` with a test identity.
pub fn init_repo(path: &Path) -> Repository {
    std::fs::create_dir_all(path).unwrap();
    let mut opts = git2::RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(path, &opts).unwrap();
    set_identity(&repo);
    repo
}

fn set_identity(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
}

/// Write a file below the working directory of `repo`.
pub fn write(repo: &Repository, rel: &str, body: &str) {
    let path = repo.workdir().unwrap().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Commit what the index of `repo` holds.
pub fn commit_index(repo: &Repository, message: &str) -> Oid {
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parents: Vec<git2::Commit> =
        repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs).unwrap()
}

/// Stage every file of `repo` (submodule folders as their pointers) and commit.
pub fn commit_all(repo: &Repository, message: &str) -> Oid {
    let mut index = repo.index().unwrap();
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
    index.update_all(["*"].iter(), None).unwrap();
    index.write().unwrap();
    commit_index(repo, message)
}

/// The main repository and the origins of its submodules.
pub struct Superproject {
    pub tmp: TempDir,
    /// How many origins this fixture made, so each origin folder is new.
    origins: std::cell::Cell<usize>,
}

impl Superproject {
    /// A main repository at `<tmp>/main` with `files`, committed.
    pub fn new(files: &[(&str, &str)]) -> Superproject {
        let tmp = TempDir::new().unwrap();
        let repo = init_repo(&tmp.path().join("main"));
        for (rel, body) in files {
            write(&repo, rel, body);
        }
        commit_all(&repo, "init");
        Superproject { tmp, origins: std::cell::Cell::new(0) }
    }

    pub fn root(&self) -> PathBuf {
        self.tmp.path().join("main")
    }

    pub fn main(&self) -> Repository {
        Repository::open(self.root()).unwrap()
    }

    /// Open the submodule at the project path `rel`.
    pub fn sub(&self, rel: &str) -> Repository {
        Repository::open(self.root().join(rel)).unwrap()
    }

    /// Add a submodule at `rel` of the repository at the project path `parent`
    /// (`""` for the main repository), from a new origin that holds `files`,
    /// and commit the addition in that parent. A parent that is a submodule of
    /// the main repository then has a new pointer, which the main repository
    /// records too, so the set starts with no change.
    pub fn add_submodule(&self, parent: &str, rel: &str, files: &[(&str, &str)]) -> Repository {
        let n = self.origins.get();
        self.origins.set(n + 1);
        let origin_dir = self.tmp.path().join(format!("o{n}"));
        let origin = init_repo(&origin_dir);
        for (file, body) in files {
            write(&origin, file, body);
        }
        commit_all(&origin, "origin init");
        let parent_repo = if parent.is_empty() { self.main() } else { self.sub(parent) };
        let url = origin_dir.to_string_lossy().into_owned();
        let mut submodule = parent_repo.submodule(&url, Path::new(rel), true).unwrap();
        submodule.clone(None).unwrap();
        submodule.add_finalize().unwrap();
        commit_index(&parent_repo, "add submodule");
        if !parent.is_empty() {
            commit_all(&self.main(), "record the nested submodule");
        }
        let repo = submodule.open().unwrap();
        set_identity(&repo);
        repo
    }

    /// A new bare repository that becomes the `origin` of `repo`, with the
    /// current branch pushed to it and tracking it.
    pub fn bare_remote(&self, repo: &Repository) -> PathBuf {
        let n = self.origins.get();
        self.origins.set(n + 1);
        let dir = self.tmp.path().join(format!("b{n}"));
        Repository::init_bare(&dir).unwrap();
        let url = dir.to_string_lossy().into_owned();
        if repo.find_remote("origin").is_ok() {
            repo.remote_set_url("origin", &url).unwrap();
        } else {
            repo.remote("origin", &url).unwrap();
        }
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let mut remote = repo.find_remote("origin").unwrap();
        remote.push(&[format!("refs/heads/{branch}:refs/heads/{branch}").as_str()], None).unwrap();
        let mut local = repo.find_branch(&branch, git2::BranchType::Local).unwrap();
        local.set_upstream(Some(&format!("origin/{branch}"))).unwrap();
        dir
    }
}
