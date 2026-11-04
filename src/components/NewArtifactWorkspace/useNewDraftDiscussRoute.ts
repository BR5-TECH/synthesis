/**
 * DCR-FR-AMLR: while a draft's tab is mounted, the new-draft cards of its
 * discussion reach its Discuss through this route.
 */
import { useEffect } from "react";

import { registerDiscussRoute } from "../../state/decompositionRoutes";
import { newDraftProposalById } from "../../state/newDraftProposals";
import type { NewDraftProposal } from "../../types";

export function useNewDraftDiscussRoute(
  draftId: string,
  discussNewDraft: (proposal: NewDraftProposal) => void,
): void {
  useEffect(
    () =>
      registerDiscussRoute(draftId, (proposalId) => {
        const proposal = newDraftProposalById(proposalId);
        if (proposal) discussNewDraft(proposal);
      }),
    [draftId, discussNewDraft],
  );
}
