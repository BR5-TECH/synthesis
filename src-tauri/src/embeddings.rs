//! The embeddings index (`EMB-embeddings-index.md`).
//!
//! An optional vector index over the chunks of specifications and documents,
//! fused with the BM25 indexes of `BMI-bm25-indexing.md` by Reciprocal Rank
//! Fusion. The vectors live in one SQLite store per project, in the
//! repository store (EMB-FR-RFNF). The specification vectors are layers: the
//! root layer of the default branch tip, read from Git objects, and the
//! difference of the active branch (EMB-FR-LGLI, EMB-FR-DCXI).
//!
//! ## Why the working copy comes from the BM25 snapshot
//!
//! A chunk must have one ID in both indexes (EMB-FR-IRQA). Reading the chunks
//! from the published BM25 snapshot gives the same cut and the same IDs, and
//! it also makes every change that reaches BM25 reach this index on the same
//! channels and under the same rules (EMB-FR-HWRQ, EMB-FR-IBXK): this module
//! mounts no watcher and reads no file of the working copy.

use std::time::Instant;

use tauri::Manager;

mod catalog;
mod commands;
mod engine;
mod hybrid;
mod jobs;
mod layers;
mod model;
mod rrf;
mod service;
mod settings;
mod store;

pub use catalog::{list as list_models, EmbeddingModelEntry};
pub use commands::*;
pub use engine::ReindexScope;
pub use hybrid::{hybrid_search, VectorRanker};
pub use jobs::EMBEDDINGS_STATUS_CHANGED;
pub use service::{EmbeddingsService, EmbeddingsStatus, MountInput};
pub use settings::{EmbeddingsRecord, EmbeddingsSettings, ProjectEmbeddingsMode};

use crate::bm25_index::{IndexId, PassScope};
use crate::logging::{log_warn, Domain, BUFFER};

/// EMB-FR-QNWF, EMB-FR-CTXC: no project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// EMB-FR-QNWF: embeddings do not work for the open project.
pub const ERR_EMBEDDINGS_NOT_WORKING: &str = "embeddings_not_working";
/// EMB-FR-JFOV: the model id is not one of `list_embedding_models`.
pub const ERR_UNKNOWN_MODEL: &str = "unknown_model";

impl Default for EmbeddingsService {
    /// The production service: models are cached under `app_data_dir()/models/`
    /// (EMB-FR-NNOK) and one worker thread runs the jobs.
    fn default() -> Self {
        let cache_dir = models_dir();
        EmbeddingsService::new(Box::new(model::FastEmbedFactory { cache_dir }), true)
    }
}

/// EMB-FR-NNOK, EMB-FR-KUFM: the model cache, or the reason there is none.
/// There is no other folder to fall back to.
fn models_dir() -> Result<std::path::PathBuf, String> {
    crate::fs::app_data_dir()
        .map(|dir| dir.join("models"))
        .map_err(|e| format!("the model cache folder is not available: {e}"))
}

/// EMB-FR-KUFM: FastEmbed and `hf-hub` read the cache folder and the hub
/// address from the environment, and `HF_HOME` wins over the folder they are
/// given. This sets their cache folder to `app_data_dir()/models/` and removes
/// the two variables that would send a download or a load somewhere else.
///
/// Called first in `run`, while the process has only the main thread, because
/// a change of the environment is not safe while other threads read it.
pub fn pin_model_cache_environment() {
    let names = model_cache_environment(Ok(std::path::PathBuf::new())).map(|(name, _)| name);
    let _ = INHERITED_ENVIRONMENT.set(names.map(|name| (name, std::env::var_os(name))));
    for (name, value) in model_cache_environment(models_dir()) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

/// EMB-FR-MRDV: the three variables as the application started with them,
/// recorded before they were pinned.
static INHERITED_ENVIRONMENT: std::sync::OnceLock<[(&'static str, Option<std::ffi::OsString>); 3]> =
    std::sync::OnceLock::new();

/// EMB-FR-MRDV: give a child process the hub variables that the application
/// started with, not the pinned ones. Before the pin, it changes nothing.
pub trait InheritedEnvironment {
    fn with_inherited_hub_environment(&mut self) -> &mut Self;
}

impl InheritedEnvironment for std::process::Command {
    fn with_inherited_hub_environment(&mut self) -> &mut Self {
        apply_inherited(self, INHERITED_ENVIRONMENT.get());
        self
    }
}

impl InheritedEnvironment for tokio::process::Command {
    fn with_inherited_hub_environment(&mut self) -> &mut Self {
        apply_inherited(self.as_std_mut(), INHERITED_ENVIRONMENT.get());
        self
    }
}

fn apply_inherited(
    command: &mut std::process::Command,
    inherited: Option<&[(&'static str, Option<std::ffi::OsString>); 3]>,
) {
    for (name, value) in inherited.into_iter().flatten() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
}

/// EMB-FR-KUFM: the value each variable gets, `None` for one that is removed.
/// With no cache folder, FastEmbed gets none either, and the factory refuses
/// every download and load.
fn model_cache_environment(
    models: Result<std::path::PathBuf, String>,
) -> [(&'static str, Option<std::ffi::OsString>); 3] {
    [
        ("HF_HOME", None),
        ("HF_ENDPOINT", None),
        ("FASTEMBED_CACHE_DIR", models.ok().map(std::path::PathBuf::into_os_string)),
    ]
}

/// Read the Global settings at startup and start the worker when embeddings are
/// on. Called once from the application's setup.
pub fn on_app_started<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let (Some(service), Some(store)) = (
        app.try_state::<EmbeddingsService>(),
        app.try_state::<crate::global_settings::GlobalSettingsStore>(),
    ) else {
        return;
    };
    let record = store.load_embeddings_config().unwrap_or_default();
    let enabled = record.enabled;
    service.apply_settings(record);
    if enabled {
        service.ensure_worker(app);
    }
}

/// EMB-FR-JIJH, EMB-FR-NHQG: a content root was mounted — at project open and
/// at every change of the active worktree or its branch (WTC-FR-08). Called
/// after the BM25 indexer mounted, so its generation is the new one.
pub fn on_content_root_mounted<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(service) = app.try_state::<EmbeddingsService>() else {
        return;
    };
    let Some(project) = app.try_state::<crate::project::ProjectState>() else {
        return;
    };
    let (Some(root), Some(indexer)) =
        (project.root(), app.try_state::<crate::bm25_index::Bm25Indexer>())
    else {
        return;
    };
    let key = project.slot_key();
    let mode = app
        .try_state::<crate::global_settings::GlobalSettingsStore>()
        .and_then(|store| store.load_embeddings_mode(&key).ok())
        .unwrap_or_default();
    let git = crate::changes::open_repo(&root).is_ok();
    service.mount(MountInput {
        key,
        git,
        root,
        store_dir: project.require_store().ok(),
        mode,
        generation: indexer.generation(),
    });
    service.ensure_worker(app);
    service.emit(app);
}

/// EMB-FR-RMZH: the project closed.
pub fn on_project_closed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(service) = app.try_state::<EmbeddingsService>() {
        service.unmount();
        service.emit(app);
    }
}

/// EMB-FR-HWRQ, EMB-FR-IBXK: the BM25 indexer published a pass of `scope`.
pub fn note_index_published<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    scope: PassScope,
    generation: u64,
) {
    if let Some(service) = app.try_state::<EmbeddingsService>() {
        service.index_published(scope.artifacts || scope.documents, generation, Instant::now());
    }
}

/// EMB-FR-VQSK: Git metadata changed, or the main window gained focus.
pub fn note_root_tip_check<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(service) = app.try_state::<EmbeddingsService>() {
        service.request_root_check();
    }
}

/// The vector half of a hybrid search, for the open project.
pub struct ServiceRanker<'a, R: tauri::Runtime> {
    service: &'a EmbeddingsService,
    app: &'a tauri::AppHandle<R>,
}

/// The ranker of the open project, or `None` when the service is not managed.
pub fn ranker<'a, R: tauri::Runtime>(
    app: &'a tauri::AppHandle<R>,
    service: Option<&'a EmbeddingsService>,
) -> Option<ServiceRanker<'a, R>> {
    service.map(|service| ServiceRanker { service, app })
}

impl<R: tauri::Runtime> VectorRanker for ServiceRanker<'_, R> {
    fn nearest(
        &self,
        index: IndexId,
        query: &str,
        n: usize,
    ) -> Result<Option<Vec<(String, String)>>, String> {
        let Some((path, view)) = self.service.ready_view(index) else {
            return Ok(None);
        };
        let vector = self.service.embed_query(query)?;
        let store = store::Store::open_existing(&path)?;
        store.nearest(view, &vector, n).map(Some)
    }

    fn report_failure(&self, reason: &str) {
        log_warn(
            self.app,
            &BUFFER,
            &[Domain::Backend],
            "vector search failed, BM25 results used",
            crate::log_fields! { "reason" => reason },
        );
    }
}

/// EMB-FR-BWTI: hybrid search of the open project through its managed state.
pub fn search_open_project<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    indexer: &crate::bm25_index::Bm25Indexer,
    index: IndexId,
    query: &str,
    limit: usize,
) -> Vec<crate::bm25_index::ChunkHit> {
    let service = app.try_state::<EmbeddingsService>();
    let ranker = ranker(app, service.as_deref());
    hybrid_search(indexer, ranker.as_ref().map(|r| r as &dyn VectorRanker), index, query, limit)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use layers::test_support;

/// A fake model factory for the tests of other modules: no download, no model.
#[cfg(test)]
pub(crate) fn test_factory() -> (Box<dyn model::EmbedderFactory>, std::sync::Arc<model::fake::Log>)
{
    let (factory, log) = model::fake::FakeFactory::new();
    (Box::new(factory), log)
}
