/**
 * CMT-FR-MPKL: a `new_draft_proposal` reference outside the discussion stream.
 *
 * It names `Part N of M` and the proposed name, and activating it opens or
 * focuses the original draft's tab and shows that proposal's card.
 */
import { useEffect } from "react";

import { showNewDraftCard } from "../state/decompositionRoutes";
import { ensureNewDraftsLoaded, newDraftProposalById, useNewDraftStore } from "../state/newDraftProposals";
import type { NewDraftProposalReference } from "../types";
import { Icon } from "./icons";

export function NewDraftReferenceControl({ attachment }: { attachment: NewDraftProposalReference }) {
  useNewDraftStore();
  useEffect(() => ensureNewDraftsLoaded(attachment.draftId), [attachment.draftId]);
  const proposal = newDraftProposalById(attachment.proposalId);
  const name = proposal
    ? `Part ${proposal.position} of ${proposal.total} · ${proposal.name}`
    : "Proposed draft";
  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-new-draft"
      title={`${name} — show it in the draft`}
      onClick={() => showNewDraftCard(attachment.draftId, attachment.proposalId)}
    >
      <Icon.Doc size={11} />
      <span className="comment-attach__chip-name">{name}</span>
    </button>
  );
}
