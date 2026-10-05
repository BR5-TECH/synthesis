/**
 * The marker a row carries when the thing it points at is gone.
 *
 * One state, one reading: a note whose entity no longer resolves
 * (`NTS-notes.md` NTS-FR-23) and a comment thread whose artifact no longer
 * resolves (`CMP-comments-panel.md` CMP-FR-12) are the same fact about the
 * project, and both specs require the same word and the same treatment for it.
 * A shared component rather than two string literals, because two literals
 * drift the moment one panel is edited and nothing anywhere notices.
 *
 * It names the ENTITY, never the row's own status. In the Comments panel that
 * distinction is load-bearing: a thread can be Unresolved (its artifact is
 * gone) while sitting under **Resolved** (its conversation is settled), and
 * CMP-FR-12 keeps the two independent.
 */
export const UNRESOLVED_MARKER_LABEL = "Unresolved";

export function UnresolvedMarker() {
  return <span className="unresolved-marker">{UNRESOLVED_MARKER_LABEL}</span>;
}
