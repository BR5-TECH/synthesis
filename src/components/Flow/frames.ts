import {
  findElement,
  MEMBER_NODE_H,
  type FlowDocument,
  type FlowEdge,
  type FlowElement,
  type FlowLoop,
  type FlowPosition,
  type FlowSize,
} from "../../state/flowDocument";
import { ANCHOR_DY, NODE_W, type Face } from "./geometry";

/**
 * The border geometry every edge is drawn against.
 *
 * A factory rather than free functions, because each of these reads the live
 * document, the positions an in-flight drag gives it, and the heights the nodes
 * report. It is built on every render, exactly as the closures it replaces were.
 */
export function createFrames(ctx: {
  doc: FlowDocument | null;
  heights: Record<string, number>;
  absOf: (id: string) => FlowPosition;
  sizeOf: (loop: FlowLoop) => FlowSize;
}) {
  const { doc, heights, absOf, sizeOf } = ctx;

  /**
   * The rectangle an element actually paints, and the point inside it that its
   * edges radiate from.
   *
   * A loop is its own size, radiating from its centre. A node is `NODE_W` wide
   * and as tall as it has been measured painting (FLO-FR-13 grows it with its
   * prompt), falling back to the nominal height before any measurement has been
   * taken. Its edges radiate from `ANCHOR_DY` below its top rather than from its
   * middle, so a node that grows — on being selected, say — does not drag every
   * edge incident to it downward and re-route a graph nothing changed in. On a
   * node too short for that line to sit above its middle, the middle is what is
   * used, which keeps an edge meeting a small node square in its side.
   */
  const frameOf = (element: FlowElement) => {
    const at = absOf(element.id);
    const size =
      element.kind === "loop"
        ? sizeOf(element)
        : { width: NODE_W, height: heights[element.id] || MEMBER_NODE_H };
    return {
      left: at.x,
      top: at.y,
      right: at.x + size.width,
      bottom: at.y + size.height,
      origin: {
        x: at.x + size.width / 2,
        y:
          at.y +
          (element.kind === "loop"
            ? size.height / 2
            : Math.min(ANCHOR_DY, size.height / 2)),
      },
    };
  };

  /** Where an element's edges radiate from, and what a magnet measures to. */
  const anchorCentre = (id: string): FlowPosition | null => {
    const element = doc ? findElement(doc, id) : null;
    return element ? frameOf(element).origin : null;
  };

  /**
   * How far along `delta` the ray leaving an element's origin crosses its own
   * border, as a fraction of `delta`'s length. `0` when there is no direction to
   * leave in, which is the only way this returns something non-finite.
   */
  const exitAt = (
    frame: ReturnType<typeof frameOf>,
    delta: FlowPosition,
  ): { t: number; face: Face } => {
    const { origin } = frame;
    const tx =
      delta.x > 0
        ? (frame.right - origin.x) / delta.x
        : delta.x < 0
          ? (frame.left - origin.x) / delta.x
          : Infinity;
    const ty =
      delta.y > 0
        ? (frame.bottom - origin.y) / delta.y
        : delta.y < 0
          ? (frame.top - origin.y) / delta.y
          : Infinity;
    if (!Number.isFinite(tx) && !Number.isFinite(ty)) return { t: 0, face: null };
    return tx <= ty
      ? { t: Math.max(tx, 0), face: delta.x > 0 ? "right" : "left" }
      : { t: Math.max(ty, 0), face: delta.y > 0 ? "bottom" : "top" };
  };

  /**
   * The two ends of an edge, each stopped on its own element's border.
   *
   * An edge therefore stops at the border of what it joins — at a loop's, so it
   * lands on the container instead of being drawn through it to something
   * inside (FLO-FR-39), and at a node's, so it lands on the node instead of
   * under it. That is also what puts the arrowhead somewhere it can be seen
   * (FLO-FR-48): elements paint over the edge layer, so a head left at an
   * element's centre would be buried by the very element it points at.
   */
  const anchorPair = (fromId: string, toId: string) => {
    const from = doc ? findElement(doc, fromId) : null;
    const to = doc ? findElement(doc, toId) : null;
    if (!from || !to) return null;
    const a = frameOf(from);
    const b = frameOf(to);
    const delta = { x: b.origin.x - a.origin.x, y: b.origin.y - a.origin.y };
    const exitA = exitAt(a, delta);
    const exitB = exitAt(b, { x: -delta.x, y: -delta.y });
    let ta = exitA.t;
    let tb = exitB.t;
    // Each border is measured out from its own element, so two elements that
    // overlap along this line push their ends PAST one another and the edge is
    // drawn backwards — with its head, which is worse than drawing nothing at
    // all, because a head pointing the wrong way is read rather than
    // disregarded. Where they would cross, both ends give up their borders and
    // the edge runs origin to origin: buried under the overlap, as anything
    // drawn between two elements sitting on each other must be, but never
    // pointing anywhere but at its target.
    if (ta + tb > 1) {
      ta = 0;
      tb = 0;
    }
    return {
      ax: a.origin.x + delta.x * ta,
      ay: a.origin.y + delta.y * ta,
      bx: b.origin.x - delta.x * tb,
      by: b.origin.y - delta.y * tb,
      faceA: exitA.face,
      faceB: exitB.face,
    };
  };

  /** One element's border, toward a loose point — the end of a drag in flight. */
  const anchorToward = (id: string, toward: FlowPosition) => {
    const element = doc ? findElement(doc, id) : null;
    if (!element) return { x: 0, y: 0, face: null as Face };
    const frame = frameOf(element);
    const delta = {
      x: toward.x - frame.origin.x,
      y: toward.y - frame.origin.y,
    };
    const { t, face } = exitAt(frame, delta);
    return {
      x: frame.origin.x + delta.x * t,
      y: frame.origin.y + delta.y * t,
      face,
    };
  };

  /** The two ends of an edge, each stopped at its own element's boundary. */
  const edgeAnchors = (edge: FlowEdge) => anchorPair(edge.from, edge.to);

  return { frameOf, anchorCentre, exitAt, anchorPair, anchorToward, edgeAnchors };
}
