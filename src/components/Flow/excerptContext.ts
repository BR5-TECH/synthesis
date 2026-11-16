import { createContext } from "react";

/**
 * FLO-FR-XCRP / MDL-FR-RILW: whether the cards and the containers of a canvas
 * show the excerpt of a description. A Flow tab shows it. The Delivery lens of
 * the Map shows none, so an item reads as its name alone.
 */
export const FlowExcerptsContext = createContext(true);
