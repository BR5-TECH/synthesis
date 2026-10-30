//! The hunks a proposal records, built from the form the caller gave
//! (PCP-FR-CNVT, PCP-FR-CUTC).
//!
//! A change comes as the complete new text, or as anchored hunks. Either way
//! the result is one hunk document placed against the artifact as it is on
//! disk, and the author decides each hunk of it separately: a `content` change
//! is converted into hunks here, at record time.

use super::*;
use crate::text_hunks::hunks::{self as text, HunkDocument, PlanError, ProposedHunk};
use crate::text_hunks::line_diff::line_changes;

/// What a caller proposes (PCP contract surface, `ProposedChange`).
#[derive(Debug, Clone, Copy)]
pub enum ProposedChange<'a> {
    /// The complete new text, converted into hunks at record time.
    Content(&'a str),
    /// Anchored hunks, resolved against the file as it is on disk.
    Hunks(&'a [ProposedHunk]),
}

/// The hunk document for `change`, or the refusal that records nothing.
///
/// PCP-FR-CNVT: hunks are resolved against `current` by the rule of PPC-FR-RJCA,
/// and every refusal names the hunk as the caller counted it (PCP-FR-CUTC). A
/// `content` change becomes one hunk for each run of changed lines
/// (`line_changes`). Where those hunks cannot all be placed once, or where the
/// texts do not differ, it becomes the one hunk of `whole_document`, which a
/// `content` call cannot get wrong: it names the whole file, and an unchanged
/// text then meets the `no_change` refusal rather than `no_hunks`.
pub(super) fn plan_change(
    current: &str,
    change: ProposedChange<'_>,
) -> Result<HunkDocument, RecordRefusal> {
    let converted;
    let hunks = match change {
        ProposedChange::Content(content) => {
            let changes = line_changes(current, content);
            if !changes.is_empty() {
                if let Ok(document) = text::plan(current, &changes, new_note_id) {
                    return Ok(document);
                }
            }
            converted = text::whole_document(current, content);
            converted.as_slice()
        }
        ProposedChange::Hunks(hunks) => hunks,
    };
    let of = hunks.len();
    text::plan(current, hunks, new_note_id).map_err(|error| match error {
        PlanError::NoHunks => RecordRefusal::NoHunks,
        PlanError::AnchorLost(at) => RecordRefusal::HunkAnchorLost { at, of },
        PlanError::Ambiguous(at) => RecordRefusal::HunkAmbiguous { at, of },
        PlanError::Overlap(at) => RecordRefusal::HunkOverlap { at, of },
        PlanError::ContextTooLong(at, side) => RecordRefusal::HunkContextTooLong { at, of, side },
    })
}

/// PCP-FR-07: the text the document gives once every hunk is accepted, as the
/// write path would persist it (PST-FR-22). This is what the `no_change`
/// comparison is taken over.
///
/// `plan` placed every hunk a moment ago against this same text, so an
/// application that fails here is a fault of this module and not a mistake in
/// the arguments. It carries no place, because no hunk is at fault.
pub(super) fn composed(
    root: &fs::RootFs,
    current: &str,
    document: &HunkDocument,
) -> Result<String, RecordRefusal> {
    let built = text::apply_all(current, document).ok_or(RecordRefusal::NotRecorded)?;
    Ok(normalise_for_write(root, &built))
}
