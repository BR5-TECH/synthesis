//! The embeddings service: switches, readiness, the job queue, and the status
//! (EMB-FR-WQGF, EMB-FR-BLHF, EMB-FR-SWRV, EMB-FR-WWMW, EMB-FR-ZFGT).
//!
//! One worker thread runs every job, one at a time. The triggers only record
//! what is due and wake it, so no trigger and no command waits for a job.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::engine::{Outcome, ReindexScope};
use super::layers::GitFacts;
use super::model::{DownloadProgress, Embedder, EmbedderFactory};
use super::settings::{embeddings_work, EmbeddingsRecord, ProjectEmbeddingsMode};
use super::store::View;

/// EMB-FR-WWMW: changes that arrive in this window after the first change of a
/// burst collapse into one incremental pass.
pub const DEBOUNCE: Duration = Duration::from_secs(2);

/// EMB-FR-ZFGT: the shortest interval between two progress-only status events.
pub(super) const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(250);

/// The state of the selected model (EMB contract surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    NotDownloaded,
    Downloading,
    Loading,
    Ready,
    Failed,
}

/// The state of the open project's vector index (EMB contract surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Off,
    Preparing,
    Ready,
    Updating,
    Unavailable,
}

/// The jobs of EMB-FR-BLHF, and the incremental pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    FirstIndexing,
    OpenCheck,
    BranchUpdate,
    RootUpdate,
    ModelSwitch,
    StaleRebuild,
    Reindex,
    Incremental,
}

impl JobKind {
    /// EMB-FR-WJCW: the operation label of the job.
    pub fn label(self) -> &'static str {
        match self {
            JobKind::FirstIndexing => "Embedding specifications…",
            JobKind::OpenCheck | JobKind::BranchUpdate | JobKind::RootUpdate => {
                "Checking embeddings…"
            }
            JobKind::ModelSwitch | JobKind::StaleRebuild => "Switching embedding model…",
            JobKind::Reindex => "Reindexing embeddings…",
            JobKind::Incremental => "Updating embeddings…",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub kind: JobKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub completed_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
}

/// The project part of the status (EMB contract surface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub mode: ProjectEmbeddingsMode,
    pub working: bool,
    pub state: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<JobStatus>,
    pub git: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_tip: Option<String>,
    pub spec_chunks: usize,
    pub document_chunks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// EMB-FR-ZFGT: `get_embeddings_status` and `"embeddings status changed"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingsStatus {
    pub enabled: bool,
    pub model: String,
    pub model_state: ModelState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<DownloadStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectStatus>,
}

/// What is due for the open project.
#[derive(Debug, Default)]
pub(super) struct Pending {
    pub full: Option<JobKind>,
    pub incremental_at: Option<Instant>,
    pub root_check: bool,
    pub reindex: Option<ReindexScope>,
}

pub(super) struct Project {
    /// Bumped on every mount, so a job of an outgoing root discards its result.
    pub serial: u64,
    pub key: String,
    pub root: std::path::PathBuf,
    pub store_dir: Option<crate::fs::RootFs>,
    pub mode: ProjectEmbeddingsMode,
    pub git: bool,
    /// The BM25 generation of this mount, and whether it has published.
    pub generation: u64,
    pub snapshot_ready: bool,
    pub pending: Pending,
    pub phase: Phase,
    pub job: Option<JobStatus>,
    pub facts: Option<GitFacts>,
    pub outcome: Option<Outcome>,
    pub last_error: Option<String>,
}

pub(super) struct ModelSlot {
    pub state: ModelState,
    /// The id of the model that is loaded now.
    pub loaded: Option<String>,
    pub download: Option<DownloadProgress>,
    pub error: Option<String>,
}

pub(super) struct State {
    pub settings: EmbeddingsRecord,
    pub model: ModelSlot,
    pub project: Option<Project>,
    pub next_serial: u64,
    pub last_progress_emit: Option<Instant>,
}

/// What a mount hands the service.
pub struct MountInput {
    pub key: String,
    /// EMB-FR-MGML: whether the content root is inside a Git repository.
    pub git: bool,
    pub root: std::path::PathBuf,
    pub store_dir: Option<crate::fs::RootFs>,
    pub mode: ProjectEmbeddingsMode,
    pub generation: u64,
}

/// The managed state of the embeddings index.
pub struct EmbeddingsService {
    pub(super) state: Mutex<State>,
    pub(super) wake: Condvar,
    /// EMB-FR-SWRV: a job stops when this moves past the value it started at.
    pub(super) epoch: AtomicU64,
    pub(super) embedder: Mutex<Option<Box<dyn Embedder>>>,
    /// Bumped each time a model is put in `embedder`, so a drop asked for
    /// before that never removes the newer model.
    pub(super) embedder_serial: AtomicU64,
    /// EMB-FR-JLRR: queries waiting for the model, which batches give way to.
    pub(super) queries: Mutex<usize>,
    pub(super) queries_done: Condvar,
    pub(super) factory: Box<dyn EmbedderFactory>,
    /// Whether triggers start the worker thread. Tests run jobs themselves.
    pub(super) spawn_worker: bool,
    pub(super) worker_started: Mutex<bool>,
    /// Every job that started, for the tests that count passes.
    #[cfg(test)]
    pub(super) jobs_run: Mutex<Vec<JobKind>>,
}

impl EmbeddingsService {
    pub fn new(factory: Box<dyn EmbedderFactory>, spawn_worker: bool) -> EmbeddingsService {
        EmbeddingsService {
            state: Mutex::new(State {
                settings: EmbeddingsRecord::default(),
                model: ModelSlot {
                    state: ModelState::NotDownloaded,
                    loaded: None,
                    download: None,
                    error: None,
                },
                project: None,
                next_serial: 1,
                last_progress_emit: None,
            }),
            wake: Condvar::new(),
            epoch: AtomicU64::new(0),
            embedder: Mutex::new(None),
            embedder_serial: AtomicU64::new(0),
            queries: Mutex::new(0),
            queries_done: Condvar::new(),
            factory,
            spawn_worker,
            worker_started: Mutex::new(false),
            #[cfg(test)]
            jobs_run: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// EMB-FR-SWRV: stop the running job.
    pub(super) fn cancel_running(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// The settings the service works with.
    pub fn settings(&self) -> EmbeddingsRecord {
        self.lock().settings.clone()
    }

    /// EMB-FR-JFOV, EMB-FR-TGCL: take new Global settings. Turning the switch
    /// off stops every job at once; a new model while the switch is on starts
    /// the model switch.
    pub fn apply_settings(&self, record: EmbeddingsRecord) {
        let mut st = self.lock();
        let was_enabled = st.settings.enabled;
        let model_changed = st.settings.model_id() != record.model_id();
        let mut drop_model = false;
        let serial = self.embedder_serial.load(Ordering::SeqCst);
        st.settings = record;
        if !st.settings.enabled {
            self.cancel_running();
            st.model.state = ModelState::NotDownloaded;
            st.model.loaded = None;
            st.model.download = None;
            st.model.error = None;
            drop_model = true;
            if let Some(project) = st.project.as_mut() {
                project.phase = Phase::Off;
                project.job = None;
                project.pending = Pending::default();
            }
        } else if model_changed || !was_enabled {
            if model_changed {
                self.cancel_running();
                st.model.loaded = None;
                st.model.error = None;
                drop_model = true;
                // The model switch embeds everything again, which takes in
                // every change a pending incremental pass was waiting for.
                if let Some(project) = st.project.as_mut() {
                    project.pending.incremental_at = None;
                }
            }
            if st.model.state == ModelState::Failed {
                st.model.state = ModelState::NotDownloaded;
                st.model.error = None;
            }
            let enabled = st.settings.enabled;
            if let Some(project) = st.project.as_mut() {
                if embeddings_work(enabled, project.mode) {
                    project.phase = Phase::Preparing;
                    project.pending.full =
                        Some(if was_enabled { JobKind::ModelSwitch } else { JobKind::OpenCheck });
                }
            }
        }
        drop(st);
        // Released after the state lock: a running batch holds the model, and
        // nothing else may wait behind it on the state.
        if drop_model {
            self.drop_embedder(serial);
        }
        self.wake.notify_all();
    }

    /// Remove the model that was loaded when `serial` was read, and not a model
    /// loaded since.
    fn drop_embedder(&self, serial: u64) {
        let mut slot = match self.embedder.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        if self.embedder_serial.load(Ordering::SeqCst) == serial {
            *slot = None;
        }
    }

    /// EMB-FR-KEGS: try a failed download again.
    pub fn retry_download(&self) {
        let mut st = self.lock();
        if st.model.state == ModelState::Failed {
            st.model.state = ModelState::NotDownloaded;
            st.model.error = None;
        }
        drop(st);
        self.wake.notify_all();
    }

    /// EMB-FR-CTXC: a new mode for the open project.
    pub fn apply_mode(&self, mode: ProjectEmbeddingsMode) {
        let mut st = self.lock();
        let enabled = st.settings.enabled;
        if let Some(project) = st.project.as_mut() {
            let was = embeddings_work(enabled, project.mode);
            project.mode = mode;
            let now = embeddings_work(enabled, mode);
            if was && !now {
                self.cancel_running();
                project.phase = Phase::Off;
                project.job = None;
                project.pending = Pending::default();
            } else if !was && now {
                project.phase = Phase::Preparing;
                project.pending.full = Some(JobKind::OpenCheck);
            }
        }
        drop(st);
        self.wake.notify_all();
    }

    /// EMB-FR-HRYV, EMB-FR-QNWF: queue a Reindex. Refused when embeddings do
    /// not work for the open project.
    pub fn request_reindex(&self, scope: ReindexScope) -> Result<(), String> {
        let mut st = self.lock();
        let enabled = st.settings.enabled;
        let project = st.project.as_mut().ok_or_else(|| super::ERR_NO_PROJECT_OPEN.to_string())?;
        if !embeddings_work(enabled, project.mode) {
            return Err(super::ERR_EMBEDDINGS_NOT_WORKING.to_string());
        }
        self.cancel_running();
        project.pending.reindex = Some(scope);
        project.pending.full = None;
        project.phase = Phase::Preparing;
        drop(st);
        self.wake.notify_all();
        Ok(())
    }

    /// EMB-FR-JIJH, EMB-FR-NHQG: a content root was mounted.
    pub fn mount(&self, input: MountInput) {
        self.cancel_running();
        let mut st = self.lock();
        let same_project = st.project.as_ref().is_some_and(|p| p.key == input.key);
        let works = embeddings_work(st.settings.enabled, input.mode);
        let serial = st.next_serial;
        st.next_serial += 1;
        st.project = Some(Project {
            serial,
            key: input.key,
            root: input.root,
            store_dir: input.store_dir,
            mode: input.mode,
            git: input.git,
            generation: input.generation,
            snapshot_ready: false,
            pending: Pending {
                full: works.then_some(if same_project {
                    JobKind::BranchUpdate
                } else {
                    JobKind::OpenCheck
                }),
                root_check: false,
                incremental_at: None,
                reindex: None,
            },
            phase: if works { Phase::Preparing } else { Phase::Off },
            job: None,
            facts: None,
            outcome: None,
            last_error: None,
        });
        drop(st);
        self.wake.notify_all();
    }

    /// EMB-FR-RMZH: the project closed.
    pub fn unmount(&self) {
        self.cancel_running();
        self.lock().project = None;
        self.wake.notify_all();
    }

    /// EMB-FR-HWRQ, EMB-FR-IBXK, EMB-FR-WWMW: the BM25 index published a pass.
    /// The first publish of a mount lets the waiting full job run; a later one
    /// schedules an incremental pass when it touched specifications or
    /// documents.
    pub fn index_published(&self, touched: bool, generation: u64, now: Instant) {
        let mut st = self.lock();
        let enabled = st.settings.enabled;
        let Some(project) = st.project.as_mut() else {
            return;
        };
        if project.generation != generation {
            return;
        }
        if !project.snapshot_ready {
            project.snapshot_ready = true;
        } else if touched && embeddings_work(enabled, project.mode) {
            match project.phase {
                Phase::Ready | Phase::Updating => {
                    project.pending.incremental_at.get_or_insert(now + DEBOUNCE);
                }
                // EMB-FR-EAEA: the next trigger after a failure tries again.
                Phase::Unavailable => {
                    project.pending.full.get_or_insert(JobKind::OpenCheck);
                }
                // EMB-FR-BZZT: a full job reads the snapshot at its start, so a
                // change during it is taken in by a pass after it ends.
                Phase::Preparing => {
                    project.pending.incremental_at.get_or_insert(now + DEBOUNCE);
                }
                Phase::Off => {}
            }
        }
        drop(st);
        self.wake.notify_all();
    }

    /// EMB-FR-VQSK: check whether the root tip moved.
    pub fn request_root_check(&self) {
        let mut st = self.lock();
        let enabled = st.settings.enabled;
        if let Some(project) = st.project.as_mut() {
            if embeddings_work(enabled, project.mode) {
                project.pending.root_check = true;
            }
        }
        drop(st);
        self.wake.notify_all();
    }

    /// EMB-FR-ZFGT: the status now.
    pub fn status(&self) -> EmbeddingsStatus {
        let st = self.lock();
        status_of(&st)
    }

    /// EMB-FR-BLHF: the store and the view a `spec` or `documents` search
    /// reads, or `None` while vector search is not ready.
    pub(super) fn ready_view(
        &self,
        index: crate::bm25_index::IndexId,
    ) -> Option<(std::path::PathBuf, View)> {
        let st = self.lock();
        let project = st.project.as_ref()?;
        let ready = st.settings.enabled
            && st.model.state == ModelState::Ready
            && st.model.loaded.as_deref() == Some(st.settings.model_id().as_str())
            && embeddings_work(st.settings.enabled, project.mode)
            && matches!(project.phase, Phase::Ready | Phase::Updating);
        if !ready {
            return None;
        }
        let outcome = project.outcome.as_ref()?;
        let view = match index {
            crate::bm25_index::IndexId::Spec => outcome.spec_view,
            crate::bm25_index::IndexId::Documents => {
                View { root: outcome.documents_layer, branch: None }
            }
            _ => return None,
        };
        let dir = project.store_dir.as_ref()?;
        Some((super::store::store_path(dir.path()), view))
    }

    /// EMB-FR-JLRR: embed a query ahead of the batches of a job.
    pub(super) fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        *self.queries.lock().map_err(|_| "query gate poisoned")? += 1;
        // Every way out of this call leaves the gate, or a job would wait for
        // a query that is gone.
        struct Leave<'a>(&'a EmbeddingsService);
        impl Drop for Leave<'_> {
            fn drop(&mut self) {
                if let Ok(mut waiting) = self.0.queries.lock() {
                    *waiting = waiting.saturating_sub(1);
                }
                self.0.queries_done.notify_all();
            }
        }
        let _leave = Leave(self);
        let mut slot = self.embedder.lock().map_err(|_| "model poisoned")?;
        let vectors = match slot.as_mut() {
            Some(model) => model.embed(&[query.to_string()])?,
            None => return Err("no model is loaded".to_string()),
        };
        vectors.into_iter().next().ok_or_else(|| "the model returned no vector".to_string())
    }

    /// EMB-FR-JLRR: embed one batch of a job, after every waiting query.
    pub(super) fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        {
            let mut waiting = self.queries.lock().map_err(|_| "query gate poisoned")?;
            while *waiting > 0 {
                waiting = self.queries_done.wait(waiting).map_err(|_| "query gate poisoned")?;
            }
        }
        let mut slot = self.embedder.lock().map_err(|_| "model poisoned")?;
        match slot.as_mut() {
            Some(model) => model.embed(texts),
            None => Err("no model is loaded".to_string()),
        }
    }
}

pub(super) fn status_of(st: &State) -> EmbeddingsStatus {
    let enabled = st.settings.enabled;
    EmbeddingsStatus {
        enabled,
        model: st.settings.model_id(),
        model_state: st.model.state,
        download: st
            .model
            .download
            .filter(|_| st.model.state == ModelState::Downloading)
            .map(|d| DownloadStatus { completed_bytes: d.completed, total_bytes: d.total }),
        model_error: st.model.error.clone(),
        project: st.project.as_ref().map(|project| {
            let facts = project.facts.as_ref();
            let outcome = project.outcome.as_ref();
            let (git, branch, detached, root_branch) = match facts {
                Some(GitFacts::Repository { root_branch, head, .. }) => (
                    "repository",
                    match head {
                        super::layers::Head::Branch(name) => Some(name.clone()),
                        super::layers::Head::Detached => None,
                    },
                    matches!(head, super::layers::Head::Detached),
                    root_branch.clone(),
                ),
                Some(GitFacts::NoRepository) => ("none", None, false, None),
                None => (if project.git { "repository" } else { "none" }, None, false, None),
            };
            // The tip the root layer records, not the tip Git shows now.
            let root_tip = outcome.and_then(|o| o.root_tip.clone());
            ProjectStatus {
                mode: project.mode,
                working: embeddings_work(enabled, project.mode),
                state: project.phase,
                job: project.job,
                git,
                branch,
                detached,
                root_branch,
                root_tip,
                spec_chunks: outcome.map(|o| o.spec_chunks).unwrap_or(0),
                document_chunks: outcome.map(|o| o.document_chunks).unwrap_or(0),
                last_error: project.last_error.clone(),
            }
        }),
    }
}
