//! The files of the plan proposals (MPR-FR-RVRN): one JSON file for each
//! proposal in `comments/map/proposals/` of the comments store. A proposal of
//! 20 rows is one file write.

use crate::fs::RootFs;

use super::errors::store_unavailable;
use super::model::{MapPlanError, MapPlanProposal};

const EXTENSION: &str = "json";

/// A proposal id is joined into a path, so it is a bare token.
pub fn is_valid_proposal_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn dir_of(store: &RootFs) -> Result<std::path::PathBuf, MapPlanError> {
    crate::comments::map_proposals_dir(store).map_err(|_| store_unavailable())
}

fn path_of(store: &RootFs, id: &str) -> Result<std::path::PathBuf, MapPlanError> {
    if !is_valid_proposal_id(id) {
        return Err(store_unavailable());
    }
    Ok(dir_of(store)?.join(format!("{id}.{EXTENSION}")))
}

/// MPR-FR-YLKL: `blocked_by` is computed at each read and never stored.
pub fn write(store: &RootFs, proposal: &MapPlanProposal) -> Result<(), MapPlanError> {
    let mut stored = proposal.clone();
    for row in &mut stored.rows {
        row.blocked_by = None;
    }
    let text = serde_json::to_string(&stored).map_err(|_| store_unavailable())?;
    store.write_text_atomic(path_of(store, &stored.id)?, &text).map_err(|_| store_unavailable())
}

/// Take back a record that was written, so a failed step leaves nothing.
pub fn remove(store: &RootFs, id: &str) {
    if let (Ok(dir), true) = (dir_of(store), is_valid_proposal_id(id)) {
        let _ = store.delete_under(dir, format!("{id}.{EXTENSION}"), false);
    }
}

fn read_file(store: &RootFs, id: &str) -> Option<MapPlanProposal> {
    let text = store.read_text(path_of(store, id).ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// One proposal by id.
pub fn find(store: &RootFs, id: &str) -> Option<MapPlanProposal> {
    if !is_valid_proposal_id(id) {
        return None;
    }
    read_file(store, id)
}

/// Every proposal of the discussion, oldest first. A file that does not read is
/// skipped: one damaged proposal never costs the author the rest.
pub fn read_for(store: &RootFs, discussion_id: &str) -> Vec<MapPlanProposal> {
    let Ok(dir) = dir_of(store) else { return Vec::new() };
    // No folder is no proposals: the first recording creates it.
    let Ok(entries) = store.list_dir(&dir) else { return Vec::new() };
    let suffix = format!(".{EXTENSION}");
    let mut found: Vec<MapPlanProposal> = entries
        .into_iter()
        .filter_map(|entry| entry.name.strip_suffix(&suffix).map(str::to_string))
        .filter(|id| is_valid_proposal_id(id))
        .filter_map(|id| read_file(store, &id))
        .filter(|p| p.discussion_id == discussion_id)
        .collect();
    found.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    found
}
