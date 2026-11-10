import {
  ancestorsOf,
  CARD_H,
  CARD_W,
  findItem,
  isContainerKind,
  type FlowDocument,
  type FlowPosition,
  type FlowRect,
  type FlowSize,
} from "../../state/flowDocument";
import { COLLAPSED_H, type Face } from "./geometry";

/** The two ends of an edge on the canvas, each on the border of its item. */
export interface EdgeRoute {
  ax: number;
  ay: number;
  bx: number;
  by: number;
  faceA: Face;
  faceB: Face;
  /** The items the edge is drawn between: an end or its collapsed container. */
  from: string;
  to: string;
}

/**
 * The geometry that the canvas draws against: where each item paints, which
 * collapsed container hides it, and where an edge lands (FLO-FR-39,
 * FLO-FR-48, FLO-FR-ZASL).
 *
 * A factory and not free functions, because it reads the live document, the
 * positions an in-flight drag or resize gives, the card heights that the cards
 * report, and the collapse state.
 */
export function createFrames(ctx: {
  doc: FlowDocument | null;
  heights: Record<string, number>;
  collapsed: ReadonlySet<string>;
  absOf: (id: string) => FlowPosition;
  sizeOf: (id: string) => FlowSize | undefined;
}) {
  const { doc, heights, collapsed, absOf, sizeOf } = ctx;

  /**
   * FLO-FR-ZASL: the outermost collapsed container that holds `id`, or
   * undefined when the item shows.
   */
  const hiddenBy = (id: string): string | undefined => {
    if (!doc) return undefined;
    const chain = ancestorsOf(doc, id);
    let out: string | undefined;
    for (const a of chain) if (collapsed.has(a)) out = a;
    return out;
  };

  /** The rectangle an item paints at, in canvas coordinates. */
  const rectOf = (id: string): FlowRect | null => {
    const item = doc ? findItem(doc, id) : undefined;
    if (!item) return null;
    const at = absOf(id);
    if (isContainerKind(item.kind)) {
      const size = sizeOf(id) ?? item.size ?? { width: CARD_W, height: CARD_H };
      return {
        x: at.x,
        y: at.y,
        width: size.width,
        height: collapsed.has(id) ? COLLAPSED_H : size.height,
      };
    }
    return { x: at.x, y: at.y, width: CARD_W, height: heights[id] || CARD_H };
  };

  /** The item an edge end is drawn on: the item, or the container that hides it. */
  const endpointOf = (id: string): string => hiddenBy(id) ?? id;

  /**
   * FLO-FR-39 / FLO-FR-48: an edge runs between the nearest sides of the two
   * items. Where the two rectangles share a band across that gap, the edge runs
   * straight across it.
   */
  const routeBetween = (a: FlowRect, b: FlowRect) => {
    const gapX = Math.max(b.x - (a.x + a.width), a.x - (b.x + b.width));
    const gapY = Math.max(b.y - (a.y + a.height), a.y - (b.y + b.height));
    if (gapX >= gapY) {
      const right = b.x + b.width / 2 > a.x + a.width / 2;
      const top = Math.max(a.y, b.y);
      const bottom = Math.min(a.y + a.height, b.y + b.height);
      const shared = bottom - top >= 16;
      const yA = shared ? (top + bottom) / 2 : a.y + Math.min(a.height / 2, 24);
      const yB = shared ? (top + bottom) / 2 : b.y + Math.min(b.height / 2, 24);
      return right
        ? { ax: a.x + a.width, ay: yA, bx: b.x, by: yB, faceA: "right" as Face, faceB: "left" as Face }
        : { ax: a.x, ay: yA, bx: b.x + b.width, by: yB, faceA: "left" as Face, faceB: "right" as Face };
    }
    const down = b.y + b.height / 2 > a.y + a.height / 2;
    const left = Math.max(a.x, b.x);
    const right = Math.min(a.x + a.width, b.x + b.width);
    const shared = right - left >= 16;
    const xA = shared ? (left + right) / 2 : a.x + a.width / 2;
    const xB = shared ? (left + right) / 2 : b.x + b.width / 2;
    return down
      ? { ax: xA, ay: a.y + a.height, bx: xB, by: b.y, faceA: "bottom" as Face, faceB: "top" as Face }
      : { ax: xA, ay: a.y, bx: xB, by: b.y + b.height, faceA: "top" as Face, faceB: "bottom" as Face };
  };

  /**
   * The route of an edge between two items, or null when it is not drawn:
   * an end is gone, or both ends are hidden in one collapsed container.
   */
  const route = (fromId: string, toId: string): EdgeRoute | null => {
    const from = endpointOf(fromId);
    const to = endpointOf(toId);
    if (from === to) return null;
    const a = rectOf(from);
    const b = rectOf(to);
    if (!a || !b) return null;
    return { ...routeBetween(a, b), from, to };
  };

  /** One item's border toward a loose point: the free end of a drag. */
  const anchorToward = (id: string, toward: FlowPosition) => {
    const r = rectOf(endpointOf(id));
    if (!r) return { x: toward.x, y: toward.y, face: null as Face };
    const point: FlowRect = { x: toward.x, y: toward.y, width: 0, height: 0 };
    const leg = routeBetween(r, point);
    return { x: leg.ax, y: leg.ay, face: leg.faceA };
  };

  /** How far `point` is from an item's rectangle; 0 inside it. */
  const distanceTo = (id: string, point: FlowPosition): number => {
    const r = rectOf(id);
    if (!r) return Infinity;
    const dx = Math.max(r.x - point.x, 0, point.x - (r.x + r.width));
    const dy = Math.max(r.y - point.y, 0, point.y - (r.y + r.height));
    return Math.hypot(dx, dy);
  };

  return { hiddenBy, rectOf, endpointOf, route, anchorToward, distanceTo };
}

export type Frames = ReturnType<typeof createFrames>;
