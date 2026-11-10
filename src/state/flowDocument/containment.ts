/**
 * The hierarchy and the geometry of a plan (FLO-FR-21, FLO-FR-42, FLO-FR-44,
 * FGV-FR-YHFX).
 *
 * These functions read a document and change nothing. A document that passed
 * the backend validation has no containment cycle (FGV-FR-13), but every walk
 * up the chain is bounded by the item count, so a bad document cannot make the
 * canvas hang.
 */

import {
  CARD_H,
  CARD_W,
  isContainerKind,
  PERMITTED_PARENTS,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
  type FlowPosition,
  type FlowSize,
} from "./types";

/** A rectangle in canvas coordinates. */
export interface FlowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function findItem(doc: FlowDocument, id: string): FlowItem | undefined {
  return doc.items.find((i) => i.id === id);
}

/** The container that holds `id`, or undefined for a top-level item. */
export function parentOf(doc: FlowDocument, id: string): string | undefined {
  return findItem(doc, id)?.parentId;
}

/** The items that `id` holds directly, in document order. */
export function childrenOf(doc: FlowDocument, id: string): FlowItem[] {
  return doc.items.filter((i) => i.parentId === id);
}

/** FLO-FR-45: whether a container holds any item. */
export function holdsAnything(doc: FlowDocument, id: string): boolean {
  return doc.items.some((i) => i.parentId === id);
}

/**
 * The chain of containers that hold `id`, the nearest first. A parent that
 * names no item ends the chain.
 */
export function ancestorsOf(doc: FlowDocument, id: string): string[] {
  const out: string[] = [];
  let at = parentOf(doc, id);
  while (at !== undefined && out.length <= doc.items.length) {
    if (out.includes(at) || at === id || !findItem(doc, at)) break;
    out.push(at);
    at = parentOf(doc, at);
  }
  return out;
}

/** Whether `ancestor` holds `of`, directly or through other containers. */
export function isAncestor(
  doc: FlowDocument,
  ancestor: string,
  of: string,
): boolean {
  return ancestorsOf(doc, of).includes(ancestor);
}

/** FGV-FR-KBLT: whether one of the two items holds the other. */
export function inLineage(doc: FlowDocument, a: string, b: string): boolean {
  return isAncestor(doc, a, b) || isAncestor(doc, b, a);
}

/** `id` and every item under it, at any depth. Empty for an unknown id. */
export function subtreeOf(doc: FlowDocument, id: string): Set<string> {
  const out = new Set<string>();
  if (!findItem(doc, id)) return out;
  out.add(id);
  let grew = true;
  while (grew) {
    grew = false;
    for (const item of doc.items) {
      if (
        item.parentId !== undefined &&
        out.has(item.parentId) &&
        !out.has(item.id)
      ) {
        out.add(item.id);
        grew = true;
      }
    }
  }
  return out;
}

/**
 * Every leaf item (a task or a defect) at or under `id`, in document order.
 * A container that holds no leaf gives an empty list.
 */
export function leavesUnder(doc: FlowDocument, id: string): string[] {
  const under = subtreeOf(doc, id);
  return doc.items
    .filter((i) => under.has(i.id) && !isContainerKind(i.kind))
    .map((i) => i.id);
}

/**
 * FGV-FR-YHFX: whether an item of `kind` may sit in a container of
 * `parentKind`. `null` is the top level.
 */
export function permitsParent(
  kind: FlowItemKind,
  parentKind: FlowItemKind | null,
): boolean {
  return PERMITTED_PARENTS[kind].includes(parentKind);
}

/** The size an item occupies on the canvas: its own size, or the card size. */
export function itemExtent(item: FlowItem): FlowSize {
  return item.size ?? { width: CARD_W, height: CARD_H };
}

/** FLO-FR-21: where an item sits on the canvas, through all its containers. */
export function absolutePosition(doc: FlowDocument, id: string): FlowPosition {
  const item = findItem(doc, id);
  if (!item) return { x: 0, y: 0 };
  let { x, y } = item.position;
  for (const ancestorId of ancestorsOf(doc, id)) {
    const ancestor = findItem(doc, ancestorId);
    if (!ancestor) break;
    x += ancestor.position.x;
    y += ancestor.position.y;
  }
  return { x, y };
}

/** The rectangle any item occupies, in canvas coordinates. */
export function itemBounds(doc: FlowDocument, id: string): FlowRect | null {
  const item = findItem(doc, id);
  if (!item) return null;
  const at = absolutePosition(doc, id);
  const extent = itemExtent(item);
  return { x: at.x, y: at.y, width: extent.width, height: extent.height };
}

/** A container's rectangle in canvas coordinates; null for a leaf. */
export function containerBounds(
  doc: FlowDocument,
  id: string,
): FlowRect | null {
  const item = findItem(doc, id);
  if (!item || !isContainerKind(item.kind)) return null;
  return itemBounds(doc, id);
}

/**
 * FLO-FR-42: the innermost container whose bounds contain `point`, or
 * undefined for open canvas. This reads the geometry only: it does not ask
 * whether the kind that is dropped may join that container (see
 * `permitsParent`).
 *
 * `exclude` holds the items that cannot be the answer, for example the item
 * that is dragged. A container under an excluded item is excluded too, because
 * it moves with that item.
 */
export function innermostContainerAt(
  doc: FlowDocument,
  point: FlowPosition,
  exclude: ReadonlySet<string> = new Set(),
): string | undefined {
  let best: string | undefined;
  let bestDepth = -1;
  for (const item of doc.items) {
    if (exclude.has(item.id) || !isContainerKind(item.kind)) continue;
    const ancestors = ancestorsOf(doc, item.id);
    if (ancestors.some((a) => exclude.has(a))) continue;
    const b = containerBounds(doc, item.id);
    if (!b) continue;
    if (
      point.x < b.x ||
      point.y < b.y ||
      point.x > b.x + b.width ||
      point.y > b.y + b.height
    ) {
      continue;
    }
    // The deepest container wins: a nested container lies inside its parent,
    // so both contain the point.
    if (ancestors.length > bestDepth) {
      best = item.id;
      bestDepth = ancestors.length;
    }
  }
  return best;
}
