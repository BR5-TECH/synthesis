/**
 * The view model of a CALM canvas (`CAD-calm-architecture-diagrams.md`): the
 * place of each node in diagram coordinates, the ends of each edge, the
 * highlight of a chosen flow, and the item the author selected.
 */
import type { Box, CalmFlowView, CalmLayout } from "../../calm/types";

/** CAD-FR-KUBU: the item that the details panel shows. */
export type CalmSelection = { kind: "node"; id: string } | { kind: "edge"; id: string };

/** The box of each node in the coordinates of the whole diagram. */
export function absoluteBoxes(layout: CalmLayout): Record<string, Box> {
  const parent = new Map(layout.graph.nodes.map((node) => [node.id, node.parent]));
  // A `unique-id` such as `__proto__` or `constructor` is an own key here.
  const out: Record<string, Box> = Object.create(null);
  const place = (id: string): Box => {
    const known = out[id];
    if (known) return known;
    const own = Object.prototype.hasOwnProperty.call(layout.boxes, id)
      ? layout.boxes[id]
      : { x: 0, y: 0, width: 0, height: 0 };
    const holder = parent.get(id);
    const origin = holder === undefined ? { x: 0, y: 0 } : place(holder);
    const box = { ...own, x: own.x + origin.x, y: own.y + origin.y };
    out[id] = box;
    return box;
  };
  for (const node of layout.graph.nodes) place(node.id);
  return out;
}

export type Side = "left" | "right" | "top" | "bottom";

/** The point where a line from the centre of `from` toward `to` leaves `from`. */
export function exitPoint(from: Box, to: Box): { x: number; y: number; side: Side } {
  const cx = from.x + from.width / 2;
  const cy = from.y + from.height / 2;
  const tx = to.x + to.width / 2;
  const ty = to.y + to.height / 2;
  const dx = tx - cx;
  const dy = ty - cy;
  // A target inside the source (a node and the group that holds it) leaves
  // through the side nearest to the target.
  if (dx === 0 && dy === 0) return { x: from.x + from.width, y: cy, side: "right" };
  const scaleX = dx === 0 ? Infinity : from.width / 2 / Math.abs(dx);
  const scaleY = dy === 0 ? Infinity : from.height / 2 / Math.abs(dy);
  if (scaleX <= scaleY) {
    return { x: dx > 0 ? from.x + from.width : from.x, y: cy, side: dx > 0 ? "right" : "left" };
  }
  return { x: cx, y: dy > 0 ? from.y + from.height : from.y, side: dy > 0 ? "bottom" : "top" };
}

/** One step mark on an edge: its sequence number and its direction. */
export interface StepMark {
  sequence: number;
  reversed: boolean;
}

/** CAD-FR-KSEJ: what a chosen flow highlights. */
export interface FlowHighlight {
  /** The step marks of each edge that a step uses. */
  steps: Map<string, StepMark[]>;
  /** The nodes that a step uses. Every other node and edge is dimmed. */
  nodes: Set<string>;
}

/** CAD-FR-KSEJ, CLM-FR-FSCM: the highlight of `flow`. */
export function flowHighlight(flow: CalmFlowView): FlowHighlight {
  const steps = new Map<string, StepMark[]>();
  const nodes = new Set<string>();
  for (const step of flow.steps) {
    for (const id of [...step.from, ...step.to, ...step.nodeIds]) nodes.add(id);
    for (const edge of step.edgeIds) {
      const marks = steps.get(edge) ?? [];
      marks.push({ sequence: step.sequence, reversed: step.reversed });
      steps.set(edge, marks);
    }
  }
  return { steps, nodes };
}

/** The id that a selection names, when it still exists in `layout`. */
export function selectionExists(layout: CalmLayout, selection: CalmSelection | null): boolean {
  if (selection === null) return false;
  return selection.kind === "node"
    ? layout.graph.nodes.some((node) => node.id === selection.id)
    : layout.graph.edges.some((edge) => edge.id === selection.id);
}
