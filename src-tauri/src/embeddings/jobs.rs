//! The worker and the jobs it runs (EMB-FR-NNOK, EMB-FR-LYKW, EMB-FR-OVPR,
//! EMB-FR-JIJH, EMB-FR-NHQG, EMB-FR-VQSK, EMB-FR-TGCL, EMB-FR-HRYV,
//! EMB-FR-SWRV, EMB-FR-HDPR, EMB-FR-OLDK).

use std::sync::atomic::Ordering;
use std::time::Instant;

use tauri::{Emitter, Manager};

use super::engine::{self, Evaluation, ReindexScope};
use super::layers::{self, GitFacts};
use super::service::{
    EmbeddingsService, JobKind, JobStatus, ModelState, Phase, PROGRESS_EMIT_INTERVAL,
};
use super::settings::embeddings_work;
use super::store::Store;
use crate::bm25_index::{Bm25Indexer, IndexId};
use crate::logging::{log_debug, log_error, log_info, log_warn, Domain, BUFFER};
use crate::progress::{self, OperationState, ProgressRegistry};

/// EMB-FR-ZFGT: the event that carries the status.
pub const EMBEDDINGS_STATUS_CHANGED: &str = "embeddings-status-changed";

/// One unit of work for the worker.
#[derive(Debug)]
pub(super) enum Work {
    PrepareModel(String),
    Full { serial: u64, kind: JobKind, drop: Option<ReindexScope> },
    RootCheck { serial: u64 },
    Incremental { serial: u64 },
}

impl EmbeddingsService {
    /// Start the worker thread once, when this service runs one.
    pub fn ensure_worker<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        if !self.spawn_worker {
            return;
        }
        let Ok(mut started) = self.worker_started.lock() else {
            return;
        };
        if *started {
            return;
        }
        *started = true;
        let app = app.clone();
        let spawned = std::thread::Builder::new().name("embeddings".to_string()).spawn(move || {
            let service = app.state::<EmbeddingsService>();
            service.worker_loop(&app);
        });
        if spawned.is_err() {
            *started = false;
        }
    }

    fn worker_loop<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        loop {
            let work = {
                let mut st = self.lock();
                loop {
                    let now = Instant::now();
                    if let Some(work) = self.next_work(&mut st, now) {
                        break work;
                    }
                    let deadline = st.project.as_ref().and_then(|p| p.pending.incremental_at);
                    st = match deadline.filter(|at| *at > now) {
                        Some(at) => {
                            let wait = at.saturating_duration_since(now);
                            match self.wake.wait_timeout(st, wait) {
                                Ok((guard, _)) => guard,
                                Err(poisoned) => poisoned.into_inner().0,
                            }
                        }
                        None => match self.wake.wait(st) {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        },
                    };
                }
            };
            self.run_work(app, work);
        }
    }

    /// Run the next due job at `now`, synchronously. Returns false when no job
    /// is due. The worker thread and the tests both go through this.
    pub fn run_next<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, now: Instant) -> bool {
        let work = {
            let mut st = self.lock();
            self.next_work(&mut st, now)
        };
        match work {
            Some(work) => {
                self.run_work(app, work);
                true
            }
            None => false,
        }
    }

    /// Run every job due at `now`. Test-facing.
    #[cfg(test)]
    pub fn run_until_idle<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, now: Instant) {
        let mut guard = 0;
        while self.run_next(app, now) {
            guard += 1;
            assert!(guard < 100, "the job queue does not settle");
        }
    }

    /// The next job, in the order: model, Reindex, full job, root check,
    /// incremental pass. Takes it off the pending set.
    pub(super) fn next_work(&self, st: &mut super::service::State, now: Instant) -> Option<Work> {
        if !st.settings.enabled {
            return None;
        }
        let model_id = st.settings.model_id();
        if st.model.loaded.as_deref() != Some(model_id.as_str()) {
            // EMB-FR-KEGS: a failed download is not tried again on its own.
            if st.model.state == ModelState::Failed {
                return None;
            }
            st.model.state = ModelState::Downloading;
            return Some(Work::PrepareModel(model_id));
        }
        let enabled = st.settings.enabled;
        let project = st.project.as_mut()?;
        if !embeddings_work(enabled, project.mode) || !project.snapshot_ready {
            return None;
        }
        let serial = project.serial;
        if let Some(scope) = project.pending.reindex.take() {
            return Some(Work::Full { serial, kind: JobKind::Reindex, drop: Some(scope) });
        }
        if let Some(kind) = project.pending.full.take() {
            return Some(Work::Full { serial, kind, drop: None });
        }
        if std::mem::take(&mut project.pending.root_check) {
            return Some(Work::RootCheck { serial });
        }
        if project.pending.incremental_at.is_some_and(|at| at <= now) {
            project.pending.incremental_at = None;
            return Some(Work::Incremental { serial });
        }
        None
    }

    fn run_work<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, work: Work) {
        match work {
            Work::PrepareModel(id) => self.prepare_model(app, &id),
            Work::Full { serial, kind, drop } => self.run_job(app, serial, kind, drop, false),
            Work::RootCheck { serial } => self.root_check(serial),
            Work::Incremental { serial } => self.incremental(app, serial),
        }
    }

    /// EMB-FR-NNOK, EMB-FR-LYKW: download the model when the cache lacks it,
    /// then load it.
    fn prepare_model<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, id: &str) {
        let Some(spec) = super::catalog::find(id) else {
            self.model_failed(app, id, "unknown model".to_string());
            return;
        };
        let cached = self.factory.is_cached(&spec);
        {
            let mut st = self.lock();
            if !still_wanted(&mut st, id) {
                return;
            }
            st.model.state = if cached { ModelState::Loading } else { ModelState::Downloading };
            st.model.download = None;
        }
        self.emit(app);
        if !cached {
            let registry = app.state::<ProgressRegistry>();
            let operation = progress::register_and_publish(
                app,
                &registry,
                "embed",
                "Downloading embedding model…",
                None,
                None,
            );
            log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "embedding model download started",
                crate::log_fields! { "model" => id },
            );
            let result = self.factory.download(&spec, &mut |p| {
                self.lock().model.download = Some(p);
                progress::update_and_publish(app, &registry, operation, Some(p.completed), p.total);
                self.emit_progress(app);
            });
            if let Err(reason) = result {
                progress::terminate_and_publish(app, &registry, operation, OperationState::Failed);
                self.model_failed(app, id, reason);
                return;
            }
            progress::terminate_and_publish(app, &registry, operation, OperationState::Finished);
            log_info(
                app,
                &BUFFER,
                &[Domain::Backend],
                "embedding model downloaded",
                crate::log_fields! { "model" => id },
            );
        }
        {
            let mut st = self.lock();
            // The settings may have moved on while the model downloaded.
            if !still_wanted(&mut st, id) {
                return;
            }
            st.model.state = ModelState::Loading;
        }
        self.emit(app);
        let loaded = self.factory.load(&spec);
        let mut st = self.lock();
        // The settings may have moved on while the model loaded.
        if !still_wanted(&mut st, id) {
            return;
        }
        match loaded {
            Ok(embedder) => {
                let mut slot = match self.embedder.lock() {
                    Ok(slot) => slot,
                    Err(poisoned) => poisoned.into_inner(),
                };
                *slot = Some(super::service::LoadedModel { embedder, prefixes: spec.prefixes });
                self.embedder_serial.fetch_add(1, Ordering::SeqCst);
                drop(slot);
                st.model.state = ModelState::Ready;
                st.model.loaded = Some(id.to_string());
                st.model.download = None;
                st.model.error = None;
                drop(st);
                log_info(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "embedding model loaded",
                    crate::log_fields! { "model" => id },
                );
                self.emit(app);
            }
            Err(reason) => {
                drop(st);
                self.model_failed(app, id, reason);
            }
        }
    }

    fn model_failed<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, id: &str, reason: String) {
        // EMB-FR-LYKW: a hub error can name a signed link; its query string
        // is neither stored nor logged.
        let reason = super::model::without_url_queries(&reason);
        {
            let mut st = self.lock();
            if st.settings.model_id() == id {
                st.model.state = ModelState::Failed;
                st.model.error = Some(reason.clone());
                st.model.download = None;
            }
        }
        log_error(
            app,
            &BUFFER,
            &[Domain::Backend, Domain::Remote],
            "embedding model unavailable",
            crate::log_fields! { "model" => id, "reason" => reason },
        );
        self.emit(app);
    }

    /// EMB-FR-VQSK, EMB-FR-HTIQ: queue a full evaluation when the root tip,
    /// the branch, or the `HEAD` moved.
    fn root_check(&self, serial: u64) {
        let root = {
            let st = self.lock();
            match st.project.as_ref().filter(|p| p.serial == serial) {
                Some(project) if project.facts.is_some() => project.root.clone(),
                _ => return,
            }
        };
        let facts = layers::git_facts(&root);
        let mut st = self.lock();
        if let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) {
            if let Some(kind) = project.facts.as_ref().and_then(|previous| moved(previous, &facts))
            {
                project.pending.full.get_or_insert(kind);
            }
        }
    }

    /// EMB-FR-BZZT, EMB-FR-NHQG: an incremental pass, or a full evaluation
    /// when the branch, the `HEAD`, or the root tip changed under it.
    fn incremental<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, serial: u64) {
        let previous = {
            let st = self.lock();
            match st.project.as_ref().filter(|p| p.serial == serial) {
                Some(project) => project.facts.clone(),
                None => return,
            }
        };
        // The job reads the Git facts itself and makes the pass a full
        // evaluation when they moved (EMB-FR-DQVX), so it is the one place that
        // decides.
        match previous {
            Some(_) => self.run_job(app, serial, JobKind::Incremental, None, true),
            None => self.run_job(app, serial, JobKind::OpenCheck, None, false),
        }
    }

    /// One evaluation of the layers (EMB-FR-OVPR, EMB-FR-JIJH, EMB-FR-NHQG,
    /// EMB-FR-DQVX, EMB-FR-TGCL, EMB-FR-UJLS, EMB-FR-HRYV, EMB-FR-BZZT).
    fn run_job<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        serial: u64,
        mut kind: JobKind,
        drop: Option<ReindexScope>,
        skip_root: bool,
    ) {
        let start_epoch = self.epoch.load(Ordering::SeqCst);
        let (root, store_dir, generation, model_id) = {
            let st = self.lock();
            match st.project.as_ref().filter(|p| p.serial == serial) {
                Some(project) => (
                    project.root.clone(),
                    project.store_dir.clone(),
                    project.generation,
                    st.settings.model_id(),
                ),
                None => return,
            }
        };
        let Some(indexer) = app.try_state::<Bm25Indexer>() else {
            return;
        };
        if indexer.generation() != generation {
            return;
        }
        let snapshot = indexer.snapshot();
        let working = layers::chunks_from(snapshot.chunks(IndexId::Spec));
        let documents = layers::chunks_from(snapshot.chunks(IndexId::Documents));
        // EMB-FR-JIJH: the documents of a collection that has not read its
        // sources yet are not compared, so a project open keeps their vectors.
        let documents_fed = snapshot.documents_fed();
        if !documents_fed {
            log_debug(
                app,
                &BUFFER,
                &[Domain::Backend],
                "embeddings keep the documents layer until the collection is read",
                crate::log_fields! { "job" => job_name(kind) },
            );
        }
        let fingerprint = fingerprint_of(&working, documents_fed.then_some(&documents));
        let facts = layers::git_facts(&root);
        // EMB-FR-DQVX: the root tip or the branch moved after the incremental
        // pass was chosen. The pass then becomes a full evaluation, so the
        // facts it records always match the layers it wrote.
        let mut skip_root = skip_root;
        if kind == JobKind::Incremental {
            let previous = self
                .lock()
                .project
                .as_ref()
                .filter(|p| p.serial == serial)
                .and_then(|p| p.facts.clone());
            if let Some(full) = previous.as_ref().and_then(|previous| moved(previous, &facts)) {
                kind = full;
                skip_root = false;
            }
        }
        // EMB-FR-USBA: only a full evaluation deletes branch layers, so only it
        // lists the worktrees.
        let facts = if skip_root { facts } else { layers::keep_checked_out(facts, &root) };
        // EMB-FR-BZZT: a pass of another index changed no chunk this index
        // holds, so there is nothing to compare and no operation to show.
        if kind == JobKind::Incremental {
            let unchanged = self
                .lock()
                .project
                .as_ref()
                .filter(|p| p.serial == serial)
                .and_then(|p| p.outcome.as_ref())
                .is_some_and(|o| o.fingerprint == fingerprint);
            if unchanged {
                return;
            }
        }

        let opened = store_dir
            .as_ref()
            .ok_or_else(|| crate::repository_store::ERR_STORE_UNAVAILABLE.to_string())
            .and_then(|dir| Store::open(dir, &model_id));
        let mut store = match opened {
            Ok((store, rebuilt)) => {
                if rebuilt && kind != JobKind::ModelSwitch {
                    kind = JobKind::StaleRebuild;
                }
                // EMB-FR-UJLS: a rebuilt store removed the layers the last view
                // named, so that view is gone whatever this job ends in.
                if rebuilt {
                    let mut st = self.lock();
                    if let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) {
                        project.outcome = None;
                    }
                }
                if kind == JobKind::OpenCheck
                    && store.layers().map(|l| l.is_empty()).unwrap_or(false)
                {
                    kind = JobKind::FirstIndexing;
                }
                store
            }
            Err(reason) => {
                self.job_failed(app, serial, kind, reason, true);
                return;
            }
        };

        {
            let mut st = self.lock();
            let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) else {
                return;
            };
            project.phase =
                if kind == JobKind::Incremental { Phase::Updating } else { Phase::Preparing };
            project.job = Some(JobStatus { kind, completed: None, total: None });
        }
        #[cfg(test)]
        self.jobs_run.lock().unwrap().push(kind);
        self.emit(app);
        let registry = app.state::<ProgressRegistry>();
        let operation = progress::register_and_publish(
            app,
            &registry,
            "embed",
            kind.label(),
            Some(root.clone()),
            None,
        );
        let started = Instant::now();
        log_info(
            app,
            &BUFFER,
            &[Domain::Backend],
            "embedding job started",
            crate::log_fields! { "job" => job_name(kind) },
        );

        let root_reader = |tip: &str, prefix: &str| -> Result<layers::RootRead, String> {
            let repo = crate::changes::open_repo(&root)?;
            layers::root_chunks(&repo, tip, prefix)
        };
        let mut embed = |texts: &[String]| self.embed_batch(texts);
        let cancelled = || self.epoch.load(Ordering::SeqCst) != start_epoch;
        let mut on_progress = |done: u64, total: u64| {
            {
                let mut st = self.lock();
                if let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) {
                    project.job =
                        Some(JobStatus { kind, completed: Some(done), total: Some(total) });
                }
            }
            progress::update_and_publish(app, &registry, operation, Some(done), Some(total));
            self.emit_progress(app);
        };
        let result = engine::evaluate(
            &mut store,
            &Evaluation {
                facts: &facts,
                working: &working,
                documents: documents_fed.then_some(&documents),
                skip_root,
                drop,
                root_reader: &root_reader,
            },
            &mut engine::Job {
                embed: &mut embed,
                cancelled: &cancelled,
                progress: &mut on_progress,
            },
        );

        match result {
            Ok(mut outcome) => {
                outcome.fingerprint = fingerprint;
                // EMB-FR-OLDK: a specification of the root tip that gave no
                // chunks. A project-relative path, never its content.
                for (path, reason) in std::mem::take(&mut outcome.skipped) {
                    log_warn(
                        app,
                        &BUFFER,
                        &[Domain::Backend],
                        "embeddings skipped a file",
                        crate::log_fields! { "path" => path, "reason" => reason },
                    );
                }
                let fields = crate::log_fields! {
                    "job" => job_name(kind),
                    "compared" => outcome.compared,
                    "embedded" => outcome.embedded,
                    "removed" => outcome.removed,
                    "specChunks" => outcome.spec_chunks,
                    "documentChunks" => outcome.document_chunks,
                    "durationMs" => started.elapsed().as_millis() as u64,
                };
                let applied = {
                    let mut st = self.lock();
                    match st.project.as_mut().filter(|p| p.serial == serial) {
                        Some(project) => {
                            project.outcome = Some(outcome);
                            project.facts = Some(facts);
                            project.phase = Phase::Ready;
                            project.job = None;
                            project.last_error = None;
                            true
                        }
                        None => false,
                    }
                };
                let state =
                    if applied { OperationState::Finished } else { OperationState::Cancelled };
                progress::terminate_and_publish(app, &registry, operation, state);
                if applied {
                    log_info(app, &BUFFER, &[Domain::Backend], "embedding job finished", fields);
                }
                self.emit(app);
            }
            Err(reason) if reason == engine::CANCELLED => {
                {
                    let mut st = self.lock();
                    if let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) {
                        project.job = None;
                    }
                }
                progress::terminate_and_publish(
                    app,
                    &registry,
                    operation,
                    OperationState::Cancelled,
                );
                log_info(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "embedding job cancelled",
                    crate::log_fields! { "job" => job_name(kind) },
                );
                self.emit(app);
            }
            Err(reason) => {
                progress::terminate_and_publish(app, &registry, operation, OperationState::Failed);
                self.job_failed(app, serial, kind, reason, false);
            }
        }
    }

    /// EMB-FR-EAEA, EMB-FR-RFBJ: a failed job leaves the store as it was and
    /// raises no error in search. Search keeps the last good view; without one,
    /// the index is unavailable until the next trigger.
    fn job_failed<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        serial: u64,
        kind: JobKind,
        reason: String,
        store_unavailable: bool,
    ) {
        // EMB-FR-RFBJ: a store that cannot be read or written is unavailable,
        // busy with another process included.
        let store_unavailable = store_unavailable || reason.starts_with(super::store::UNAVAILABLE);
        {
            let mut st = self.lock();
            let Some(project) = st.project.as_mut().filter(|p| p.serial == serial) else {
                return;
            };
            project.job = None;
            project.last_error = Some(reason.clone());
            project.phase = if store_unavailable || project.outcome.is_none() {
                Phase::Unavailable
            } else {
                Phase::Ready
            };
        }
        log_error(
            app,
            &BUFFER,
            &[Domain::Backend],
            if store_unavailable { "embeddings store unavailable" } else { "embedding job failed" },
            crate::log_fields! { "job" => job_name(kind), "reason" => reason },
        );
        self.emit(app);
    }

    /// EMB-FR-ZFGT: publish the status.
    pub fn emit<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        let status = {
            let mut st = self.lock();
            st.last_progress_emit = Some(Instant::now());
            super::service::status_of(&st)
        };
        let _ = app.emit(EMBEDDINGS_STATUS_CHANGED, status);
    }

    /// EMB-FR-ZFGT: publish a progress-only change, at most a few times a
    /// second.
    fn emit_progress<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        let due =
            self.lock().last_progress_emit.is_none_or(|at| at.elapsed() >= PROGRESS_EMIT_INTERVAL);
        if due {
            self.emit(app);
        }
    }
}

/// Whether model `id` is still the one the settings ask for. When the switch
/// was turned off meanwhile, the model state goes back to not downloaded, so
/// the status does not keep a download or a load that was abandoned.
fn still_wanted(st: &mut super::service::State, id: &str) -> bool {
    if !st.settings.enabled {
        st.model.state = ModelState::NotDownloaded;
        st.model.download = None;
        return false;
    }
    st.settings.model_id() == id
}

/// Whether the project moved into or out of a Git repository.
fn repository_changed(previous: &GitFacts, now: &GitFacts) -> bool {
    matches!(previous, GitFacts::NoRepository) != matches!(now, GitFacts::NoRepository)
}

/// EMB-FR-VQSK, EMB-FR-NHQG, EMB-FR-HTIQ: the full evaluation that a change of
/// the Git facts needs — a root update when the root tip moved, a branch update
/// when the branch, the `HEAD`, or the repository changed — or `None`.
fn moved(previous: &GitFacts, now: &GitFacts) -> Option<JobKind> {
    if previous.root_tip() != now.root_tip() {
        Some(JobKind::RootUpdate)
    } else if previous.branch() != now.branch() || repository_changed(previous, now) {
        Some(JobKind::BranchUpdate)
    } else {
        None
    }
}

/// EMB-FR-BZZT: the chunk IDs and content hashes a pass saw, as one digest.
/// Documents that were not compared (EMB-FR-JIJH) give a digest of their own,
/// so the first pass that compares them never reads as unchanged.
fn fingerprint_of(working: &layers::Chunks, documents: Option<&layers::Chunks>) -> String {
    let mut text = String::new();
    if documents.is_none() {
        text.push_str("documents not read\n");
    }
    for (id, chunk) in working.iter().chain(documents.into_iter().flatten()) {
        text.push_str(id);
        text.push('\u{1f}');
        text.push_str(&chunk.hash);
        text.push('\n');
    }
    crate::fs::sha256_bytes(text.as_bytes())
}

fn job_name(kind: JobKind) -> &'static str {
    match kind {
        JobKind::FirstIndexing => "first_indexing",
        JobKind::OpenCheck => "open_check",
        JobKind::BranchUpdate => "branch_update",
        JobKind::RootUpdate => "root_update",
        JobKind::ModelSwitch => "model_switch",
        JobKind::StaleRebuild => "stale_rebuild",
        JobKind::Reindex => "reindex",
        JobKind::Incremental => "incremental",
    }
}

#[cfg(test)]
mod tests;
