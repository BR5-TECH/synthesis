//! Where a run lives on disk, and how it is read back (GSU-FR-RJRF).

use super::*;

/// GSU-FR-RJRF: a project's run order, under the store root.
const QUEUES_DIR: &str = "q";
/// GSU-FR-RJRF: every run's own directory, under the store root.
const RUNS_DIR: &str = "g";
const RUN_FILE: &str = "run.toml";
/// GRD storage: the run's two persisted log streams.
const LOGS_DIR: &str = "logs";
/// GRL-FR-YKRI: the review turn's throwaway checkout.
const REVIEW_DIR: &str = "rv";
/// GRD-FR-KZPT: a merge run's own worktree.
const MERGE_DIR: &str = "mw";

/// The persisted index of a project's run order.
///
/// It carries a summary of each run beside its id, so a reader that needs the
/// order, a draft's lock, or a stream's queue depth opens one file rather than
/// every run record (WKS-FR-YBST).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueIndex {
    #[serde(default)]
    pub project_key: String,
    #[serde(default)]
    pub runs: Vec<QueueIndexEntry>,
}

/// One run, as the order's index summarises it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueIndexEntry {
    pub run_id: String,
    /// The stream the run names, or empty (GRD-FR-PZAK).
    pub stream_id: String,
    pub draft_id: String,
    pub state: GraduationRunState,
    /// GRD-FR-OYPY: the pinned worktree of a direct run, or empty.
    #[serde(default)]
    pub worktree_path: String,
    /// GRD-FR-OYPY: a direct run that was dispatched and has not ended.
    #[serde(default)]
    pub dispatched_direct: bool,
    /// GRD-FR-MRNQ: a merge run, so a stream listing finds a stream's merge run
    /// from the index alone (WKS-FR-YBST).
    #[serde(default)]
    pub merge: bool,
    /// GRD-FR-JOFE: where the author filed the run, so a listing leaves an
    /// archived merge run out of a stream's row without opening the record.
    #[serde(default)]
    pub archived: bool,
}

/// GSU-FR-RJRF: the one root this module writes under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreBase {
    /// `short_data_dir()` in the application.
    pub root: PathBuf,
}

impl StoreBase {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// A project's run order.
    pub fn queue_path(&self, project_key: &str) -> PathBuf {
        self.root
            .join(QUEUES_DIR)
            .join(format!("{}.toml", sanitize_key(project_key)))
    }

    /// One run's own directory.
    pub fn home(&self, run_id: &str) -> PathBuf {
        self.root.join(RUNS_DIR).join(run_id)
    }

    /// One run's whole record.
    pub fn record(&self, run_id: &str) -> PathBuf {
        self.home(run_id).join(RUN_FILE)
    }

    /// GRD-FR-OSCG: the run's two persisted log streams.
    pub fn logs(&self, run_id: &str) -> PathBuf {
        self.home(run_id).join(LOGS_DIR)
    }

    /// GRL-FR-YKRI: the throwaway checkout a review turn stands in.
    ///
    /// A checkout of the stream's revision that is removed when the turn ends,
    /// so the reviewer runs the project's own build and test commands without
    /// reaching the stream at all.
    pub fn review_checkout(&self, run_id: &str) -> PathBuf {
        self.home(run_id).join(REVIEW_DIR)
    }

    /// GRD-FR-KZPT: the worktree a merge run reconciles its merge in.
    ///
    /// Seeded from the run's merge snapshot and owned by the run, so a work turn
    /// stands in it and neither live branch nor live worktree is written until
    /// the merge is applied.
    pub fn merge_worktree(&self, run_id: &str) -> PathBuf {
        self.home(run_id).join(MERGE_DIR)
    }
}

/// A project key reduced to one path-safe segment.
fn sanitize_key(key: &str) -> String {
    let mut out: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

/// GRD-FR-VLFO: a run id, unique across projects.
pub fn new_run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("g{nanos:x}{:x}", std::process::id())
}

/// GRD-FR-LGDV: read one run's record.
pub fn read_run_record(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    run_id: &str,
) -> Result<GraduationRun, String> {
    fs.read_toml(base.record(run_id)).map_err(|e| e.to_string())
}

/// Write one run's record durably.
pub fn write_run_record(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    run: &GraduationRun,
) -> Result<(), String> {
    let home = base.home(&run.id);
    if !home.is_dir() {
        fs.create_dir(&home).map_err(|e| e.to_string())?;
    }
    fs.write_toml_atomic(base.record(&run.id), run)
        .map_err(|e| e.to_string())
}

/// GRD-FR-VLFO: the project's run order.
pub fn read_queue_index(fs: &fsa::FsAccess, base: &StoreBase, project_key: &str) -> QueueIndex {
    fs.read_toml(base.queue_path(project_key))
        .unwrap_or_else(|_| QueueIndex {
            project_key: project_key.to_string(),
            runs: Vec::new(),
        })
}

/// Write the project's run order durably.
pub fn write_queue_index(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    index: &QueueIndex,
) -> Result<(), String> {
    let path = base.queue_path(&index.project_key);
    if let Some(parent) = path.parent() {
        if !parent.is_dir() {
            fs.create_dir(parent).map_err(|e| e.to_string())?;
        }
    }
    fs.write_toml_atomic(path, index).map_err(|e| e.to_string())
}

#[allow(dead_code)]
/// GRD-FR-GMTX: remove everything one run owns.
///
/// A run owns its record and its logs. It owns no branch and no working copy:
/// the stream outlives it.
pub fn remove_run_home(
    fs: &fsa::FsAccess,
    base: &StoreBase,
    run_id: &str,
) -> Result<(), String> {
    let dir = base.home(run_id);
    if dir.exists() {
        // FSA-FR-ZUCF: a run's home holds its review checkout, which the
        // review's own build and test commands write into.
        fs.delete_owned_tree(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The guarded instance every read and write of this module goes through.
pub fn store_fs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Arc<fsa::FsAccess>, String> {
    app.try_state::<fsa::FsAccessState>()
        .and_then(|state| state.get())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())
}

/// The store root: `short_data_dir()`, or whatever root the managed state names.
pub fn store_base<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<StoreBase, String> {
    if let Some(root) = app
        .try_state::<GraduationState>()
        .and_then(|state| state.root())
    {
        return Ok(StoreBase::new(root));
    }
    fsa::short_data_dir()
        .map(StoreBase::new)
        .map_err(|e| e.to_string())
}
