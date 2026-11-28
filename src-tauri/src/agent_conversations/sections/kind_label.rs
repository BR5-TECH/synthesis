//! The word a rendering names a change's kind by (`AGC-agent-conversations.md`
//! AGC-FR-RVQP, AGC-FR-DKHT).

use crate::draft_proposals::anchors::HunkKind;

/// The kind word of an edit. A diagram hunk is named as a diagram edit,
/// addition, or removal, and its text is its source (AGC-FR-DKHT).
pub(in crate::agent_conversations) fn kind_label(kind: HunkKind, diagram: bool) -> &'static str {
    match (kind, diagram) {
        (HunkKind::Replace, false) => "replace",
        (HunkKind::Add, false) => "add",
        (HunkKind::Del, false) => "del",
        (HunkKind::Replace, true) => "diagram edit",
        (HunkKind::Add, true) => "diagram addition",
        (HunkKind::Del, true) => "diagram removal",
    }
}
