import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import {
  MEMBER_NODE_H,
  MEMBER_NODE_W,
  type FlowEdge,
  type FlowPosition,
  type FlowSize,
} from "../../state/flowDocument";
import type { ArtifactType } from "../../types";

/**
 * The width a node is laid out at, and the line its edges radiate from.
 *
 * A node's height grows with its rendered prompt (FLO-FR-13), so edges leave a
 * fixed distance below its top rather than its middle: a node that grew would
 * otherwise drag every edge incident to it downward, and the graph would
 * re-route itself when nothing about it changed.
 */
export const NODE_W = MEMBER_NODE_W;
export const ANCHOR_DY = MEMBER_NODE_H / 2;

/** Which side of an element an edge leaves or arrives on. */
export type Face = "left" | "right" | "top" | "bottom" | null;

/** The direction that leads out of an element, per face. */
export const OUTWARD: Record<Exclude<Face, null>, FlowPosition> = {
  left: { x: -1, y: 0 },
  right: { x: 1, y: 0 },
  top: { x: 0, y: -1 },
  bottom: { x: 0, y: 1 },
};

/**
 * The curve an edge is drawn as: a cubic that leaves each of its ends straight
 * out of the face it is attached to.
 *
 * Those two directions are the curve's tangents, and the arriving one is what an
 * arrowhead is turned to (FLO-FR-48). Tying them to the faces is what keeps a
 * head square to the border it sits on: an edge that attaches underneath an
 * element but leaves sideways puts its head across its own stroke, reading as
 * something that slid off the border rather than as an arrow into it. It is also
 * what gives every edge a tangent at all — an offset taken along the run between
 * two elements collapses onto the end points whenever that run is zero, leaving
 * the head no direction to read and its angle to whatever the renderer falls
 * back to. An end with no face — the loose end of a connection being drawn — is
 * the one case there is no border to be square to, and it follows the drag.
 */
export const edgeCurve = (
  ax: number,
  ay: number,
  bx: number,
  by: number,
  faceA: Face,
  faceB: Face,
): string => {
  const dx = bx - ax;
  const dy = by - ay;
  const span = Math.hypot(dx, dy);
  // Half the run, so a plain left-to-right edge keeps the shape it always had,
  // with a floor that keeps the curve legible when there is barely a run at all.
  const reach = Math.min(Math.max(span * 0.5, 8), 140);
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
  return `M ${ax} ${ay} C ${ax + c1.x} ${ay + c1.y}, ${bx + c2.x} ${by + c2.y}, ${bx} ${by}`;
};

export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 2;
export const ZOOM_STEP = 0.1;

/**
 * FLO-FR-33: how close (in graph units) a connection drag has to come to an
 * element before it snaps to it. Generous enough that the author does not have
 * to land on it, tight enough that it cannot reach past the nearest neighbour at
 * the spacing a laid-out Flow uses.
 */
export const MAGNET_RADIUS = 130;

/**
 * FLO-FR-10: how the picker groups what it offers, and the order it offers it
 * in. Only these types appear — see `useProjectArtifacts`.
 */
export const REFERENCE_GROUPS: ReadonlyArray<{ type: ArtifactType; label: string }> = [
  { type: "skill", label: "Skills" },
  { type: "prompt", label: "Prompts" },
  { type: "instructions", label: "Instructions" },
];

/** FLO-FR-22: pan and zoom, which are view state and are never serialized. */
export interface View {
  x: number;
  y: number;
  scale: number;
}

export const DEFAULT_VIEW: View = { x: 0, y: 0, scale: 1 };

/**
 * An in-flight element drag. `subtree` is the element and everything nested
 * inside it, which is what travels with a loop (FLO-FR-44); `startAbs` is where
 * the dragged element sat when the gesture began, so every member of the subtree
 * can be offset by the same delta.
 */
export interface Drag {
  elementId: string;
  subtree: Set<string>;
  offsetX: number;
  offsetY: number;
  startAbs: FlowPosition;
  x: number;
  y: number;
  moved: boolean;
}

/** An in-flight loop resize (FLO-FR-44). */
export interface Resize {
  loopId: string;
  startX: number;
  startY: number;
  from: FlowSize;
  /** FLO-FR-44: the floor, measured from what the loop actually holds. */
  min: FlowSize;
  size: FlowSize;
}

/** An in-flight canvas pan: where it began, and the view it began from. */
export interface Pan {
  startX: number;
  startY: number;
  originX: number;
  originY: number;
}

/**
 * FLO-FR-43: a move out of a container that would sever edges, held until the
 * author says whether they meant it.
 */
export interface PendingMove {
  elementId: string;
  name: string;
  absolute: FlowPosition;
  newParentId?: string;
  severed: FlowEdge[];
}

/**
 * How one of an element's stored artifact references resolves right now
 * (FLO-FR-11).
 *
 * `pending` is the state before any project tree has been read: the reference is
 * neither known-good nor known-missing, and rendering it as unresolved would
 * warn about an artifact that in all likelihood exists.
 */
export type Reference =
  | { state: "pending"; id: string }
  | { state: "resolved"; id: string; artifact: ArtifactOption }
  | { state: "unresolved"; id: string };

/**
 * FLO-FR-10: every reference the element carries, in the order it carries them.
 * A loop's references resolve on exactly a node's terms (FLO-FR-37).
 */
export function resolveReferences(
  element: { artifactIds?: string[] },
  byId: Map<string, ArtifactOption>,
  treeLoaded: boolean,
): Reference[] {
  return (element.artifactIds ?? []).map((id) => {
    const artifact = byId.get(id);
    if (artifact) return { state: "resolved", id, artifact };
    return treeLoaded ? { state: "unresolved", id } : { state: "pending", id };
  });
}

/** What a reference is called on the node, and in the remove affordance. */
export function referenceLabel(ref: Reference): string {
  return ref.state === "resolved" ? ref.artifact.displayName : ref.id;
}

/**
 * FLO-FR-10 / FLO-FR-11: what the picker calls an artifact. A skill goes by its
 * declared name, because every skill's file is `SKILL.md` and a list of those
 * distinguishes nothing; everything else goes by its path, so two artifacts
 * sharing a basename are still told apart.
 */
export function optionLabel(a: ArtifactOption): string {
  return a.artifactType === "skill" ? a.displayName : a.path;
}

/**
 * Whether a pointer landed on something the user is operating rather than on the
 * element itself. A drag armed from a field would move the element out from
 * under the cursor the moment they tried to select text in it.
 */
export function isInteractive(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el?.closest?.("input, textarea, select, button");
}
