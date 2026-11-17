/**
 * What the Map tab gives the review of a plan proposal beside the proposal
 * itself (`MPV-map-proposal-review.md`): the shared review state (MPV-FR-CZDO)
 * and the ways into the canvas and the composer. A summary row outside the Map
 * tab has no host. It then reads a store of its own and offers no review.
 */
import { createContext, useContext } from "react";

import type { IssueKind } from "../../types";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";

export interface MapPlanHost {
  /** MPV-FR-NMYT: whether the plan item of this issue number is on the canvas. */
  itemOnCanvas: (issueNumber: number) => boolean;
  /** MPV-FR-NMYT / MDL-FR-WTTQ: select that item on the canvas and centre it. */
  selectItem: (issueNumber: number) => void;
  /** MPV-FR-PIQU: open the draft a row created. */
  openDraft?: (draftId: string) => void;
  /** MPV-FR-PTUT: focus the composer, with the item of this issue number as the focus node, or the thread's focus. */
  discuss: (issueNumber: number | null) => void;
  /** MPV-FR-FNIS: show the Delivery lens, where the sheet shows. */
  showDelivery: () => void;
  /** MPV-FR-PIQU: the repository of the plan, as `owner/name`, or null. */
  repository: string | null;
  /** MPV-FR-SNCR: the kind of a plan issue, or null where the plan does not name it. */
  kindOfIssue: (issueNumber: number) => IssueKind | null;
  /** MPV-FR-SNCR: the kind the plan gives a Type name, or null. */
  kindOfType: (type: string | null) => IssueKind | null;
}

export interface MapPlanContextValue {
  store: MapPlanReviewStore;
  host: MapPlanHost | null;
}

export const MapPlanContext = createContext<MapPlanContextValue | null>(null);

let fallback: MapPlanContextValue | null = null;

/** The review context of the Map tab, or a store of its own with no host. */
export function useMapPlan(): MapPlanContextValue {
  const value = useContext(MapPlanContext);
  if (value) return value;
  fallback ??= { store: new MapPlanReviewStore(), host: null };
  return fallback;
}

/** Forget the store that a summary row outside the Map tab reads. Tests only. */
export function resetMapPlanFallbackForTest(): void {
  fallback?.store.dispose();
  fallback = null;
}
