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
export { InPlaceReviewScope } from "./InPlaceScope";
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
 *
 * MPV-FR-ZQRE: with `placement` "tab", the layer is the one of a tab other
 * than the Map tab, in the positioned box of the content of the tab. It shows
 * only inside an in-place scope (`InPlaceScope.tsx`), and its sheet stacks
 * below the chrome of the tab: the action control, the floating composer, and
 * the floating discussion panel.
 */
export function MapPlanReviewLayer({ placement = "map" }: { placement?: "map" | "tab" }) {
  const { store, host, inPlace } = useMapPlan();
  useSyncExternalStore(store.subscribe, store.getVersion);
  if (placement === "tab" && !inPlace) return null;
  return (
    <>
      <ReviewSheet store={store} host={host} placement={placement} />
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
