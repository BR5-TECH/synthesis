/** Elements and containment (FLO-FR-06 / FLO-FR-41 / FLO-FR-42). */

import type {
  FlowDocument,
  FlowElement,
  FlowPosition,
} from "./types";

/** Every element in the document: loops first, then nodes, in document order. */
export function elements(doc: FlowDocument): FlowElement[] {
  return [
    ...doc.loops.map((l) => ({ kind: "loop" as const, ...l })),
    ...doc.nodes.map((n) => ({ kind: "node" as const, ...n })),
  ];
}

export function findElement(
  doc: FlowDocument,
  id: string,
): FlowElement | undefined {
  const loop = doc.loops.find((l) => l.id === id);
  if (loop) return { kind: "loop", ...loop };
  const node = doc.nodes.find((n) => n.id === id);
  return node ? { kind: "node", ...node } : undefined;
}

/** The loop holding `id`, or undefined for a top-level element. */
export function parentOf(doc: FlowDocument, id: string): string | undefined {
  return findElement(doc, id)?.parentId;
}

/**
 * FLO-FR-40: two elements may be joined only when they share a container.
 * An absent parent on both is the top level, which is a container like any other.
 */
export function areSiblings(doc: FlowDocument, a: string, b: string): boolean {
  return parentOf(doc, a) === parentOf(doc, b);
}

/**
 * The chain of loops containing `id`, innermost first. Bounded by the element
 * count so a document that somehow closed a containment cycle — which
 * `FGV-FR-13` refuses and nothing here creates — terminates rather than hanging
 * the canvas.
 */
export function ancestorsOf(doc: FlowDocument, id: string): string[] {
  const out: string[] = [];
  const limit = doc.loops.length + doc.nodes.length;
  let at = parentOf(doc, id);
  while (at && out.length <= limit) {
    if (out.includes(at)) break;
    out.push(at);
    at = parentOf(doc, at);
  }
  return out;
}

/** `id` and every element nested anywhere inside it. */
export function subtreeOf(doc: FlowDocument, id: string): Set<string> {
  const out = new Set<string>([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const e of elements(doc)) {
      if (e.parentId && out.has(e.parentId) && !out.has(e.id)) {
        out.add(e.id);
        grew = true;
      }
    }
  }
  return out;
}

/** Where an element sits on the canvas, resolved through its containers. */
export function absolutePosition(
  doc: FlowDocument,
  id: string,
): FlowPosition {
  const element = findElement(doc, id);
  if (!element) return { x: 0, y: 0 };
  let { x, y } = element.position;
  for (const ancestorId of ancestorsOf(doc, id)) {
    const ancestor = doc.loops.find((l) => l.id === ancestorId);
    if (!ancestor) break;
    x += ancestor.position.x;
    y += ancestor.position.y;
  }
  return { x, y };
}

/** A loop's rectangle in canvas coordinates. */
export function loopBounds(
  doc: FlowDocument,
  loopId: string,
): { x: number; y: number; width: number; height: number } | null {
  const loop = doc.loops.find((l) => l.id === loopId);
  if (!loop) return null;
  const at = absolutePosition(doc, loopId);
  return { x: at.x, y: at.y, width: loop.size.width, height: loop.size.height };
}

/**
 * FLO-FR-42: the innermost loop whose bounds contain `point`, or undefined for
 * open canvas.
 *
 * `exclude` holds the elements that cannot be the answer — the element being
 * dragged and everything nested inside it, since a loop is never placed inside
 * itself (FLO-FR-41).
 */
export function innermostLoopAt(
  doc: FlowDocument,
  point: FlowPosition,
  exclude: ReadonlySet<string> = new Set(),
): string | undefined {
  let best: string | undefined;
  let bestDepth = -1;
  for (const loop of doc.loops) {
    if (exclude.has(loop.id)) continue;
    const bounds = loopBounds(doc, loop.id);
    if (!bounds) continue;
    if (
      point.x < bounds.x ||
      point.y < bounds.y ||
      point.x > bounds.x + bounds.width ||
      point.y > bounds.y + bounds.height
    ) {
      continue;
    }
    // Deepest wins: a nested loop's bounds lie inside its parent's, so both
    // contain the point and only the innermost is the container being dropped
    // into.
    const depth = ancestorsOf(doc, loop.id).length;
    if (depth > bestDepth) {
      best = loop.id;
      bestDepth = depth;
    }
  }
  return best;
}
