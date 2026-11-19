/**
 * MPV-FR-UWQL: the plan proposal comments that arrive in the thread that the
 * Discussion column shows, or that a discussion surface of another tab shows
 * (`../MapPlanProposal/inPlaceArrivals.ts`). Each one asks the review store to
 * open its sheet. The comments that the thread held when the column showed it
 * open nothing.
 */
import { useEffect, useRef } from "react";

import { logDebug } from "../../logging";
import { requestPendingFocus } from "../../state/discussionFocus";
import type { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import type { Discussion } from "../../types";

export function useProposalArrivals(
  thread: Discussion | undefined,
  /** Null follows nothing, as no thread does. */
  review: MapPlanReviewStore | null,
  onOpen: (() => void) | undefined,
): void {
  const seen = useRef<{ threadId: string | null; comments: Set<string> }>({ threadId: null, comments: new Set() });
  const openRef = useRef(onOpen);
  openRef.current = onOpen;
  useEffect(() => {
    if (!thread || !review) {
      seen.current = { threadId: null, comments: new Set() };
      return;
    }
    const held = seen.current;
    if (held.threadId !== thread.id) {
      seen.current = { threadId: thread.id, comments: new Set(thread.comments.map((c) => c.id)) };
      return;
    }
    for (const comment of thread.comments) {
      if (held.comments.has(comment.id)) continue;
      held.comments.add(comment.id);
      for (const attachment of comment.attachments) {
        if (attachment.kind !== "mapPlanProposal") continue;
        logDebug(["frontend"], "map plan proposal comment arrived", { update: attachment.update !== undefined });
        review.arrived(thread.id, attachment.proposalId, () => openRef.current?.());
      }
    }
  }, [thread, review]);
}

/**
 * MPV-FR-UWQL / MPV-FR-KHDP: show the Delivery lens for a sheet that opens on
 * its own. A switch of the lens mounts a new Discussion column, so where the
 * focus was in the column, the composer of the thread in the new column takes
 * it back.
 */
export function showDeliveryKeepingFocus(
  showDelivery: (() => void) | undefined,
  switching: boolean,
  threadId: string | null,
  columnSelector: string,
): void {
  const active = document.activeElement;
  const inColumn =
    switching && threadId !== null && active instanceof HTMLElement && active.closest(columnSelector) !== null;
  showDelivery?.();
  if (inColumn) requestPendingFocus(threadId, "composer");
}
