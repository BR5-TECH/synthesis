/**
 * What the Map tab gives a plan proposal card beside the proposal itself
 * (`MPV-map-proposal-review.md` MPV-FR-NMYT, MPV-FR-FTXV). A card outside the
 * Map tab has no host, and then its item chips are plain text.
 */
import { createContext, useContext } from "react";

export interface MapPlanHost {
  /** MPV-FR-NMYT: whether the plan item of this issue number is on the canvas. */
  itemOnCanvas: (issueNumber: number) => boolean;
  /** MPV-FR-NMYT / MDL-FR-WTTQ: select that item on the canvas and centre it. */
  selectItem: (issueNumber: number) => void;
  /** MPV-FR-FTXV: open the draft a row created. */
  openDraft?: (draftId: string) => void;
}

export const MapPlanHostContext = createContext<MapPlanHost | null>(null);

export function useMapPlanHost(): MapPlanHost | null {
  return useContext(MapPlanHostContext);
}
