/**
 * The shapes of the delivery-plan operations: the delivery state
 * (`FDL-flow-delivery.md`), the overlaps (`FOA-flow-overlap-analysis.md`), and
 * the tracker sync (`FTS-flow-tracker-sync.md`). Field names are camelCase, as
 * the backend serializes them.
 */
import type { FlowValidationReport } from "./core";
import type { TrackerRef } from "../state/flowDocument";

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

/** FDL-FR-JLXF: one plan item a draft is worked for. */
export interface FlowItemMatch {
  flowId: string;
  flowName: string;
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

/** FTS-FR-VEEG. */
export type SyncConflictKind =
  | "field"
  | "invalid_on_tracker"
  | "removed_in_flow"
  | "removed_on_tracker";

export type SyncChoice =
  | "keep_flow"
  | "take_tracker"
  | "unlink"
  | "restore"
  | "remove_item";

export interface SyncConflict {
  id: string;
  kind: SyncConflictKind;
  itemId: string;
  itemName: string;
  field: "name" | "description" | "type" | "parent" | "milestone" | "due" | null;
  base: unknown;
  flow: unknown;
  tracker: unknown;
  choices: SyncChoice[];
  url: string | null;
}

export interface SkippedIssue {
  number: number;
  title: string;
  reason: "no_kind_fits" | "issue_type_unavailable" | "foreign_repository";
}

/** FTS-FR-LVTU. */
export interface SyncResult {
  created: number;
  updated: number;
  added: number;
  changed: number;
  conflicts: SyncConflict[];
  skipped: SkippedIssue[];
  stale: string[];
}

export interface ConflictResolution {
  conflictId: string;
  choice: SyncChoice;
}

/** FTS-FR-TPWE. */
export interface TrackerItemSnapshot {
  ref: TrackerRef;
  name: string;
  description: string;
  state: "open" | "closed";
  type?: string | null;
  parent?: TrackerRef | null;
  milestone?: number | null;
  blockedBy?: TrackerRef[];
  due?: string | null;
}

/** FTS-FR-LMBE. */
export interface FlowTrackerSettings {
  deliverableType: string;
  epicType: string;
  featureType: string;
  taskType: string;
  defectType: string;
}
