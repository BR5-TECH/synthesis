//! The records, the typed errors, and the store of new change proposals
//! (NCP contract surface, NCP-FR-AZON).

use super::*;

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_PROPOSAL_NOT_FOUND: &str = "proposal_not_found";
pub const ERR_ALREADY_DECIDED: &str = "already_decided";
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
pub const ERR_CHANGE_NOT_FOUND: &str = "change_not_found";
pub const ERR_STREAM_BUSY: &str = "stream_busy";
pub const ERR_CHANGE_EXISTS: &str = "openspec_change_exists";
pub const ERR_INVALID_CHANGE_NAME: &str = "invalid_change_name";
pub const ERR_MISSING_PROPOSAL_FILE: &str = "missing_proposal_file";
pub const ERR_PATH_OUTSIDE_CHANGE: &str = "path_outside_change";
pub const ERR_PROPOSAL_PENDING: &str = "new_change_proposal_pending";
pub const ERR_NOTHING_ACCEPTED: &str = "nothing_accepted";
pub const ERR_SPLIT_CONFIRMED: &str = "split_confirmed";
pub const ERR_DISCUSSION_RESOLVED: &str = "discussion_resolved";
pub const ERR_WRITE_FAILED: &str = "write_failed";
pub const ERR_NO_OPEN_PROPOSAL: &str = "no_open_proposal";
pub const ERR_NO_CHANGE: &str = "no_change";
pub const ERR_BLANK_NAME: &str = "blank_name";
pub const ERR_BLANK_FILE: &str = "blank_file";
pub const ERR_DUPLICATE_PATH: &str = "duplicate_path";
pub const ERR_BAD_POSITION: &str = "bad_position";

/// The proposed file of a new change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedFile {
    /// Change-relative path (CEP-FR-WJKY).
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) struct FilesDocument {
    #[serde(rename = "file", default)]
    pub(super) files: Vec<ProposedFile>,
}

/// NCP contract surface `NewChangeProposalState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewChangeProposalState {
    Pending,
    Accepted,
    Rejected,
    Withdrawn,
}

impl NewChangeProposalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
        }
    }
}

/// NCP contract surface `NewChangeProposal`, also the persisted record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChangeProposal {
    pub id: String,
    pub set_id: String,
    pub stream_id: String,
    pub change: String,
    pub name: String,
    pub paths: Vec<String>,
    pub position: u32,
    pub total: u32,
    pub rationale: String,
    pub thread_id: String,
    pub comment_id: String,
    pub state: NewChangeProposalState,
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_change: Option<String>,
    #[serde(default)]
    pub commit_owed: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    pub agent: Participant,
}

/// NCP contract surface `ChangeSplit`, also `split.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSplit {
    pub set_id: String,
    pub accepted: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_at: Option<String>,
}

/// NCP contract surface `NewChangeDecisionOutcome`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChangeDecisionOutcome {
    pub proposal: NewChangeProposal,
    pub committed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    pub origin_kind: &'static str,
}

/// What `"list new change proposals"` returns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChangeProposals {
    pub proposals: Vec<NewChangeProposal>,
    pub split: Option<ChangeSplit>,
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

pub(super) const RECORD_EXT: &str = "toml";
pub(super) const FILES_EXT: &str = "files";

pub(super) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The change's own folder in the store.
pub(super) fn change_root(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Result<PathBuf, String> {
    if !comments::is_change_owner(stream_id, change) {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    fs::resolve_under(
        store.path(),
        format!("{}/{stream_id}/{change}", crate::repository_store::CHANGES_DIRNAME),
    )
    .map_err(|e| e.to_string())
}

/// NCP-FR-AZON: `changes/<stream-id>/<change>/new-changes/`.
pub(super) fn proposals_dir(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Result<PathBuf, String> {
    Ok(change_root(store, stream_id, change)?.join("new-changes"))
}

pub(super) fn at(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// Every proposal of a change, most recently created first.
pub fn list_proposals_impl(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Vec<NewChangeProposal> {
    let Ok(dir) = proposals_dir(store, stream_id, change) else {
        return Vec::new();
    };
    let Ok(entries) = store.list_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<NewChangeProposal> = entries
        .into_iter()
        .filter_map(|e| e.name.strip_suffix(&format!(".{RECORD_EXT}")).map(str::to_string))
        .filter(|id| is_valid_id(id))
        .filter_map(|id| store.read_toml::<NewChangeProposal>(at(&dir, &id, RECORD_EXT)).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// The split of a change, where one was confirmed.
pub fn read_split(store: &fs::RootFs, stream_id: &str, change: &str) -> Option<ChangeSplit> {
    let root = change_root(store, stream_id, change).ok()?;
    store.read_toml::<ChangeSplit>(root.join("split.toml")).ok()
}

/// The change a proposal id belongs to, found by its record file.
pub(super) fn read_proposal(
    store: &fs::RootFs,
    proposal_id: &str,
) -> Result<(NewChangeProposal, PathBuf), String> {
    if !is_valid_id(proposal_id) {
        return Err(ERR_PROPOSAL_NOT_FOUND.to_string());
    }
    let root = fs::resolve_under(store.path(), crate::repository_store::CHANGES_DIRNAME)
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    for stream in store.list_dir(&root).unwrap_or_default() {
        for change in store.list_dir(root.join(&stream.name)).unwrap_or_default() {
            let Ok(dir) = proposals_dir(store, &stream.name, &change.name) else {
                continue;
            };
            if let Ok(record) =
                store.read_toml::<NewChangeProposal>(at(&dir, proposal_id, RECORD_EXT))
            {
                return Ok((record, dir));
            }
        }
    }
    Err(ERR_PROPOSAL_NOT_FOUND.to_string())
}

pub(super) fn read_files(
    store: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
) -> Result<Vec<ProposedFile>, String> {
    store
        .read_toml::<FilesDocument>(at(dir, proposal_id, FILES_EXT))
        .map(|doc| doc.files)
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())
}

pub(super) fn write_record(
    store: &fs::RootFs,
    dir: &Path,
    record: &NewChangeProposal,
) -> Result<(), String> {
    store.write_toml_atomic(at(dir, &record.id, RECORD_EXT), record).map_err(|e| e.to_string())
}

/// One lock per change: a recording, an update, a decision, and a
/// confirmation of one change never interleave.
pub(super) fn split_lock(stream_id: &str, change: &str) -> std::sync::MutexGuard<'static, ()> {
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
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The thread a proposal belongs to, among the change's discussions.
pub(super) fn locate_thread(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
    thread_id: &str,
) -> Result<comments::Discussion, String> {
    let target = comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: change.to_string(),
    };
    comments::locate_discussion_of(store, &target, thread_id)
        .ok_or_else(|| comments::ERR_DISCUSSION_NOT_FOUND.to_string())
}

pub(super) fn origin_kind_of(thread: &comments::Discussion) -> &'static str {
    if thread.is_fragment_targeted() {
        "change_comment"
    } else {
        "change_discussion"
    }
}

/// NCP-FR-MTCW / NCP-FR-RPGN / PNC-FR-RLNW: the files of a proposal are
/// well formed: each path stays inside the change, no path is named twice,
/// no text is blank, and `proposal.md` is among them.
pub fn check_files(files: &[ProposedFile]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for file in files {
        if !comments::is_change_relative_path(&file.path) {
            return Err(ERR_PATH_OUTSIDE_CHANGE.to_string());
        }
        if seen.contains(&file.path.as_str()) {
            return Err(ERR_DUPLICATE_PATH.to_string());
        }
        if file.text.trim().is_empty() {
            return Err(ERR_BLANK_FILE.to_string());
        }
        seen.push(&file.path);
    }
    if !seen.contains(&"proposal.md") {
        return Err(ERR_MISSING_PROPOSAL_FILE.to_string());
    }
    Ok(())
}

/// Whether a change folder of `name` stands in the site's working copy.
pub(super) fn folder_exists(site: &ChangeSite, name: &str) -> bool {
    fs::resolve_under(
        site.working_copy.path(),
        format!("{}/{name}", crate::change_proposals::CHANGES_REL),
    )
    .ok()
    .is_some_and(|path| site.working_copy.file_info(path).is_ok())
}
