/**
 * MPV-FR-CUWB: the host of the review of a plan proposal in a tab other than
 * the Map tab. There is no Map lens and no canvas, so a reference is plain
 * text (MPV-FR-NMYT) and nothing is selected. **Discuss** puts the focus on the
 * composer of the discussion of the proposal, with no focus node
 * (MPV-FR-PTUT).
 */
import { focusDiscussion, requestPendingFocus } from "../../state/discussionFocus";
import { kindOfType } from "../../state/mapPlan/kinds";
import type { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import type { DeliverySettings } from "../../types";
import type { MapPlanHost } from "./host";

/** What the tab read of the delivery source for the review. */
export interface InPlaceDelivery {
  /** MPV-FR-PIQU: `owner/name` of a GitHub delivery source, else null. */
  repository: string | null;
  /** MPV-FR-SNCR: the Type mapping, or null before it is read. */
  settings: DeliverySettings | null;
}

export const NO_DELIVERY: InPlaceDelivery = { repository: null, settings: null };

export function inPlaceHost(
  store: MapPlanReviewStore,
  delivery: InPlaceDelivery,
  openDraft: ((draftId: string) => void) | undefined,
): MapPlanHost {
  return {
    itemOnCanvas: () => false,
    selectItem: () => {},
    ...(openDraft ? { openDraft } : {}),
    discuss: () => {
      const discussionId = store.snapshot().review?.discussionId;
      if (!discussionId) return;
      // The surface that shows the discussion takes the focus now. A surface
      // that mounts later (a closed floating panel) takes the request.
      if (focusDiscussion(discussionId, "composer") === "closed") {
        requestPendingFocus(discussionId, "composer");
      }
    },
    showDelivery: () => {},
    repository: delivery.repository,
    // MPV-FR-CUWB: a `modify_issue` row shows no kind chip outside the Map tab.
    kindOfIssue: () => null,
    kindOfType: (type) => kindOfType(delivery.settings, type),
  };
}
