/**
 * MPV-FR-UWQL: in a tab other than the Map tab, a plan proposal comment that
 * arrives in the discussion that a surface shows opens the sheet of the tab.
 * The Map tab follows its own arrivals from its Discussion column, so a
 * surface there asks nothing here.
 */
import { useContext } from "react";

import { useProposalArrivals } from "../MapDiscussion/useProposalArrivals";
import type { Discussion } from "../../types";
import { MapPlanContext } from "./host";

export function useInPlaceArrivals(thread: Discussion | undefined): void {
  const value = useContext(MapPlanContext);
  const scope = value?.inPlace ? value : null;
  useProposalArrivals(scope ? thread : undefined, scope?.store ?? null, undefined);
}
