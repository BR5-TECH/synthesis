/**
 * OCW-FR-BGDN: a `new_change_proposal` reference outside the discussion
 * stream of its change.
 *
 * It names `Part N of M` and the proposed name, and activating it opens the
 * tab of the original change on its stream, where the card stands.
 */
import { useEffect } from "react";

import { openOpenSpecChange } from "../state/openspec/changeRoutes";
import {
  ensureNewChangesLoaded,
  newChangeProposalById,
  useNewChangeStore,
} from "../state/openspec/newChangeProposals";
import type { NewChangeProposalReference } from "../types";
import { Icon } from "./icons";

export function NewChangeReferenceControl({
  attachment,
}: {
  attachment: NewChangeProposalReference;
}) {
  useNewChangeStore();
  useEffect(
    () => ensureNewChangesLoaded(attachment.streamId, attachment.change),
    [attachment.streamId, attachment.change],
  );
  const proposal = newChangeProposalById(attachment.proposalId);
  const name = proposal
    ? `Part ${proposal.position} of ${proposal.total} · ${proposal.name}`
    : "Proposed change";
  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-new-change"
      title={`${name} — show it in the change ${attachment.change}`}
      onClick={() =>
        openOpenSpecChange({ kind: "stream", streamId: attachment.streamId }, attachment.change)
      }
    >
      <Icon.Doc size={11} />
      <span className="comment-attach__chip-name">{name}</span>
    </button>
  );
}
