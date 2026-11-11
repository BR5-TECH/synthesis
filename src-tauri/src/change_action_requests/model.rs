//! The records, the typed errors, and the store of change action requests
//! (CAR-FR-VQJC, CAR-FR-TJAD, CAR-FR-KYJU).

use super::*;

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_REQUEST_NOT_FOUND: &str = "request_not_found";
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
pub const ERR_ACTION_REQUEST_PENDING: &str = "action_request_pending";
pub const ERR_DISCUSSION_RESOLVED: &str = comments::ERR_DISCUSSION_RESOLVED;
pub const ERR_UNKNOWN_STREAM: &str = crate::change_proposals::ERR_UNKNOWN_STREAM;
pub const ERR_CHANGE_NOT_FOUND: &str = crate::change_proposals::ERR_CHANGE_NOT_FOUND;
pub const ERR_BLANK_INSTRUCTION: &str = "blank_instruction";
pub const ERR_WRITE_FAILED: &str = "write_failed";

/// CAR contract surface `ChangeAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Apply,
    Rework,
}

impl ChangeAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Rework => "rework",
        }
    }

    /// RCA-FR-BYYI: `apply` or `rework`, case ignored.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "apply" => Some(Self::Apply),
            "rework" => Some(Self::Rework),
            _ => None,
        }
    }
}

/// CAR-FR-KYJU: the one state a request stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeActionRequestState {
    Pending,
    Started,
    Declined,
    Failed,
}

impl ChangeActionRequestState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Started => "started",
            Self::Declined => "declined",
            Self::Failed => "failed",
        }
    }
}

/// CAR-FR-TJAD / CAR contract surface `ChangeActionRequest`, also the
/// persisted record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeActionRequest {
    pub id: String,
    pub stream_id: String,
    pub change: String,
    pub action: ChangeAction,
    /// A rework alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    pub rationale: String,
    pub thread_id: String,
    pub comment_id: String,
    pub state: ChangeActionRequestState,
    /// A started request alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// The code of the last refused confirmation, or of the failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refusal: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// Last, because TOML writes a table after every plain value.
    pub agent: Participant,
}

/// CAR contract surface `ChangeActionOutcome`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeActionOutcome {
    pub request: ChangeActionRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<crate::graduation::GraduationRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    pub origin_kind: &'static str,
}

/// CAR-FR-IFGR: one request as a turn reads it, with the state of its run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestForTurn {
    pub request: ChangeActionRequest,
    /// The state of the run where one started and its record reads.
    pub run_state: Option<String>,
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

const RECORD_EXT: &str = "toml";

pub(super) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// CAR-FR-VQJC: `changes/<stream-id>/<change>/action-requests/`.
pub(super) fn requests_dir(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Result<PathBuf, String> {
    if !comments::is_change_owner(stream_id, change) {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    fs::resolve_under(
        store.path(),
        format!(
            "{}/{stream_id}/{change}/action-requests",
            crate::repository_store::CHANGES_DIRNAME
        ),
    )
    .map_err(|e| e.to_string())
}

pub(super) fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{RECORD_EXT}"))
}

pub(super) fn write_record(
    store: &fs::RootFs,
    dir: &Path,
    record: &ChangeActionRequest,
) -> Result<(), String> {
    store
        .write_toml_atomic(record_path(dir, &record.id), record)
        .map_err(|_| ERR_WRITE_FAILED.to_string())
}

/// CAR-FR-ALHA: every request of a change, the most recently created first.
pub fn list_requests_impl(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Vec<ChangeActionRequest> {
    let Ok(dir) = requests_dir(store, stream_id, change) else {
        return Vec::new();
    };
    let Ok(entries) = store.list_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<ChangeActionRequest> = entries
        .into_iter()
        .filter_map(|e| e.name.strip_suffix(&format!(".{RECORD_EXT}")).map(str::to_string))
        .filter(|id| is_valid_id(id))
        .filter_map(|id| store.read_toml::<ChangeActionRequest>(record_path(&dir, &id)).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// The pending request of a change, where one stands (CAR-FR-MNTP).
pub fn pending_request(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Option<ChangeActionRequest> {
    list_requests_impl(store, stream_id, change)
        .into_iter()
        .find(|r| r.state == ChangeActionRequestState::Pending)
}

/// A request by its id alone, found by its record file in the open project's
/// store, with the folder that holds it.
pub(super) fn read_request(
    store: &fs::RootFs,
    request_id: &str,
) -> Result<(ChangeActionRequest, PathBuf), String> {
    if !is_valid_id(request_id) {
        return Err(ERR_REQUEST_NOT_FOUND.to_string());
    }
    let root = fs::resolve_under(store.path(), crate::repository_store::CHANGES_DIRNAME)
        .map_err(|_| ERR_REQUEST_NOT_FOUND.to_string())?;
    for stream in store.list_dir(&root).unwrap_or_default() {
        for change in store.list_dir(root.join(&stream.name)).unwrap_or_default() {
            let Ok(dir) = requests_dir(store, &stream.name, &change.name) else {
                continue;
            };
            if let Ok(record) =
                store.read_toml::<ChangeActionRequest>(record_path(&dir, request_id))
            {
                return Ok((record, dir));
            }
        }
    }
    Err(ERR_REQUEST_NOT_FOUND.to_string())
}

/// CAR-FR-OKOV: one exclusion per change. A recording and a decision of one
/// change never interleave, so two confirmations never start two runs.
pub(super) fn change_lock(stream_id: &str, change: &str) -> std::sync::MutexGuard<'static, ()> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mutex = {
        let mut guard = locks.lock().unwrap_or_else(|e| e.into_inner());
        *guard
            .entry(format!("{stream_id}/{change}"))
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    #[cfg(test)]
    match mutex.try_lock() {
        Ok(guard) => return guard,
        Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => LOCK_WAIT.with(|hook| {
            if let Some(hook) = hook.borrow().as_ref() {
                hook();
            }
        }),
    }
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
thread_local! {
    /// A test sets it on one thread. It runs when that thread finds the
    /// exclusion of its change held and is about to wait for it.
    pub(crate) static LOCK_WAIT: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

/// The thread of a request, among the discussions of its change.
pub(super) fn locate_thread(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
    thread_id: &str,
) -> Option<comments::Discussion> {
    let target = comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: change.to_string(),
    };
    comments::locate_discussion_of(store, &target, thread_id)
}

/// CAR contract surface: the origin kind of a request's thread.
pub(super) fn origin_kind_of(thread: Option<&comments::Discussion>) -> &'static str {
    if thread.is_some_and(comments::Discussion::is_fragment_targeted) {
        "change_comment"
    } else {
        "change_discussion"
    }
}

// ---------------------------------------------------------------------------
// The read of a turn (CAR-FR-IFGR)
// ---------------------------------------------------------------------------

/// How a turn learns the state of a run by its id, where the run's record
/// reads. Installed once at application setup, because a turn builds its
/// input with no application handle.
type RunStateLookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

static RUN_STATE_LOOKUP: std::sync::OnceLock<RunStateLookup> = std::sync::OnceLock::new();

/// The run states a test gives the lookup.
#[cfg(test)]
static TEST_RUN_STATES: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Give the turn builders a way to read the state of a run.
pub fn install_run_state_lookup<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let _ = RUN_STATE_LOOKUP.set(Box::new(move |run_id: &str| {
        let run = crate::graduation::load_run(&app, run_id).ok()?;
        serde_json::to_value(run.state).ok()?.as_str().map(str::to_string)
    }));
}

/// A run state for the lookup of a test.
#[cfg(test)]
pub fn set_test_run_state(run_id: &str, state: &str) {
    let mut states = TEST_RUN_STATES.lock().unwrap_or_else(|e| e.into_inner());
    states.retain(|(id, _)| id != run_id);
    states.push((run_id.to_string(), state.to_string()));
}

fn run_state(run_id: &str) -> Option<String> {
    #[cfg(test)]
    {
        let found = TEST_RUN_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(id, _)| id == run_id)
            .map(|(_, state)| state.clone());
        if found.is_some() {
            return found;
        }
    }
    RUN_STATE_LOOKUP.get().and_then(|lookup| lookup(run_id))
}

/// CAR-FR-IFGR: the requests of a change, the most recently created first,
/// each with the state of its run where one started. It writes nothing.
pub fn read_change_action_requests(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Vec<RequestForTurn> {
    list_requests_impl(store, stream_id, change)
        .into_iter()
        .map(|request| {
            let run_state = request.run_id.as_deref().and_then(run_state);
            RequestForTurn { request, run_state }
        })
        .collect()
}
