/**
 * The review of a plan proposal (`MPV-map-proposal-review.md`): a summary row
 * in the Map discussion (SMP-FR-PPJB) and a review sheet over the Delivery
 * canvas (MPV-FR-CTTA). Nothing reaches GitHub, and no draft is created, before
 * the author accepts the change that does it.
 */
import { useSyncExternalStore } from "react";

import { Icon } from "../icons";
import type { MapNodeRef } from "../../types";
import { useMapPlan } from "./host";
import { ReviewSheet } from "./Sheet";

export { MapPlanEntry } from "./Entry";
export {
  MapPlanContext,
  resetMapPlanFallbackForTest,
  useMapPlan,
  type MapPlanContextValue,
  type MapPlanHost,
} from "./host";

/**
 * MPV-FR-CTTA / MPV-FR-XAOG: what the canvas area holds of the review: the
 * sheet while one is open, and the announcement of each change of a status.
 * The canvas area is a positioned box, so both stay inside it.
 */
export function MapPlanReviewLayer() {
  const { store, host } = useMapPlan();
  useSyncExternalStore(store.subscribe, store.getVersion);
  return (
    <>
      <ReviewSheet store={store} host={host} />
      <span className="pp-sr-only" role="status" aria-live="polite" data-testid="map-plan-announcement">
        {store.snapshot().announcement}
      </span>
    </>
  );
}

/** MPR-FR-PMLX: the focus a message was sent with, as a chip below its body. */
export function MapNodeChip({ node }: { node: MapNodeRef }) {
  return (
    <span className="comment-attach__chip" data-testid="comment-attachment-map-node" title={`Focus: ${node.label}`}>
      <Icon.Node size={11} />
      <span className="comment-attach__chip-name">{node.label}</span>
      <span className="comment-attach__chip-type">focus</span>
    </span>
  );
}
