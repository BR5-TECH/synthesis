/**
 * The decomposition badges of a Drafts panel row (`DRP-drafts-panel.md`
 * DRP-FR-KAWD, DRP-FR-IAWQ, DRP-FR-AOLJ, DRP-FR-NOXB, DRP-FR-RUVR).
 *
 * Neither badge is a control. Each states its fact in words on hover and to
 * assistive technology, and neither changes the row's order or filter.
 */
import { sequenceLabel, supersededLabel } from "../types";
import type { DraftSummary } from "../types";

/** DRP-FR-IAWQ: the original draft's current name, else the stored name. */
export function originalNameOf(
  draft: DraftSummary,
  draftsByFolder: Map<string, DraftSummary[]>,
): string {
  const sequence = draft.sequence;
  if (!sequence) return "";
  for (const list of draftsByFolder.values()) {
    const found = list.find((d) => d.id === sequence.originalDraftId);
    if (found) return found.name;
  }
  return sequence.originalName;
}

/** DRP-FR-RUVR: the accessible-name suffix of the two facts. */
export function decompositionSuffix(draft: DraftSummary): string {
  const part = draft.sequence
    ? `, part ${draft.sequence.position} of ${draft.sequence.total}`
    : "";
  return `${part}${draft.superseded ? ", superseded" : ""}`;
}

/** DRP-FR-KAWD: the sequence badge, in the style of the publication marker. */
export function SequenceBadge({ draft, originalName }: { draft: DraftSummary; originalName: string }) {
  if (!draft.sequence) return null;
  const label = sequenceLabel(draft.sequence, originalName);
  return (
    <span
      className="drafts-tree__badge drafts-tree__sequence"
      data-testid="draft-sequence-marker"
      title={label}
      aria-label={label}
    >
      Part {draft.sequence.position} of {draft.sequence.total}
    </span>
  );
}

/** DRP-FR-AOLJ / DRP-FR-NOXB: the superseded marker, on every status. */
export function SupersededBadge({ draft }: { draft: DraftSummary }) {
  if (!draft.superseded) return null;
  const label = supersededLabel(draft.superseded);
  return (
    <span
      className="drafts-tree__badge"
      data-testid="draft-superseded-marker"
      title={label}
      aria-label={label}
    >
      Superseded
    </span>
  );
}
