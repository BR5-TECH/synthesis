//! What one stream's last update did, durably (WKS-FR-ZKUP, WKS-FR-VQRD).
//!
//! An update brings the stream branch up to a pinned revision of its base
//! branch. It runs for minutes and may stop to ask the author a question, so
//! what it settled is written beside the stream rather than returned to the one
//! call that started it.

use std::path::PathBuf;

use super::*;

/// WKS-FR-ZKUP: the directory every stream's update record stands under.
const UPDATES_DIR: &str = "wu";
const RECORD_SUFFIX: &str = ".toml";

impl StreamStore {
    /// The directory every update record of every project stands under.
    pub fn updates_root(&self) -> PathBuf {
        self.root().join(UPDATES_DIR)
    }

    /// One stream's update record.
    pub fn update_record(&self, stream_id: &str) -> PathBuf {
        self.updates_root().join(format!("{stream_id}{RECORD_SUFFIX}"))
    }
}

/// WKS-FR-NRQT: which direction Git is asked to take the stream up to its base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamUpdateStrategy {
    /// Merge the pinned base revision into the stream branch, as one merge
    /// commit.
    #[default]
    MergeSource,
    /// Replay the stream commits onto the pinned base revision.
    RebaseSource,
}

impl StreamUpdateStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MergeSource => "merge_source",
            Self::RebaseSource => "rebase_source",
        }
    }
}

/// WKS-FR-VQRD: where one stream's last update stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamUpdateState {
    /// An update of this stream is working now.
    #[default]
    Running,
    /// The stream branch now holds the pinned base revision.
    Updated,
    /// The stream already held the pinned revision.
    NothingToUpdate,
    /// Git and its semantic turns settled nothing. Nothing was written.
    Conflicted,
    /// A semantic turn asked the author. The update waits for their answer.
    Escalated,
    /// The author stopped the update. Neither branch moved.
    Cancelled,
    /// A typed refusal, which `failure` names.
    Failed,
}

impl StreamUpdateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Updated => "updated",
            Self::NothingToUpdate => "nothing_to_update",
            Self::Conflicted => "conflicted",
            Self::Escalated => "escalated",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// Whether an update is working right now.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    /// WKS-FR-VQRD: whether the update rests on something only the author can
    /// answer.
    pub fn waits_on_author(self) -> bool {
        matches!(self, Self::Escalated)
    }

    /// WKS-FR-DPNM: whether a new update of the stream may start from this one.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Conflicted | Self::Cancelled | Self::Failed)
    }
}

/// WKS-FR-UBGX: one commit the stream is missing from its base branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamUpdateCommit {
    /// The full object id.
    pub revision: String,
    /// The commit's first line.
    pub summary: String,
    /// The commit author's name. No address: an address is the author's own
    /// and a listing does not need one to name who wrote a commit.
    pub author: String,
    /// RFC 3339 UTC.
    pub committed_at: String,
}

/// WKS-FR-ZKUP: what one stream's most recent update did.
///
/// **Every scalar stands before every table.** `write_toml_atomic` serialises
/// through `toml::to_string`, which refuses a document whose value follows a
/// table, so a field moved below `escalation` is a runtime failure rather than
/// a compile error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamUpdateRecord {
    pub stream_id: String,
    pub project_key: String,
    pub state: StreamUpdateState,
    /// WKS-FR-AMWE: the strategy the author chose.
    pub strategy: StreamUpdateStrategy,
    /// The attempt the state belongs to. Empty before the first attempt.
    #[serde(default)]
    pub attempt_id: String,
    /// WKS-FR-XDBM: the stream's recorded base branch, which is the source.
    #[serde(default)]
    pub base_branch: String,
    /// WKS-FR-AMWE: the revision the author's confirmation pinned. Every Git
    /// act of this update reads it, and the branch is never resolved again.
    #[serde(default)]
    pub base_revision: String,
    /// How many semantic turns a settled update spent.
    #[serde(default)]
    pub semantic_turns: u32,
    /// The turn's own words, where the state is `escalated`.
    #[serde(default)]
    pub reason: String,
    /// The typed refusal, where the state is `failed`.
    #[serde(default)]
    pub failure: String,
    /// RFC 3339 UTC.
    pub requested_at: String,
    /// RFC 3339 UTC.
    pub updated_at: String,
    // -- tables, last -------------------------------------------------------
    /// WKS-FR-AMWE: the commits the stream was missing at the request.
    #[serde(default)]
    pub missing_commits: Vec<StreamUpdateCommit>,
    /// Set where the state is `escalated`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation: Option<crate::graduation::GraduationEscalation>,
    /// Set where the state is `conflicted`.
    #[serde(default)]
    pub conflicts: Vec<StreamMergeConflict>,
    /// Set where the state is `updated`.
    #[serde(default)]
    pub updated_paths: Vec<String>,
    /// GRB-FR-KMXT: what the author has answered, carried into the turns of the
    /// update that follows.
    #[serde(default)]
    pub decisions: Vec<StreamMergeDecision>,
}

impl StreamUpdateRecord {
    /// WKS-FR-MJEB: the record an update is started with.
    pub(super) fn starting(
        stream: &WorkStream,
        strategy: StreamUpdateStrategy,
        base_revision: &str,
        missing_commits: Vec<StreamUpdateCommit>,
        decisions: Vec<StreamMergeDecision>,
    ) -> Self {
        let now = crate::notes::now_rfc3339();
        Self {
            stream_id: stream.id.clone(),
            project_key: stream.project_key.clone(),
            state: StreamUpdateState::Running,
            strategy,
            attempt_id: String::new(),
            base_branch: stream.base_branch.clone(),
            base_revision: base_revision.to_string(),
            semantic_turns: 0,
            reason: String::new(),
            failure: String::new(),
            requested_at: now.clone(),
            updated_at: now,
            missing_commits,
            escalation: None,
            conflicts: Vec::new(),
            updated_paths: Vec::new(),
            decisions,
        }
    }
}

/// GRB-FR-BQNF: what one update did — the four reports, kept apart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamUpdateOutcome {
    /// The stream branch already holds the pinned revision.
    NothingToUpdate,
    /// The stream branch now holds it.
    Updated {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        /// Project-relative paths the update wrote into the stream.
        #[serde(rename = "updatedPaths")]
        updated_paths: Vec<String>,
        #[serde(rename = "semanticTurns")]
        semantic_turns: u32,
    },
    /// Git could not settle these paths and no turn settled them either.
    /// Nothing was written.
    Conflicted {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        #[serde(rename = "conflictedPaths")]
        conflicted_paths: Vec<String>,
    },
    /// GRB-FR-LADU: the turn could not choose and asked the author.
    Escalated {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        reason: String,
        questions: Vec<crate::graduation::GraduationEscalationQuestion>,
    },
}

/// One stream's update record, or nothing where no update of it has run.
pub(super) fn read_update(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream_id: &str,
) -> Option<StreamUpdateRecord> {
    fs.read_toml(store.update_record(stream_id)).ok()
}

/// Write one stream's update record durably.
pub(super) fn write_update(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    record: &StreamUpdateRecord,
) -> Result<(), String> {
    store::ensure_dir(fs, &store.updates_root())?;
    fs.write_toml_atomic(store.update_record(&record.stream_id), record)
        .map_err(|e| e.to_string())
}

/// WKS-FR-LRAV / WKS-FR-EIBC: remove one stream's update record.
pub(super) fn remove_update(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    stream_id: &str,
) -> Result<(), String> {
    let path = store.update_record(stream_id);
    if !path.exists() {
        return Ok(());
    }
    fs.delete_path(&path, false).map_err(|e| e.to_string())
}

/// Every update record the store holds for one project, in a stable order by id.
pub(super) fn list_updates(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    project_key: &str,
) -> Vec<StreamUpdateRecord> {
    let Ok(entries) = fs.list_dir(store.updates_root()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .iter()
        .filter_map(|e| {
            e.name
                .as_str()
                .strip_suffix(RECORD_SUFFIX)
                .filter(|id| store::is_stream_id(id))
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids.iter()
        .filter_map(|id| read_update(fs, store, id))
        .filter(|record| record.project_key == project_key)
        .collect()
}

/// The one place a finished update becomes a durable record.
pub(super) fn settle(
    record: &mut StreamUpdateRecord,
    conflicts: &[StreamMergeConflict],
    outcome: &Result<StreamUpdateOutcome, String>,
) {
    record.updated_at = crate::notes::now_rfc3339();
    match outcome {
        Ok(StreamUpdateOutcome::NothingToUpdate) => {
            record.state = StreamUpdateState::NothingToUpdate;
            record.decisions.clear();
        }
        Ok(StreamUpdateOutcome::Updated {
            attempt_id,
            updated_paths,
            semantic_turns,
        }) => {
            record.state = StreamUpdateState::Updated;
            record.attempt_id = attempt_id.clone();
            record.updated_paths = updated_paths.clone();
            record.semantic_turns = *semantic_turns;
            record.conflicts.clear();
            record.decisions.clear();
        }
        Ok(StreamUpdateOutcome::Conflicted {
            attempt_id,
            conflicted_paths,
        }) => {
            record.state = StreamUpdateState::Conflicted;
            record.attempt_id = attempt_id.clone();
            record.conflicts = conflicts_for(conflicts, conflicted_paths);
        }
        Ok(StreamUpdateOutcome::Escalated {
            attempt_id,
            reason,
            questions,
        }) => {
            record.state = StreamUpdateState::Escalated;
            record.attempt_id = attempt_id.clone();
            record.reason = reason.clone();
            record.conflicts = conflicts.to_vec();
            record.escalation = Some(crate::graduation::GraduationEscalation {
                reason: reason.clone(),
                questions: questions.clone(),
                origin: crate::graduation::GraduationEscalationOrigin::SemanticMerge,
                raised_at: record.updated_at.clone(),
                resume: None,
            });
        }
        Err(reason) => {
            let failure = typed_failure(reason);
            record.state = if failure == ERR_UPDATE_CANCELLED {
                StreamUpdateState::Cancelled
            } else {
                StreamUpdateState::Failed
            };
            record.failure = failure;
            record.conflicts = conflicts.to_vec();
        }
    }
}

/// The typed refusal an update rested on.
///
/// A typed refusal arrives either alone or as `code: detail`, so the code is
/// what stands before the first colon. A reason that is not one of them keeps
/// its whole text.
fn typed_failure(reason: &str) -> String {
    const TYPED: [&str; 15] = [
        ERR_NOT_A_GIT_REPOSITORY,
        ERR_UNKNOWN_STREAM,
        ERR_STREAM_BUSY,
        ERR_STREAM_DIRTY,
        ERR_BASE_DIRTY,
        crate::storage_floor::save::ERR_DRAFT_SAVE_FAILED,
        ERR_STREAM_MISSING,
        ERR_BASE_NOT_CHECKED_OUT,
        ERR_STALE_BASE_REVISION,
        ERR_ARTIFACT_GENERATION_FAILED,
        ERR_UPDATE_ATTEMPTS_EXHAUSTED,
        ERR_UPDATE_IN_PROGRESS,
        ERR_UPDATE_CANCELLED,
        ERR_UPDATE_INTERRUPTED,
        // WKS-FR-TSOA: a merge holding the repository update guard refuses an
        // update with its own code, which the surface renders as itself.
        ERR_MERGE_IN_PROGRESS,
    ];
    let code = reason.split(':').next().unwrap_or(reason).trim();
    if TYPED.contains(&code) {
        return code.to_string();
    }
    reason.trim().to_string()
}

/// The conflict summaries the last attempt planned over, kept to the paths the
/// outcome names.
fn conflicts_for(planned: &[StreamMergeConflict], paths: &[String]) -> Vec<StreamMergeConflict> {
    paths
        .iter()
        .map(|path| {
            planned
                .iter()
                .find(|c| &c.path == path)
                .cloned()
                .unwrap_or_else(|| StreamMergeConflict {
                    path: path.clone(),
                    base_change: String::new(),
                    stream_change: String::new(),
                })
        })
        .collect()
}

/// WKS-FR-QFTH: the update records this project left `running` with no update
/// behind them.
pub(super) fn interrupted_records(
    fs: &fsa::FsAccess,
    store: &StreamStore,
    project_key: &str,
    is_live: impl Fn(&str) -> bool,
) -> Vec<StreamUpdateRecord> {
    list_updates(fs, store, project_key)
        .into_iter()
        .filter(|record| record.state.is_running() && !is_live(&record.stream_id))
        .map(|mut record| {
            record.state = StreamUpdateState::Failed;
            record.failure = ERR_UPDATE_INTERRUPTED.to_string();
            record.updated_at = crate::notes::now_rfc3339();
            record
        })
        .collect()
}
