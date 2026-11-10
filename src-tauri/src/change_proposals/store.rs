//! The store of change edit proposals in the repository machine store
//! (CEP-FR-IISM), and the locks that serialise one change's operations.

use super::*;

pub(super) const RECORD_EXT: &str = "toml";
pub(super) const HUNKS_EXT: &str = "hunks";
pub(super) const JOURNAL_EXT: &str = "journal";
const PROPOSALS_SUBDIR: &str = "proposals";

/// A proposal id is a bare, filename-safe token, because it is joined into a
/// path.
pub fn is_valid_proposal_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// CEP-FR-IISM: `changes/<stream-id>/<change>/proposals/` in the store.
pub(super) fn proposals_dir(
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
            "{}/{stream_id}/{change}/{PROPOSALS_SUBDIR}",
            crate::repository_store::CHANGES_DIRNAME
        ),
    )
    .map_err(|e| e.to_string())
}

pub(super) fn at(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// Every record of one change, most recently created first.
pub(super) fn read_change(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Vec<ChangeEditProposal> {
    let Ok(dir) = proposals_dir(store, stream_id, change) else {
        return Vec::new();
    };
    let Ok(entries) = store.list_dir(&dir) else {
        // No folder is no proposals: the first recording creates it.
        return Vec::new();
    };
    let mut out: Vec<ChangeEditProposal> = entries
        .into_iter()
        .filter_map(|entry| entry.name.strip_suffix(&format!(".{RECORD_EXT}")).map(str::to_string))
        .filter(|id| is_valid_proposal_id(id))
        // A record that does not read is skipped: one damaged proposal never
        // costs the author the rest.
        .filter_map(|id| store.read_toml::<ChangeEditProposal>(at(&dir, &id, RECORD_EXT)).ok())
        .collect();
    // CEP-FR-HUFE: most recently created first. Two records of one second are
    // ordered by id, which grows with time, so the newer still comes first.
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// CEP-FR-HUFE: every proposal of the change, most recently created first.
pub fn list_proposals_impl(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Vec<ChangeEditProposal> {
    read_change(store, stream_id, change)
}

/// CEP-FR-ZOIG: the change's pending proposal, if it has one.
pub(super) fn pending_for(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Option<ChangeEditProposal> {
    read_change(store, stream_id, change)
        .into_iter()
        .find(|p| p.state == ChangeProposalState::Pending)
}

/// The change a proposal id belongs to, found by its record file.
///
/// Proposal ids are unique in the store, and each change folder is small, so
/// the walk is one listing per change that holds proposals.
pub(super) fn owner_of(store: &fs::RootFs, proposal_id: &str) -> Option<(String, String)> {
    if !is_valid_proposal_id(proposal_id) {
        return None;
    }
    let root = fs::resolve_under(store.path(), crate::repository_store::CHANGES_DIRNAME).ok()?;
    let streams = store.list_dir(&root).ok()?;
    for stream in streams {
        let Ok(changes) = store.list_dir(root.join(&stream.name)) else {
            continue;
        };
        for change in changes {
            let Ok(dir) = proposals_dir(store, &stream.name, &change.name) else {
                continue;
            };
            if store.file_info(at(&dir, proposal_id, RECORD_EXT)).is_ok() {
                return Some((stream.name.clone(), change.name));
            }
        }
    }
    None
}

/// One proposal's record by id, with the folder it lives in.
pub(super) fn read_proposal(
    store: &fs::RootFs,
    proposal_id: &str,
) -> Result<(ChangeEditProposal, PathBuf), String> {
    let (stream_id, change) =
        owner_of(store, proposal_id).ok_or_else(|| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    let dir = proposals_dir(store, &stream_id, &change)?;
    let record = store
        .read_toml::<ChangeEditProposal>(at(&dir, proposal_id, RECORD_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    Ok((record, dir))
}

/// The hunk set of a proposal.
pub(super) fn read_hunks(
    store: &fs::RootFs,
    dir: &Path,
    proposal_id: &str,
) -> Result<HunkSet, String> {
    let text = store
        .read_text(at(dir, proposal_id, HUNKS_EXT))
        .map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())?;
    HunkSet::from_toml(&text).map_err(|_| ERR_PROPOSAL_NOT_FOUND.to_string())
}

pub(super) fn write_record(
    store: &fs::RootFs,
    dir: &Path,
    record: &ChangeEditProposal,
) -> Result<(), String> {
    store.write_toml_atomic(at(dir, &record.id, RECORD_EXT), record).map_err(|e| e.to_string())
}

/// CEP-FR-ZOIG / PCP-FR-14: serialises every operation that records, decides,
/// or reconciles one change's proposals, for the reasons
/// `prompt_proposals::artifact_lock` gives.
pub(super) fn change_lock(stream_id: &str, change: &str) -> std::sync::MutexGuard<'static, ()> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let key = format!("{stream_id}/{change}");
    let mutex = {
        let mut guard = locks.lock().unwrap_or_else(|e| e.into_inner());
        *guard.entry(key).or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn in_flight() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

/// PCP-FR-14: one acceptance of a proposal at a time, refused rather than
/// queued.
pub(super) fn try_acceptance_slot(proposal_id: &str) -> Option<AcceptanceSlot> {
    let mut guard = in_flight().lock().unwrap_or_else(|e| e.into_inner());
    if !guard.insert(proposal_id.to_string()) {
        return None;
    }
    Some(AcceptanceSlot { proposal_id: proposal_id.to_string() })
}

pub(super) fn is_accepting(proposal_id: &str) -> bool {
    in_flight().lock().unwrap_or_else(|e| e.into_inner()).contains(proposal_id)
}

pub(super) struct AcceptanceSlot {
    proposal_id: String,
}

impl Drop for AcceptanceSlot {
    fn drop(&mut self) {
        in_flight().lock().unwrap_or_else(|e| e.into_inner()).remove(&self.proposal_id);
    }
}

/// The conversation of a proposal: the thread, found among the change's
/// discussions.
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
        ORIGIN_CHANGE_COMMENT
    } else {
        ORIGIN_CHANGE_DISCUSSION
    }
}
