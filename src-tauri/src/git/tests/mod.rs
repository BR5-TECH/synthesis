//! The test scenarios of `specifications/core/GTC-git.md`.
//!
//! Every scenario runs against a real repository in a temporary directory:
//! no Tauri runtime is started, and no network is reached. This module holds
//! the fixtures and the sinks the topic files share; each topic file holds
//! the tests of one operation.

use super::*;
// Named explicitly rather than reached through the parent module's own imports:
// the operations now live in submodules of `git`, so what a topic file can see
// through `use super::*` is what this module states here.
use crate::changes::{ERR_NOT_A_REPO, ERR_NO_MERGE_BASE, ERR_UNKNOWN_BRANCH};
use crate::github_tokens;
use crate::logging::Domain;
use crate::progress::ProgressSink;
use crate::worktree::{ERR_BRANCH_ALREADY_CHECKED_OUT, ERR_CHECKOUT_BLOCKED};
use git2::{BranchType, Oid, Repository};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

mod diff;
mod wire_shape;
mod file_revisions;
mod branches;
mod fetch;
mod commit_paths;
mod rollback;
mod upstream;
mod push;
mod log_records;
mod working_tree;
mod history;
mod branch_info;
mod branch_compare;
mod branch_deletion;
mod branch_deletion_remote;
mod pull_requests;
mod pull_request_create;
mod pull_request_timeline;


/// A `LogSink` that publishes nowhere: no Tauri runtime is running to
/// deliver an event, and the records themselves reach the buffer regardless.
#[derive(Clone, Copy, Default)]
struct NullSink;

impl crate::logging::LogSink for NullSink {
    fn publish(&self, _state: &crate::logging::BufferState) {}
}

/// Where the tests that are *not* about logging send their records.
///
/// Deliberately not `logging::BUFFER`: that one is process-wide and is
/// cleared when a project closes or a worktree changes (LGC-FR-15), which
/// `crate::project`'s and `crate::menu`'s tests do while these run. Nothing
/// reads this buffer; it exists so an operation under test has somewhere to
/// report to.
static SCRATCH_BUFFER: LogBuffer = LogBuffer::new();

/// A buffer of one test's own, leaked so it satisfies the `&'static` the
/// logging facility takes.
///
/// One per test rather than one for the module: no other test can append to
/// it or clear it, so every count asserted below is exact and — the part
/// that matters — every "this never appears in a record" guard is a real
/// guard rather than a query that happened to match nothing. The leak is a
/// few hundred bytes in a test binary.
fn own_buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

/// Every record in `buffer` whose message is exactly `message`.
fn records_for(buffer: &'static LogBuffer, message: &str) -> Vec<crate::logging::LogRecord> {
    all_records(buffer)
        .into_iter()
        .filter(|r| r.message == message)
        .collect()
}

/// The one record in `buffer` reading `message`, and a readable failure when
/// there is not exactly one.
fn record_for(buffer: &'static LogBuffer, message: &str) -> crate::logging::LogRecord {
    let mut found = records_for(buffer, message);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {message:?} record, got {found:?}"
    );
    found.remove(0)
}

fn all_records(buffer: &'static LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &crate::logging::LogFilter::default(),
            None,
            crate::logging::BUFFER_CAPACITY,
        )
        .expect("the buffer answers a default filter")
        .records
}

/// Every record in `buffer` as one string, so an assertion that something is
/// absent covers the records nobody thought to look at as well as the ones
/// they did.
fn buffer_text(buffer: &'static LogBuffer) -> String {
    serde_json::to_string(&all_records(buffer)).expect("records serialise")
}

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
        std::fs::write(self.dir.path().join(rel), contents).unwrap();
    }

    fn commit(&self, message: &str) -> Oid {
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
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .unwrap()
    }

    fn stage_all(&self) {
        let repo = self.repo();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.update_all(["*"].iter(), None).unwrap();
        index.write().unwrap();
    }

    fn current_branch(&self) -> String {
        self.repo().head().unwrap().shorthand().unwrap().to_string()
    }

    fn branch_and_checkout(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
        let reference = repo
            .find_branch(name, BranchType::Local)
            .unwrap()
            .into_reference();
        let target = reference.name().unwrap().to_string();
        repo.set_head(&target).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
    }
}

fn all_lines(payload: &DiffPayload) -> Vec<(String, String)> {
    payload
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| (l.kind.clone(), l.content.clone()))
        .collect()
}

/// A worktree fixture: the repository lives at `<container>/repo` so
/// siblings created for linked worktrees stay inside the container and are
/// removed with it.
struct WorktreeFixture {
    container: TempDir,
}

impl WorktreeFixture {
    fn new() -> WorktreeFixture {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let repo = Repository::init(&root).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        drop(config);
        drop(repo);
        let f = WorktreeFixture { container };
        f.write("a.md", "one\n");
        f.commit();
        f
    }

    fn root(&self) -> PathBuf {
        crate::changes::canonicalize_lenient(&self.container.path().join("repo"))
    }

    fn sibling(&self, name: &str) -> PathBuf {
        self.container.path().join(name)
    }

    /// Every file in the repository *and* its linked worktrees.
    ///
    /// Snapshotting a linked worktree's own directory would miss almost
    /// everything that matters: `<linked>/.git` is a one-line pointer file,
    /// and the index, `HEAD`, and all refs for that worktree live under
    /// `<primary>/.git/worktrees/<name>/` and `<primary>/.git/refs`. A
    /// checkout that wrongly moved HEAD or left a stray branch behind would
    /// be invisible to the narrower snapshot.
    fn snapshot_all(&self) -> std::collections::BTreeMap<String, String> {
        crate::changes::tests_support::snapshot(self.container.path().to_path_buf())
    }

    fn repo(&self) -> Repository {
        Repository::open(self.root()).unwrap()
    }

    fn write(&self, rel: &str, contents: &str) {
        std::fs::write(self.container.path().join("repo").join(rel), contents).unwrap();
    }

    fn commit(&self) {
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
        repo.commit(Some("HEAD"), &sig, &sig, "c", &tree, &refs)
            .unwrap();
    }

    fn current(&self) -> String {
        self.repo().head().unwrap().shorthand().unwrap().to_string()
    }

    fn branch(&self, name: &str) {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
    }

    /// A remote-tracking ref, plus the remote it belongs to so upstream
    /// wiring is exercisable. No network: the URL is never contacted.
    fn remote_ref(&self, name: &str) {
        let repo = self.repo();
        let remote = name.split_once('/').map(|(r, _)| r).unwrap_or("origin");
        if repo.find_remote(remote).is_err() {
            repo.remote(remote, "file:///nonexistent").unwrap();
        }
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reference(&format!("refs/remotes/{name}"), head.id(), true, "t")
            .unwrap();
    }
}

/// Collects what a fetch publishes, so the progress attribution of
/// GTC-FR-15 is observable without a Tauri runtime.
///
/// `Clone` and shared behind an `Arc` because a fetch's sink is now its log
/// sink too, and the logging facility clones one onto a background thread to
/// settle a coalesced flush (LGC-FR-17).
#[derive(Clone, Default)]
struct CollectingSink {
    published: Arc<Mutex<Vec<crate::progress::Operation>>>,
}

impl ProgressSink for CollectingSink {
    fn publish(&self, operation: &crate::progress::Operation) {
        self.published.lock().unwrap().push(operation.clone());
    }
}

impl crate::logging::LogSink for CollectingSink {
    fn publish(&self, _state: &crate::logging::BufferState) {}
}

/// A downstream repository whose `origin` is a real local repository, reached
/// over libgit2's `file://` transport.
///
/// A genuine fetch, deliberately: the prune of GTC-FR-13 and the refspec
/// mapping of GTC-FR-12 are libgit2's behaviour, and a mocked transfer would
/// assert what this code *asked for* rather than what a fetch actually does
/// to the refs. No network is involved, so the offline promise holds.
struct RemoteFixture {
    upstream: WorktreeFixture,
    downstream: WorktreeFixture,
}

impl RemoteFixture {
    fn new() -> RemoteFixture {
        let upstream = WorktreeFixture::new();
        let downstream = WorktreeFixture::new();
        let url = format!("file://{}", upstream.root().to_string_lossy());
        downstream.repo().remote("origin", &url).unwrap();
        RemoteFixture {
            upstream,
            downstream,
        }
    }

    fn root(&self) -> PathBuf {
        self.downstream.root()
    }

    /// The `file://` URL `origin` holds — unique to this fixture, and the
    /// string no log record may ever contain (GTC-FR-11).
    fn url(&self) -> String {
        format!("file://{}", self.upstream.root().to_string_lossy())
    }

    fn fetch(&self) -> Result<(), String> {
        self.fetch_with(&GlobalSettingsStore::in_memory(), "")
    }

    fn fetch_with(&self, store: &GlobalSettingsStore, project_key: &str) -> Result<(), String> {
        fetch_remote_branches(
            &CollectingSink::default(),
            &SCRATCH_BUFFER,
            &ProgressRegistry::default(),
            &self.root(),
            store,
            // Never consulted for a `file://` remote — only a `github.com`
            // HTTPS remote resolves a token (GTC-FR-09) — so the real
            // keychain-backed store is never asked for a secret here.
            &GithubTokens::default(),
            project_key,
        )
    }

    /// The downstream repository's remote-tracking branch names.
    fn tracking(&self) -> Vec<String> {
        branches_for(&self.root())
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == "remote")
            .map(|b| b.name)
            .collect()
    }
}

/// A `ProgressSink` that records what it was handed, so an attributed
/// operation's lifecycle is inspectable without a Tauri runtime.
#[derive(Clone, Default)]
struct RecordingSink {
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl ProgressSink for RecordingSink {
    fn publish(&self, operation: &crate::progress::Operation) {
        self.seen
            .lock()
            .unwrap()
            .push((operation.kind.clone(), format!("{:?}", operation.state)));
    }
}

impl crate::logging::LogSink for RecordingSink {
    fn publish(&self, _state: &crate::logging::BufferState) {}
}
