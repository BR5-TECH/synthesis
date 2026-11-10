//! The reads of a review and of a turn (CEP-FR-HUFE, CEP-FR-XXMW). They write
//! nothing.

use super::*;
use crate::prompt_proposals::{HunkCounts, HunkState};
use crate::text_hunks::anchors::{self, HunkKind};

/// CEP-FR-HUFE: the hunks of a proposal in proposal order, each with its path,
/// its state, and its placement in its file as the file stands now.
///
/// A file that cannot be read places every hunk of it as lost, except a new
/// file's hunk, which is placed at the start of the empty text.
pub fn load_hunks_impl(
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    proposal_id: &str,
) -> Result<ChangeProposalHunks, String> {
    let (record, dir) = read_proposal(store, proposal_id)?;
    let set = read_hunks(store, &dir, proposal_id)?;
    let site = sites.site(&record.stream_id, &record.change).ok();
    let mut hunks = Vec::new();
    let mut resolutions = Vec::new();
    for file in &set.files {
        let current =
            if file.new_file && !matches!(record.row_state_of(file), Some(HunkState::Accepted)) {
                Some(String::new())
            } else {
                site.as_ref().and_then(|s| s.read_file(&file.path))
            };
        let placed: Vec<HunkPlacement> = match &current {
            Some(text) => file
                .document
                .resolve_all(text)
                .into_iter()
                .map(|r| match r {
                    anchors::Resolution::Resolved { start, end } => {
                        HunkPlacement::Resolved { start, end }
                    }
                    anchors::Resolution::Lost => HunkPlacement::Lost,
                })
                .collect(),
            None => file.document.hunks.iter().map(|_| HunkPlacement::Lost).collect(),
        };
        for (hunk, placement) in file.document.hunks.iter().zip(placed) {
            let state = record.row(&hunk.id).map(|r| r.state).unwrap_or(HunkState::Pending);
            hunks.push(ChangeEditHunk {
                hunk: hunk.clone(),
                path: file.path.clone(),
                new_file: file.new_file,
                state,
            });
            resolutions.push(placement);
        }
    }
    Ok(ChangeProposalHunks { hunks, resolutions, checksum: set.checksum() })
}

impl ChangeEditProposal {
    /// The state of a new file's one hunk.
    fn row_state_of(&self, file: &HunkSetFile) -> Option<HunkState> {
        file.document.hunks.first().and_then(|h| self.row(&h.id)).map(|r| r.state)
    }
}

/// One hunk of a change edit proposal, as a turn assembling its input reads
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeProposalChange {
    pub id: String,
    /// Change-relative path.
    pub path: String,
    pub new_file: bool,
    pub kind: HunkKind,
    pub state: HunkState,
    pub before: String,
    pub after: String,
}

/// CEP-FR-XXMW: one proposal's change, counts, and hunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeProposalChanges {
    pub stream_id: String,
    pub change: String,
    pub counts: HunkCounts,
    pub changes: Vec<ChangeProposalChange>,
}

/// CEP-FR-XXMW: read one proposal's hunks in proposal order, each with its
/// path and state. It reconciles nothing and writes nothing. Any failure is
/// `None`.
pub fn read_change_proposal_changes(
    store: &fs::RootFs,
    proposal_id: &str,
) -> Option<ChangeProposalChanges> {
    let (record, dir) = read_proposal(store, proposal_id).ok()?;
    let set = read_hunks(store, &dir, proposal_id).ok()?;
    let changes = set
        .hunks()
        .map(|(file, hunk)| ChangeProposalChange {
            id: hunk.id.clone(),
            path: file.path.clone(),
            new_file: file.new_file,
            kind: hunk.kind,
            state: record.row(&hunk.id).map(|r| r.state).unwrap_or(HunkState::Pending),
            before: hunk.before_text().to_string(),
            after: hunk.after_text().to_string(),
        })
        .collect();
    Some(ChangeProposalChanges {
        stream_id: record.stream_id,
        change: record.change,
        counts: record.counts,
        changes,
    })
}

/// One hunk of one proposal, for a hunk discussion's turn.
pub fn read_change_proposal_change(
    store: &fs::RootFs,
    proposal_id: &str,
    hunk_id: &str,
) -> Option<ChangeProposalChange> {
    read_change_proposal_changes(store, proposal_id)?
        .changes
        .into_iter()
        .find(|change| change.id == hunk_id)
}

/// The pending proposal of a change, if it has one, for a turn's input.
pub fn pending_change_proposal(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Option<ChangeEditProposal> {
    pending_for(store, stream_id, change)
}
