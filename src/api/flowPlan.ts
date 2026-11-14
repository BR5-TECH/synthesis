/**
 * The delivery-plan operations: the state of a plan, its overlaps, and the plan
 * items of a draft.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type { FlowDeliveryState, FlowItemMatch, FlowOverlapReport } from "../types";

/** FDL-FR-AZQM: `"load flow delivery state"`, over the serialized plan. */
export const loadFlowDeliveryState = (body: string) =>
  invoke<FlowDeliveryState>("load_flow_delivery_state", { body });

/** FDL-FR-BDOD: `"find flow items for draft"`. */
export const findFlowItemsForDraft = (draftId: string) =>
  invoke<FlowItemMatch[]>("find_flow_items_for_draft", { draftId });

/** FOA-FR-LBTE: `"analyze flow overlap"`, over the serialized plan. */
export const analyzeFlowOverlap = (body: string) =>
  invoke<FlowOverlapReport>("analyze_flow_overlap", { body });
