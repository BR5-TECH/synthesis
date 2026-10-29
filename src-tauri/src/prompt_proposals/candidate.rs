//! The whole candidate a proposal records, built from the form the caller gave
//! (PCP-FR-CVNA, PCP-FR-CUTC, PCP-FR-ENRL).
//!
//! A change comes as the complete new text, or as anchored hunks that this
//! module resolves against the artifact as it is on disk. Either way the result
//! is one complete document, and nothing after this point can tell which form
//! it came in: the record, the `.content` file, the comment, and the comparison
//! are the same (PCP-FR-ENRL).

use super::*;
use crate::text_hunks::hunks::{self as text, PlanError, ProposedHunk};

/// What a caller proposes (PCP contract surface, `ProposedChange`).
#[derive(Debug, Clone, Copy)]
pub enum ProposedChange<'a> {
    /// The complete new text.
    Content(&'a str),
    /// Anchored hunks, resolved against the file as it is on disk.
    Hunks(&'a [ProposedHunk]),
}

/// The complete candidate for `change`, or the refusal that records nothing.
///
/// PCP-FR-CVNA: hunks are resolved against `current` by the rule of PPC-FR-RJCA
/// and applied in one pass, before any check that reads the candidate and any
/// write. A built candidate takes the project's line-ending convention, which
/// is what the write path would give it on acceptance (PST-FR-22); a `content`
/// candidate is kept exactly as the caller gave it (PCP-FR-09).
pub(super) fn build_candidate(
    root: &fs::RootFs,
    current: &str,
    change: ProposedChange<'_>,
) -> Result<String, RecordRefusal> {
    let hunks = match change {
        ProposedChange::Content(content) => return Ok(content.to_string()),
        ProposedChange::Hunks(hunks) => hunks,
    };
    // PCP-FR-CUTC: each refusal names the hunk as the caller counted it.
    let of = hunks.len();
    let mut minted = 0usize;
    let document = text::plan(current, hunks, || {
        minted += 1;
        format!("h{minted}")
    })
    .map_err(|error| match error {
        PlanError::NoHunks => RecordRefusal::NoHunks,
        PlanError::AnchorLost(at) => RecordRefusal::HunkAnchorLost { at, of },
        PlanError::Ambiguous(at) => RecordRefusal::HunkAmbiguous { at, of },
        PlanError::Overlap(at) => RecordRefusal::HunkOverlap { at, of },
        PlanError::ContextTooLong(at) => RecordRefusal::HunkContextTooLong { at, of },
    })?;
    // `plan` placed every hunk a moment ago against this same text, so an
    // application that fails here is a fault of this module and not a mistake
    // in the arguments. It carries no place, because no hunk is at fault.
    let built = text::apply_all(current, &document).ok_or(RecordRefusal::NotRecorded)?;
    Ok(normalise_for_write(root, &built))
}
