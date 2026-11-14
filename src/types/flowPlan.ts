/**
 * The shapes of the delivery-plan operations: the delivery state
 * (`FDL-flow-delivery.md`) and the overlaps (`FOA-flow-overlap-analysis.md`).
 * Field names are camelCase, as the backend serializes them.
 */
import type { FlowValidationReport } from "./core";

/** FDL-FR-VGZI, FDL-FR-JYKC. */
export type FlowItemState =
  | "not_started"
  | "drafting"
  | "in_progress"
  | "done"
  | "dropped";

export interface FlowLeafCounts {
  notStarted: number;
  drafting: number;
  inProgress: number;
  done: number;
  dropped: number;
}

/** One item's delivery state (FDL contract surface). */
export interface ItemDeliveryState {
  itemId: string;
  state: FlowItemState;
  draft: { id: string; name: string; status: string } | null;
  draftMissing: boolean;
  run: { runId: string; state: string } | null;
  blocked: boolean;
  blockedBy: string[];
  leafCounts: FlowLeafCounts | null;
}

export interface FlowDeliveryState {
  items: ItemDeliveryState[];
}

/**
 * FDL-FR-JLXF: one plan item a draft is worked for. A result of a GitHub plan
 * carries no `flowId`, and the Project's title as `flowName`.
 */
export interface FlowItemMatch {
  flowId: string | null;
  flowName: string;
  /** FDL-FR-JLXF: the plan is the project's delivery. */
  delivery: boolean;
  itemId: string;
  itemName: string;
  kind: string;
}

/** FOA contract surface. */
export interface FlowOverlap {
  items: [string, string];
  paths: string[];
}

export interface FlowOverlapReport {
  overlaps: FlowOverlap[];
}

/** The typed error of the delivery and overlap operations (FDL-FR-VTOQ). */
export interface FlowReadError {
  code: string;
  message: string;
  report?: FlowValidationReport;
}
