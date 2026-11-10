import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import {
  CONTAINER_HEADER_H,
  type FlowPosition,
  type FlowRect,
} from "../../state/flowDocument";

/** FLO-FR-22: pan and zoom are view state. They are never serialized. */
export interface View {
  x: number;
  y: number;
  scale: number;
}

export const DEFAULT_VIEW: View = { x: 24, y: 24, scale: 1 };
export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 2;
export const ZOOM_STEP = 0.1;

/**
 * FLO-FR-33: how near (in canvas units) a connection drag must come to an item
 * before it snaps to it.
 */
export const MAGNET_RADIUS = 60;

/**
 * FLO-FR-ZASL: the height a collapsed container paints at: its header and one
 * line of leaf counts.
 */
export const COLLAPSED_H = CONTAINER_HEADER_H + 26;

/** The side of an item an edge leaves or arrives on. */
export type Face = "left" | "right" | "top" | "bottom" | null;

const OUTWARD: Record<Exclude<Face, null>, FlowPosition> = {
  left: { x: -1, y: 0 },
  right: { x: 1, y: 0 },
  top: { x: 0, y: -1 },
  bottom: { x: 0, y: 1 },
};

/** The two control points of an edge's curve. */
function edgeControls(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  faceA: Face,
  faceB: Face,
) {
  const dx = bx - ax;
  const dy = by - ay;
  const span = Math.hypot(dx, dy);
  const reach = Math.min(Math.max(span * 0.45, 8), 120);
  const out = (face: Face, awayX: number, awayY: number): FlowPosition => {
    if (face) {
      const n = OUTWARD[face];
      return { x: n.x * reach, y: n.y * reach };
    }
    const len = span || 1;
    return { x: (awayX / len) * reach, y: (awayY / len) * reach };
  };
  const c1 = out(faceA, dx, dy);
  const c2 = out(faceB, -dx, -dy);
  return { c1: { x: ax + c1.x, y: ay + c1.y }, c2: { x: bx + c2.x, y: by + c2.y } };
}

/**
 * FLO-FR-48: the curve of an edge. Each end leaves its item straight out of the
 * side it is on, so the arrowhead at the blocked end points into the border of
 * that item. An end with no side (the free end of a connection that is being
 * drawn) follows the drag.
 */
export function edgePath(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  faceA: Face,
  faceB: Face,
): string {
  const { c1, c2 } = edgeControls(ax, ay, bx, by, faceA, faceB);
  return `M ${ax} ${ay} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${bx} ${by}`;
}

/** The middle of an edge's curve, where its handle sits. */
export function edgeMidpoint(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  faceA: Face,
  faceB: Face,
): FlowPosition {
  const { c1, c2 } = edgeControls(ax, ay, bx, by, faceA, faceB);
  return {
    x: (ax + 3 * c1.x + 3 * c2.x + bx) / 8,
    y: (ay + 3 * c1.y + 3 * c2.y + by) / 8,
  };
}

/** An item drag in flight. `subtree` moves with the item (FLO-FR-21). */
export interface Drag {
  itemId: string;
  subtree: Set<string>;
  offsetX: number;
  offsetY: number;
  startAbs: FlowPosition;
  x: number;
  y: number;
  /** Where the pointer is, in canvas units: the drop point (FLO-FR-42). */
  pointer: FlowPosition;
  moved: boolean;
}

/** Which edges of a container a resize moves (FLO-FR-44). */
export interface ResizeSides {
  left: boolean;
  right: boolean;
  top: boolean;
  bottom: boolean;
}

/**
 * A container resize in flight (FLO-FR-44). The limits are in canvas
 * coordinates: the left edge cannot pass `maxLeft`, the right edge cannot come
 * before `minRight`, and the same for the top and the bottom.
 */
export interface Resize {
  itemId: string;
  sides: ResizeSides;
  startX: number;
  startY: number;
  from: FlowRect;
  maxLeft: number;
  maxTop: number;
  minRight: number;
  minBottom: number;
  minWidth: number;
  minHeight: number;
  /** The rectangle now, in canvas coordinates. */
  rect: FlowRect;
}

/** A canvas pan in flight. */
export interface Pan {
  startX: number;
  startY: number;
  originX: number;
  originY: number;
}

/**
 * How one stored artifact reference resolves (FLO-FR-11). `pending` is the
 * state before the project tree has been read.
 */
export type Reference =
  | { state: "pending"; id: string }
  | { state: "resolved"; id: string; artifact: ArtifactOption }
  | { state: "unresolved"; id: string };

export function resolveReferences(
  ids: readonly string[] | undefined,
  byId: Map<string, ArtifactOption>,
  treeLoaded: boolean,
): Reference[] {
  return (ids ?? []).map((id) => {
    const artifact = byId.get(id);
    if (artifact) return { state: "resolved", id, artifact };
    return treeLoaded ? { state: "unresolved", id } : { state: "pending", id };
  });
}

/** What a reference is called in its row and in its remove control. */
export function referenceLabel(ref: Reference): string {
  return ref.state === "resolved" ? ref.artifact.displayName : ref.id;
}

/**
 * FLO-FR-10: what the picker calls an artifact. A skill goes by its declared
 * name, every other artifact by its path, so two files with one basename are
 * told apart.
 */
export function optionLabel(a: ArtifactOption): string {
  return a.artifactType === "skill" ? a.displayName : a.path;
}

/**
 * Whether a pointer landed on a control inside an item, and not on the item.
 * A drag that starts on a field would move the item away from the cursor.
 */
export function isInteractive(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el?.closest?.("input, textarea, select, button, a");
}

/** FLO-FR-XAPK: today as a calendar date, in local time. */
export function todayIso(now: Date = new Date()): string {
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${m}-${d}`;
}

/** The text of an error from a backend call: its typed message, if it has one. */
export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message: unknown }).message;
    if (typeof message === "string" && message !== "") return message;
  }
  return String(e);
}
