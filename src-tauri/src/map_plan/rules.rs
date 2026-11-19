//! The rules that read a proposal: which rows are blocked (MPR-FR-YLKL) and
//! whether a discussion has an open row (MPR-FR-SBYA). Pure.

use super::model::{parent_row_of, MapPlanProposal, MapPlanRow, RowBlock, RowFields, RowState};

/// MPR-FR-YLKL: why the row at `at` cannot be accepted yet. A `create_issue`
/// row, or a `modify_issue` row whose parent change, names a parent row is
/// blocked while that row is `waiting`, `rejected`, or `failed`. A row that is
/// decided is not blocked.
pub fn block_of(rows: &[MapPlanRow], at: usize) -> Option<RowBlock> {
    let row = rows.get(at)?;
    if row.state != RowState::Waiting {
        return None;
    }
    let parent_key = parent_row_of(&row.fields)?;
    let parent = rows.iter().find(|r| r.key == parent_key)?;
    (parent.state != RowState::Accepted)
        .then(|| RowBlock { row_key: parent.key.clone(), reason: parent.state })
}

/// MPR-FR-YLKL: the proposal with `blocked_by` computed for each row.
pub fn with_blocks(mut proposal: MapPlanProposal) -> MapPlanProposal {
    let blocks: Vec<Option<RowBlock>> =
        (0..proposal.rows.len()).map(|at| block_of(&proposal.rows, at)).collect();
    for (row, block) in proposal.rows.iter_mut().zip(blocks) {
        row.blocked_by = block;
    }
    proposal
}

/// MPR-FR-SBYA: whether any row of the proposals is `waiting`.
pub fn has_open_row(proposals: &[MapPlanProposal]) -> bool {
    proposals.iter().flat_map(|p| &p.rows).any(|r| r.state == RowState::Waiting)
}

/// The word for a row kind in a log record.
pub fn kind_name(fields: &RowFields) -> &'static str {
    match fields {
        RowFields::CreateIssue(_) => "create_issue",
        RowFields::ModifyIssue(_) => "modify_issue",
        RowFields::NewDraft(_) => "new_draft",
    }
}
