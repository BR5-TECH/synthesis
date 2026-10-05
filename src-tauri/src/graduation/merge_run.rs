//! A merge run: the graduation run a conflict merge is handed to
//! (`GRD-graduation.md` GRD-FR-MRNQ and the requirements that follow it).
//!
//! A stream merge that Git settles on its own never reaches this module. One it
//! cannot settle becomes a run that carries `merge` data. The run is the merge's
//! only durable record. It reconciles the merge in a worktree of its own, seeded
//! from the merge snapshot, and the result reaches the base branch only after a
//! review has let it through.

use super::*;
use crate::streams::{StreamMergeConflict, StreamMergePublication};

/// GRL-FR-CZBT: how many passes one budget window of a merge run grants.
///
/// A constant of the merge run and not a project setting. Continue after the
/// budget is spent moves the window's floor and grants this many passes again.
pub const MERGE_PASS_BUDGET: u32 = 2;

/// How the result of an applied merge was published.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergePublished {
    /// Left unstaged in the base worktree.
    Uncommitted,
    /// One merge commit on the base branch.
    Commit,
}

/// GRD-FR-AQNW: what an applied merge wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationMergeResult {
    pub published: MergePublished,
    /// The merge commit, where the publication made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Project-relative paths the result changes against the base tip.
    #[serde(default)]
    pub merged_paths: Vec<String>,
}

/// GRD-FR-KZPT: what a merge run pins and carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationMergeData {
    /// `Merge <stream name>`: the run's title. Never a draft name.
    pub name: String,
    pub stream_branch: String,
    pub base_branch: String,
    /// The pinned full object id of the base branch.
    pub base_tip: String,
    /// The pinned full object id of the stream branch.
    pub stream_tip: String,
    /// The revision both branches last shared.
    pub merge_base: String,
    /// The merge snapshot commit.
    pub snapshot_commit: String,
    pub publication: StreamMergePublication,
    /// Every path the Git merge changes against the base tip.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// The paths Git could not merge.
    #[serde(default)]
    pub unresolved_paths: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<StreamMergeConflict>,
    /// Written when the merge is applied (GRD-FR-AQNW).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GraduationMergeResult>,
}

/// What the stream merge handoff gives this module.
#[derive(Clone, Debug)]
pub struct MergeRunRequest {
    pub run_id: String,
    pub project_key: String,
    pub stream_id: String,
    pub stream_name: String,
    pub stream_branch: String,
    pub base_branch: String,
    pub base_tip: String,
    pub stream_tip: String,
    pub merge_base: String,
    pub snapshot_commit: String,
    pub publication: StreamMergePublication,
    pub changed_paths: Vec<String>,
    pub unresolved_paths: Vec<String>,
    pub conflicts: Vec<StreamMergeConflict>,
}

/// GRD-FR-VCTH: the title of a merge run.
pub fn merge_title(stream_name: &str) -> String {
    format!("Merge {stream_name}")
}

/// GRD-FR-MRNQ: enqueue a merge run, durably, and tell every reader.
///
/// The run enters `queued` like every run. It is not dispatched here: the caller
/// offers the queue a dispatch once it has released the repository update guard,
/// because a claim of the stream is refused while that guard is held.
pub fn enqueue_merge_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: MergeRunRequest,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    if request.project_key.is_empty() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }

    let title = merge_title(&request.stream_name);
    let prompt = format!(
        "Reconcile the merge of the work stream \"{}\" (branch {}) into the branch {}.",
        request.stream_name, request.stream_branch, request.base_branch
    );
    let input = CapturedGraduationInput {
        draft_id: String::new(),
        draft_name: String::new(),
        prompt_checksum: crate::fs::sha256_bytes(prompt.as_bytes()),
        prompt,
        captured_at: crate::notes::now_rfc3339(),
    };
    let mut run = commands::build_run(
        &request.project_key,
        request.stream_id.clone(),
        request.stream_name.clone(),
        None,
        input,
        StandingWork::Keep,
        None,
    );
    run.id = request.run_id.clone();
    run.merge = Some(GraduationMergeData {
        name: title,
        stream_branch: request.stream_branch,
        base_branch: request.base_branch,
        base_tip: request.base_tip,
        stream_tip: request.stream_tip,
        merge_base: request.merge_base,
        snapshot_commit: request.snapshot_commit,
        publication: request.publication,
        changed_paths: request.changed_paths,
        unresolved_paths: request.unresolved_paths,
        conflicts: request.conflicts,
        result: None,
    });
    // GRS-FR-KDOY: both log streams are created in the write that creates the
    // run, so a run that has emitted nothing still has two readable ones.
    crate::graduation::logs::initialize(app, &mut run);
    runs::save_run(app, &mut run)?;
    events::announce_queue(app, &run);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation enqueued a merge run",
        log_fields! {
            "run_id" => run.id.clone(),
            "stream_id" => run.stream_id.clone(),
            "unresolved" => run.merge.as_ref().map(|m| m.unresolved_paths.len()).unwrap_or(0) as i64,
        },
    );
    Ok(run)
}

impl GraduationRun {
    /// GRD-FR-MRNQ: whether this run is a merge run.
    pub fn is_merge(&self) -> bool {
        self.merge.is_some()
    }
}

/// What one boundary record of a merge run says (GLG-FR-QNLC, GLG-FR-WYSP).
pub(crate) struct MergeBoundary<'a> {
    pub level: crate::logging::LogLevel,
    pub domains: &'a [Domain],
    pub message: &'a str,
    /// Which boundary: `dispatch`, `tip_check`, `apply` or `cleanup`.
    pub boundary: &'a str,
    /// What the boundary found, in the words of GLG-FR-QNLC.
    pub outcome: &'a str,
    /// The pass the boundary lies in, where it lies inside one.
    pub pass: Option<u32>,
    /// How many paths the boundary concerns, where it concerns any.
    pub paths: Option<usize>,
    /// Anything else the boundary can say without file content.
    pub extra: crate::logging::Fields,
}

/// GLG-FR-QNLC, GLG-FR-WYSP, GRB-FR-OHWT: one record at one boundary of a merge
/// run, correlated by the run id.
///
/// It carries the run, the stream, the outcome, the pass where the boundary lies
/// inside one, a count of paths, and the object ids of the two pinned tips. It
/// carries no file content, no marker text, no finding text and no prompt.
pub(crate) fn log_merge_boundary<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    record: MergeBoundary<'_>,
) {
    let mut fields = log_fields! {
        "run_id" => run.id.clone(),
        "stream_id" => run.stream_id.clone(),
        "boundary" => record.boundary,
        "outcome" => record.outcome,
    };
    if let Some(data) = run.merge.as_ref() {
        fields.insert("base_tip".to_string(), serde_json::json!(data.base_tip));
        fields.insert("stream_tip".to_string(), serde_json::json!(data.stream_tip));
    }
    if let Some(pass) = record.pass {
        fields.insert("pass".to_string(), serde_json::json!(pass));
    }
    if let Some(paths) = record.paths {
        fields.insert("paths".to_string(), serde_json::json!(paths as u64));
    }
    fields.extend(record.extra);
    crate::logging::log(
        app,
        &BUFFER,
        record.level,
        record.domains,
        record.message,
        fields,
    );
}

/// GRD-FR-PFMD / GRD-FR-KZPT: remove everything an **ended** merge run owns.
///
/// Idempotent. A run a loop of this process is still inside is left alone,
/// whether or not the loop still holds the run's claim: its own thread removes
/// what the run owns when the turn that stands in it has returned, so a discard
/// never pulls a worktree from under a turn that is still stopping.
pub fn reclaim_ended_merge_run<R: tauri::Runtime>(app: &tauri::AppHandle<R>, run: &GraduationRun) {
    if run.merge.is_none() || !run.state.is_terminal() {
        return;
    }
    if app
        .try_state::<GraduationState>()
        .is_some_and(|state| state.is_looping(&run.id))
    {
        return;
    }
    // GLG-FR-QNLC: one record for one cleanup. A run that has nothing left
    // standing is reclaimed again at every later read, and says nothing.
    if !driver::merge_workspace::stands(app, run) {
        return;
    }
    let (level, outcome, message, extra) = match driver::merge_workspace::release(app, run) {
        Ok(()) => (
            crate::logging::LogLevel::Info,
            "reclaimed",
            "graduation reclaimed what a merge run owned",
            crate::logging::Fields::new(),
        ),
        Err(reason) => (
            crate::logging::LogLevel::Error,
            "reclaim_failed",
            "graduation could not reclaim what a merge run owned",
            log_fields! { "reason" => reason },
        ),
    };
    log_merge_boundary(
        app,
        run,
        MergeBoundary {
            level,
            domains: &[Domain::Backend],
            message,
            boundary: "cleanup",
            outcome,
            pass: None,
            paths: None,
            extra,
        },
    );
}
