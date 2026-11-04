/**
 * The shell's half of the decomposition surfaces (`DCR-draft-change-review.md`
 * DCR-FR-QQLE, DCR-FR-HWLA, DCR-FR-RJZP; `DRP-drafts-panel.md` DRP-FR-TCRW;
 * `CMT-comments.md` CMT-FR-MPKL).
 *
 * It subscribes to `"new draft proposals changed"` once, at the shell, and it
 * wires the routes that open a draft's tab and the draft preview tab.
 */
import { useEffect, type MutableRefObject } from "react";

import { onNewDraftProposalsChanged } from "../events";
import { closeDraftPreview, configureDecompositionRoutes } from "../state/decompositionRoutes";
import { noteNewDraftChanged } from "../state/newDraftProposals";
import type { Raise } from "../state/notifications";
import { newDraftProposalRaise, type OpenRoot } from "../state/proposalRaises";
import type { DraftSummary } from "../types";
import type { useShellSession } from "./useShellSession";

export function useDecompositionShell(
  s: ReturnType<typeof useShellSession>,
  knownDrafts: DraftSummary[],
  rootRef: MutableRefObject<OpenRoot>,
  /** NTF-FR-24: the raise is made by `useAppNotifications` alone. */
  raise: (raise: Raise) => void,
): void {
  // DCR-FR-QQLE / DRP-FR-TCRW: the store follows the event, the Drafts panel
  // re-lists, and a recording or an update that leaves the proposal pending
  // raises. Subscribed once, so no event is lost to a re-subscription.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onNewDraftProposalsChanged((payload) => {
      noteNewDraftChanged(payload);
      // DCR-FR-RJZP / TAB-FR-OMWY: the preview closes with its proposal.
      if (payload.proposal && payload.proposal.state !== "pending") {
        closeDraftPreview(payload.proposal.id);
      }
      s.bumpDrafts();
      const next = newDraftProposalRaise(payload, rootRef.current);
      if (next !== null) raise(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // DCR-FR-HWLA / CMT-FR-MPKL: the routes the cards and controls take.
  useEffect(() => {
    configureDecompositionRoutes({
      openDraft: (draftId) => {
        const draft = knownDrafts.find((d) => d.id === draftId);
        if (draft) s.openDraft({ id: draft.id, name: draft.name });
      },
      draftOf: (draftId) => knownDrafts.find((d) => d.id === draftId),
      openPreview: s.openDraftPreview,
      closePreview: s.closeDraftPreview,
    });
    return () => configureDecompositionRoutes(null);
  }, [knownDrafts, s]);
}
