/**
 * The delivery-plan operations: the state of a plan, its overlaps, the plan
 * items of a draft, and the tracker sync.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ConflictResolution,
  FlowDeliveryState,
  FlowItemMatch,
  FlowOverlapReport,
  FlowTrackerSettings,
  SyncResult,
  TrackerItemSnapshot,
} from "../types";

/** FDL-FR-AZQM: `"load flow delivery state"`, over the serialized plan. */
export const loadFlowDeliveryState = (body: string) =>
  invoke<FlowDeliveryState>("load_flow_delivery_state", { body });

/** FDL-FR-BDOD: `"find flow items for draft"`. */
export const findFlowItemsForDraft = (draftId: string) =>
  invoke<FlowItemMatch[]>("find_flow_items_for_draft", { draftId });

/** FOA-FR-LBTE: `"analyze flow overlap"`, over the serialized plan. */
export const analyzeFlowOverlap = (body: string) =>
  invoke<FlowOverlapReport>("analyze_flow_overlap", { body });

/** FTS-FR-LTMW: `"sync flow with tracker"`. */
export const syncFlowWithTracker = (flowId: string) =>
  invoke<SyncResult>("sync_flow_with_tracker", { flowId });

/** FTS-FR-ARHM: `"resolve flow sync conflicts"`. */
export const resolveFlowSyncConflicts = (
  flowId: string,
  resolutions: ConflictResolution[],
) => invoke<SyncResult>("resolve_flow_sync_conflicts", { flowId, resolutions });

/** FTS-FR-TPWE: `"read tracker item"`. */
export const readTrackerItem = (
  flowId: string,
  kind: "issue" | "milestone",
  number: number,
) => invoke<TrackerItemSnapshot>("read_tracker_item", { flowId, kind, number });

/** FTS-FR-OBPX: `"open tracker link"`. */
export const openTrackerLink = (url: string) =>
  invoke<void>("open_tracker_link", { url });

/** FTS-FR-LMBE: `"get flow tracker settings"`. */
export const getFlowTrackerSettings = () =>
  invoke<FlowTrackerSettings>("get_flow_tracker_settings");

/** FTS-FR-ALOJ: `"set flow tracker settings (deliverable type, epic type, feature type, task type, defect type)"`. */
export const setFlowTrackerSettings = (settings: FlowTrackerSettings) =>
  invoke<FlowTrackerSettings>("set_flow_tracker_settings", { ...settings });
