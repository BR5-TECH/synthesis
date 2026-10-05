//! The unit tests of `specifications/core/WTC-worktree-context.md`.
//!
//! This module holds the repository fixture and the helpers that every topic
//! file uses. Each topic file below covers one part of the module.

use super::*;
use crate::changes::ERR_NOT_A_REPO;
use std::fs;
use tempfile::TempDir;

/// A repository with one commit on its initial branch, plus helpers for
/// branching and adding linked worktrees.
///
/// The repository lives at `<container>/repo` rather than at the temporary
/// directory itself, so `sibling()` lands inside the container: linked
/// worktrees are isolated per test (tests run concurrently, and a name like
/// `wt-alpha` directly under the system temp directory would collide) and
/// are removed with it.
struct Fixture {
    container: TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        let repo = Repository::init(&root).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        drop(config);
        drop(repo);
        let f = Fixture { container };
        f.write("a.md", "one\n");
        f.commit("initial");
        f
    }

    fn dir(&self) -> PathBuf {
        self.container.path().join("repo")
    }

    fn root(&self) -> PathBuf {
        canonicalize_lenient(&self.dir())
    }

    /// A sibling directory of the repository, for linked worktrees.
    fn sibling(&self, name: &str) -> PathBuf {
        self.root().parent().unwrap().join(name)
    }

    fn repo(&self) -> Repository {
        Repository::open(self.dir()).unwrap()
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.dir().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn commit(&self, message: &str) {
        let repo = self.repo();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.update_all(["*"].iter(), None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parents: Vec<git2::Commit> =
            match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
                Some(c) => vec![c],
                None => vec![],
            };
        let refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs)
            .unwrap();
    }

    fn current_branch(&self) -> String {
        self.repo().head().unwrap().shorthand().unwrap().to_string()
    }

    fn branch(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
    }

    /// Add a linked worktree for `branch` at a sibling directory.
    fn add_worktree(&self, dir_name: &str, branch: &str) -> PathBuf {
        let path = self.sibling(dir_name);
        create_worktree_at(&self.root(), branch, &path.to_string_lossy()).unwrap()
    }

    /// A remote-tracking ref, faked by writing the ref directly — no
    /// network, exactly as the non-functional requirement demands.
    fn remote_ref(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reference(
            &format!("refs/remotes/{name}"),
            head.id(),
            true,
            "test remote",
        )
        .unwrap();
    }
}

fn names(entries: &[WorktreeEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

fn branch_names(entries: &[BranchEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

/// A headless app carrying the managed state the switching commands touch.
fn mock_app_on(root: &Path, anchor: &Path) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    app.manage(ProjectState::default());
    app.manage(ProjectWatcher::default());
    app.manage(crate::artifacts::ContentTracker::default());
    app.manage(crate::draft_watcher::DraftsWatcher::default());
    // PRG-FR-13: a reroot terminates the operations scoped to the outgoing
    // content root, so the registry is part of the app's state.
    app.manage(crate::progress::ProgressRegistry::default());
    // ASC-FR-16 / SCC-FR-04: a reroot tears the outgoing root's candidate
    // list down and stops any search still sweeping it, so both stores are
    // part of the app's state too.
    app.manage(crate::scanning::CandidateStore::default());
    // ASC-FR-16: and the attribution baseline, which belongs to the outgoing
    // content root for the same reason and is dropped with it.
    app.manage(crate::scanning::AttributionBaseline::default());
    app.manage(crate::search::SearchRegistry::default());
    // FSA-FR-21: a reroot replaces the shared filesystem instance, so the
    // state it is installed into is part of the app's state too.
    app.manage(crate::fs::FsAccessState::default());
    let project = app.state::<ProjectState>();
    project.set_root(root.to_path_buf());
    project.set_anchor(to_string_path(anchor));
    app
}

/// Stand in for `GTC`'s fetch by doing to the refs what a real one would.
/// The point of a refresh is that the *re-enumeration* picks up whatever the
/// fetch wrote, so the transfer is the one part worth faking.
fn fake_fetch(f: &Fixture, appeared: &[&str], disappeared: &[&str]) {
    for name in appeared {
        f.remote_ref(name);
    }
    let repo = f.repo();
    for name in disappeared {
        repo.find_reference(&format!("refs/remotes/{name}"))
            .expect("precondition: the stale ref exists")
            .delete()
            .unwrap();
    }
}

/// A diagnostic buffer of one test's own, leaked so it satisfies the
/// `&'static` the logging facility takes.
///
/// Not `logging::BUFFER`: that one is process-wide and is *cleared* by other
/// modules' tests (LGC-FR-15), which is precisely the event under test here
/// — an assertion against it would be a coin toss. Same reasoning, and same
/// remedy, as `git.rs`'s `own_buffer`.
fn own_buffer() -> &'static crate::logging::LogBuffer {
    Box::leak(Box::new(crate::logging::LogBuffer::new()))
}

fn messages_in(buffer: &'static crate::logging::LogBuffer) -> Vec<String> {
    buffer
        .query(
            &crate::logging::LogFilter::default(),
            None,
            crate::logging::BUFFER_CAPACITY,
        )
        .expect("the buffer answers a default filter")
        .records
        .into_iter()
        .map(|r| r.message)
        .collect()
}

fn mark(app: &tauri::AppHandle<tauri::test::MockRuntime>, buffer: &'static crate::logging::LogBuffer) {
    crate::logging::log(
        app,
        buffer,
        crate::logging::LogLevel::Info,
        &[crate::logging::Domain::Backend],
        "from before the switch",
        crate::logging::Fields::new(),
    );
}

mod activation;
mod co_located;
mod creation;
mod diagnostics;
mod enumeration;
mod identity;
mod refresh;
mod removal;
mod rerooting;
mod wire_shapes;
