/**
 * The Flow document shape, version 3: a delivery plan (FGV-FR-01, FLO-FR-03),
 * and the layout of a plan on the canvas, which is not part of the document
 * (FLO-FR-21, DMS-FR-DMWD).
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

/** A container kind holds other items, and its layout has a size (FGV-FR-01). */
export function isContainerKind(kind: FlowItemKind): boolean {
  return kind !== "task" && kind !== "defect";
}

/**
 * FGV-FR-YHFX: the containers each kind may sit in. `null` is the top level.
 * A milestone sits at the top level. Every other kind sits at the top level or
 * in a container of a higher rank.
 */
export const PERMITTED_PARENTS: Readonly<
  Record<FlowItemKind, readonly (FlowItemKind | null)[]>
> = {
  milestone: [null],
  epic: [null, "milestone"],
  feature: [null, "milestone", "epic"],
  task: [null, "milestone", "epic", "feature"],
  defect: [null, "milestone", "epic", "feature"],
};

/**
 * FLO-FR-06: one plan item. Optional fields are absent rather than empty. The
 * item has no position and no size: the layout keeps them (FLO-FR-21).
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
  /** FLO-FR-NRBV: the draft a task or a defect is worked in. Leaf kinds only. */
  draftId?: string;
  /** FLO-FR-ZEJU: project-relative paths; a trailing `/` names a folder. Leaf kinds only. */
  footprint?: string[];
  /** FLO-FR-XAPK: a calendar date `YYYY-MM-DD`. Milestones only. */
  due?: string;
}

/** FLO-FR-15: `blocks` runs from the blocking item to the blocked one. */
export type FlowEdgeType = "blocks" | "relates";

export interface FlowEdge {
  id: string;
  from: string;
  to: string;
  type: FlowEdgeType;
}

export interface FlowDocument {
  version: 3;
  /** FLO-FR-25: always written, empty or not. */
  name: string;
  /** FLO-FR-25: absent when empty. */
  description?: string;
  /** FLO-FR-YDWT: always present in memory; absent from the file when empty. */
  references: string[];
  items: FlowItem[];
  edges: FlowEdge[];
}

/** The only document version this build reads or writes (FGV-FR-07). */
export const FLOW_VERSION = 3;

/** FLO-FR-07: what a freshly added item of each kind is called. */
export const DEFAULT_ITEM_NAMES: Readonly<Record<FlowItemKind, string>> = {
  milestone: "New milestone",
  epic: "New epic",
  feature: "New feature",
  task: "New task",
  defect: "New defect",
};

/** FLO-FR-HBBF: the size an empty container of each kind takes when no size is stored. */
export const DEFAULT_CONTAINER_SIZES: Readonly<
  Record<"milestone" | "epic" | "feature", FlowSize>
> = {
  milestone: { width: 640, height: 380 },
  epic: { width: 520, height: 300 },
  feature: { width: 400, height: 220 },
};

/**
 * FLO-FR-21 / FLO-FR-44 / FLO-FR-ZASL: where one item sits on the canvas. The
 * position is measured from the origin of whatever holds the item. A container
 * has a size and can be collapsed; a leaf has neither.
 */
export interface PlacedItem {
  position: FlowPosition;
  size?: FlowSize;
  collapsed?: boolean;
}

/**
 * The layout of a plan on the canvas: an entry for every item, a size for every
 * container. The canvas builds it from the stored layout and the automatic
 * layout (FLO-FR-PLUZ, FLO-FR-HBBF), and it is never part of the document.
 */
export type FlowLayout = Readonly<Record<string, PlacedItem>>;

/**
 * DMS-FR-DMWD: the layout as the machine stores it for a scope. Every field of
 * an entry is optional, and an entry for an item that the plan does not hold is
 * kept and ignored (DMS-FR-EKKY).
 */
export interface StoredLayout {
  items: Record<
    string,
    { position?: FlowPosition; size?: FlowSize; collapsed?: boolean }
  >;
}

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
