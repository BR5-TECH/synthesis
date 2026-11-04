/**
 * NAW-FR-MONI / NAW-FR-IDIJ: the decomposition facts in the tab's chrome row,
 * beside the archived marker. Neither tag is a control.
 */
import { draftSummaryOf } from "../../state/decompositionRoutes";
import { sequenceLabel, supersededLabel } from "../../types";
import type { DraftSequence, SupersededMarker } from "../../types";

export function DecompositionTags({
  sequence,
  superseded,
}: {
  sequence: DraftSequence | null;
  superseded: SupersededMarker | null;
}) {
  const original = sequence
    ? (draftSummaryOf(sequence.originalDraftId)?.name ?? sequence.originalName)
    : "";
  return (
    <>
      {sequence && (
        <span
          className="draft-workspace__archived t-ui-xs"
          data-testid="draft-workspace-sequence"
          title={sequenceLabel(sequence, original)}
          aria-label={sequenceLabel(sequence, original)}
        >
          Part {sequence.position} of {sequence.total}
        </span>
      )}
      {superseded && (
        <span
          className="draft-workspace__archived t-ui-xs"
          data-testid="draft-workspace-superseded"
          title={supersededLabel(superseded)}
          aria-label={supersededLabel(superseded)}
        >
          Superseded
        </span>
      )}
    </>
  );
}
