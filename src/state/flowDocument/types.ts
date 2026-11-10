/**
 * The Flow document shape, version 2: a delivery plan (FGV-FR-01, FLO-FR-03).
 *
 * The backend validator (`src-tauri/src/flow_validation.rs`) is the authority on
 * what a valid document is; these types describe one that has passed it.
 */

export interface FlowPosition {
  x: number;
  y: number;
}

export interface FlowSize {
  width: number;
  height: number;
}

/** FLO-FR-06: the five kinds of a plan item. */
export type FlowItemKind =
  | "milestone"
  | "epic"
  | "feature"
  | "task"
  | "defect";

/** The order the add menu offers the kinds in (FLO-FR-07). */
export const ITEM_KINDS: readonly FlowItemKind[] = [
  "milestone",
  "epic",
  "feature",
  "task",
  "defect",
];

/** A container kind carries a size and holds other items (FGV-FR-01). */
export function isContainerKind(kind: FlowItemKind): boolean {
  return kind !== "task" && kind !== "defect";
}

/**
 * FGV-FR-YHFX: the containers each kind may sit in. `null` is the top level.
 */
export const PERMITTED_PARENTS: Readonly<
  Record<FlowItemKind, readonly (FlowItemKind | null)[]>
> = {
  milestone: [null],
  epic: ["milestone"],
  feature: ["milestone", "epic"],
  task: ["feature"],
  defect: ["feature", "epic", "milestone"],
};

/**
 * FLO-FR-06: one plan item. Optional fields are absent rather than empty.
 * `position` is measured from the origin of whatever holds the item
 * (FLO-FR-21).
 */
export interface FlowItem {
  /** Unique among all items in the document, and stable across saves. */
  id: string;
  kind: FlowItemKind;
  name: string;
  /** FLO-FR-13: Markdown. */
  description?: string;
  /** FLO-FR-10: stable, path-derived artifact keys (ASC-FR-13). */
  artifactIds?: string[];
  /** The container that holds this item; absent at the top level. */
  parentId?: string;
  /** FLO-FR-TZSU: never created on the tracker by a sync. */
  localOnly?: boolean;
  /** FLO-FR-NRBV: the draft a task or a defect is worked in. Leaf kinds only. */
  draftId?: string;
  /** FLO-FR-ZEJU: project-relative paths; a trailing `/` names a folder. Leaf kinds only. */
  footprint?: string[];
  /** FLO-FR-XAPK: a calendar date `YYYY-MM-DD`. Milestones only. */
  due?: string;
  position: FlowPosition;
  /** Present on container kinds, absent on leaf kinds (FGV-FR-MZIA). */
  size?: FlowSize;
}

/** FLO-FR-15: `blocks` runs from the blocking item to the blocked one. */
export type FlowEdgeType = "blocks" | "relates";

export interface FlowEdge {
  id: string;
  from: string;
  to: string;
  type: FlowEdgeType;
}

/** A tracker reference (FGV-FR-01's `ref`). */
export interface TrackerRef {
  kind: "issue" | "milestone";
  host: string;
  owner: string;
  repo: string;
  number: number;
  url: string;
}

/** The tracker values at the last sync (FTS-FR-PNMA). */
export interface SyncBase {
  name: string;
  description: string;
  state: "open" | "closed";
  type?: string | null;
  parent?: TrackerRef | null;
  milestone?: number | null;
  blockedBy?: TrackerRef[];
  due?: string | null;
}

export interface SyncLink {
  itemId: string;
  ref: TrackerRef;
  base: SyncBase;
}

export interface FlowSync {
  tracker: "github";
  links: SyncLink[];
  /**
   * FTS-FR-YHVD: the tracker items a sync never creates or pulls. Absent from
   * the file when empty (FGV-FR-01).
   */
  ignored?: TrackerRef[];
}

export interface FlowDocument {
  version: 2;
  /** FLO-FR-25: always written, empty or not. */
  name: string;
  /** FLO-FR-25: absent when empty. */
  description?: string;
  /** FLO-FR-YDWT: always present in memory; absent from the file when empty. */
  references: string[];
  items: FlowItem[];
  edges: FlowEdge[];
  /** FLO-FR-AKMV: present once the Flow is connected to a tracker. */
  sync?: FlowSync;
}

/** The only document version this build reads or writes (FGV-FR-07). */
export const FLOW_VERSION = 2;

/** FLO-FR-07: what a freshly added item of each kind is called. */
export const DEFAULT_ITEM_NAMES: Readonly<Record<FlowItemKind, string>> = {
  milestone: "New milestone",
  epic: "New epic",
  feature: "New feature",
  task: "New task",
  defect: "New defect",
};

/** FLO-FR-07: the size a new, empty container of each kind arrives at. */
export const DEFAULT_CONTAINER_SIZES: Readonly<
  Record<"milestone" | "epic" | "feature", FlowSize>
> = {
  milestone: { width: 640, height: 380 },
  epic: { width: 520, height: 300 },
  feature: { width: 400, height: 220 },
};

/** FLO-FR-44: the room a container keeps around what it holds. */
export const CONTAINER_PADDING = 16;
/** The room a container's header takes above its members. */
export const CONTAINER_HEADER_H = 40;
/**
 * The extent a leaf card occupies, which edges anchor to and a container must
 * hold (FLO-FR-44). A card grows with its content, so this is the floor.
 */
export const CARD_W = 170;
export const CARD_H = 96;

export function emptyFlow(): FlowDocument {
  return { version: FLOW_VERSION, name: "", references: [], items: [], edges: [] };
}
