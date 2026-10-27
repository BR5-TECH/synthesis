//! Service-level tests: the switches, readiness, the job queue, the triggers,
//! and the failures, on a mock app with the fake embedder. Jobs run
//! synchronously through `run_until_idle` at an explicit time, so no test
//! waits for real time. A test acts during a running job through the fake's
//! `on_embed` hook, never through a sleep.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use git2::Repository;
use tauri::Manager;

use super::layers::test_support::{commit, init};
use super::model::fake::{FakeFactory, Log};
use super::service::{JobKind, ModelState, Phase};
use super::*;
use crate::bm25_index::{Bm25Indexer, PassScope};

mod documents;
mod failures;
mod git_moves;
mod jobs;
mod search;
mod switches;

const SPEC_A: &str =
    "# Worktrees\n\nHow a worktree is torn down.\n\n# Branches\n\nHow a branch is created.";
const SPEC_B: &str = "# Search\n\nThe search bar finds files by their text.";

/// Past the 2-second debounce, whatever the clock read when it was set.
const LATER: Duration = Duration::from_secs(3600);

struct Harness {
    app: tauri::App<tauri::test::MockRuntime>,
    repo_dir: tempfile::TempDir,
    store_dir: tempfile::TempDir,
    _settings_dir: tempfile::TempDir,
    repo: Repository,
    log: Arc<Log>,
    rooted: crate::fs::RootFs,
    store_root: crate::fs::RootFs,
}

impl Harness {
    fn new() -> Harness {
        Harness::with_files(&[("specifications/a.md", SPEC_A), ("specifications/b.md", SPEC_B)])
    }

    fn with_files(files: &[(&str, &str)]) -> Harness {
        let (factory, log) = FakeFactory::new();
        Harness::with_factory(files, factory, log)
    }

    fn with_factory(files: &[(&str, &str)], factory: FakeFactory, log: Arc<Log>) -> Harness {
        let repo_dir = tempfile::tempdir().unwrap();
        let repo = init(repo_dir.path());
        commit(&repo, files);
        let store_dir = tempfile::tempdir().unwrap();
        let settings_dir = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_app();
        app.manage(Bm25Indexer::default());
        app.manage(crate::progress::ProgressRegistry::default());
        app.manage(crate::scanning::CandidateStore::default());
        app.manage(crate::scanning::AttributionBaseline::default());
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::global_settings::GlobalSettingsStore::with_path(
            settings_dir.path().join("synthesis.toml"),
        ));
        app.manage(EmbeddingsService::new(Box::new(factory), false));
        let rooted = crate::fs::RootFs::for_root(repo_dir.path());
        let store_root = crate::fs::RootFs::for_root(store_dir.path());
        Harness {
            app,
            repo_dir,
            store_dir,
            _settings_dir: settings_dir,
            repo,
            log,
            rooted,
            store_root,
        }
    }

    fn service(&self) -> tauri::State<'_, EmbeddingsService> {
        self.app.state::<EmbeddingsService>()
    }

    fn handle(&self) -> &tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle()
    }

    fn enable(&self) {
        self.service().apply_settings(EmbeddingsRecord { enabled: true, model: None });
    }

    /// Mount the content root on both indexes, as `remount_content_root` does:
    /// the embeddings index after the BM25 indexer, before any pass.
    fn mount(&self, mode: ProjectEmbeddingsMode) {
        self.mount_with_store(mode, Some(self.store_root.clone()));
    }

    fn mount_with_store(&self, mode: ProjectEmbeddingsMode, store_dir: Option<crate::fs::RootFs>) {
        let indexer = self.app.state::<Bm25Indexer>();
        indexer.mount(&self.rooted);
        self.service().mount(MountInput {
            key: "project".to_string(),
            git: true,
            root: self.repo_dir.path().to_path_buf(),
            store_dir,
            mode,
            generation: indexer.generation(),
        });
    }

    /// Run one BM25 pass; its publish reaches the service.
    fn pass(&self) {
        pass_on(self.handle(), &self.rooted);
    }

    fn settle(&self, now: Instant) {
        self.service().run_until_idle(self.handle(), now);
    }

    /// Enable, mount, index, and settle: a project whose index is ready.
    fn ready(&self) {
        self.enable();
        self.mount(ProjectEmbeddingsMode::UseGlobal);
        self.pass();
        self.settle(Instant::now());
        assert_eq!(self.phase(), Some(Phase::Ready));
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.repo_dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn phase(&self) -> Option<Phase> {
        self.service().status().project.map(|p| p.state)
    }

    fn embedded(&self) -> usize {
        self.log.embedded.load(Ordering::SeqCst)
    }

    fn texts(&self) -> Vec<String> {
        self.log.texts.lock().unwrap().clone()
    }

    fn jobs(&self) -> Vec<JobKind> {
        self.service().jobs_run.lock().unwrap().clone()
    }

    fn ready_for_spec(&self) -> bool {
        self.service().ready_view(crate::bm25_index::IndexId::Spec).is_some()
    }

    fn store_file(&self) -> std::path::PathBuf {
        self.store_dir.path().join(super::store::STORE_FILE)
    }

    /// Run `hook` inside the next embedding call of a job.
    fn during_next_embed(&self, hook: impl FnOnce() + Send + 'static) {
        *self.log.on_embed.lock().unwrap() = Some(Box::new(hook));
    }
}

/// One BM25 pass of `root`, with the candidate list invalidated first — as the
/// watcher does on every change, since a test writes files directly.
fn pass_on(app: &tauri::AppHandle<tauri::test::MockRuntime>, root: &crate::fs::RootFs) {
    app.state::<crate::scanning::CandidateStore>().invalidate();
    let indexer = app.state::<Bm25Indexer>();
    let generation = indexer.generation();
    indexer.run_pass(app, root, PassScope::ALL, generation);
}

/// Every record of the shared log buffer, rendered, for a leak scan by a
/// sentinel unique to the test.
fn rendered_log() -> String {
    use crate::logging::{LogFilter, LogLevel, BUFFER};
    serde_json::to_string(
        &BUFFER
            .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 100_000)
            .unwrap()
            .records,
    )
    .unwrap()
}
