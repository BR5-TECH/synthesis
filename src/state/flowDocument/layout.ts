/**
 * Position, size, and membership (FLO-FR-21, FLO-FR-42, FLO-FR-44,
 * FLO-FR-OAOZ).
 */

import {
  absolutePosition,
  childrenOf,
  findItem,
  inLineage,
  itemExtent,
  permitsParent,
  subtreeOf,
} from "./containment";
import { accept, refuse, type FlowEditResult } from "./result";
import { patchItem, without } from "./shared";
import {
  CARD_H,
  CARD_W,
  CONTAINER_HEADER_H,
  CONTAINER_PADDING,
  isContainerKind,
  PERMITTED_PARENTS,
  type FlowDocument,
  type FlowItemKind,
  type FlowPosition,
  type FlowSize,
} from "./types";
import { dependencyCycleEdges } from "./waitGraph";

/** The smallest size of an empty container. */
export const MIN_CONTAINER_SIZE: FlowSize = {
  width: 160,
  height: CONTAINER_HEADER_H + CONTAINER_PADDING * 2,
};

/**
 * FLO-FR-44: the smallest a container may be made: the bounds of its members,
 * plus the padding the container keeps around them. A leaf gives its card size.
 */
export function minContainerSize(doc: FlowDocument, id: string): FlowSize {
  const item = findItem(doc, id);
  if (!item || !isContainerKind(item.kind)) return { width: CARD_W, height: CARD_H };
  let width = MIN_CONTAINER_SIZE.width;
  let height = MIN_CONTAINER_SIZE.height;
  for (const m of childrenOf(doc, id)) {
    const extent = itemExtent(m);
    width = Math.max(width, m.position.x + extent.width + CONTAINER_PADDING);
    height = Math.max(height, m.position.y + extent.height + CONTAINER_PADDING);
  }
  return { width, height };
}

/**
 * FLO-FR-44: resize a container, never below `minContainerSize`, so a resize
 * never puts a member outside it. A leaf has no size to change.
 */
export function resizeItem(doc: FlowDocument, id: string, size: FlowSize): FlowDocument {
  const item = findItem(doc, id);
  if (!item || !isContainerKind(item.kind)) return doc;
  const min = minContainerSize(doc, id);
  const next = {
    width: Math.max(min.width, Math.round(size.width)),
    height: Math.max(min.height, Math.round(size.height)),
  };
  if (item.size && item.size.width === next.width && item.size.height === next.height) {
    return doc;
  }
  return patchItem(doc, id, (i) => ({ ...i, size: next }));
}

/**
 * FLO-FR-21 / FLO-FR-42 / FLO-FR-OAOZ: move an item to `absolute` (canvas
 * coordinates), into `newParentId`, or to the top level when it is undefined.
 * The position is stored in the new container's coordinates, and every edge
 * stays.
 *
 * Refused when the container does not permit the kind (the refusal names the
 * containers the kind can join), when the item would go into itself or into an
 * item under it, when an edge would then join an item to its ancestor or
 * descendant (FGV-FR-KBLT), and when the new membership gives the plan a
 * dependency cycle (FGV-FR-PMTY), because a `blocks` edge into a container
 * blocks everything in it.
 */
export function moveItem(
  doc: FlowDocument,
  id: string,
  absolute: FlowPosition,
  newParentId?: string,
): FlowEditResult {
  const item = findItem(doc, id);
  if (!item) return refuse("unknown_item");
  let parentKind: FlowItemKind | null = null;
  if (newParentId !== undefined) {
    const parent = findItem(doc, newParentId);
    if (!parent) return refuse("unknown_parent");
    if (subtreeOf(doc, id).has(newParentId)) return refuse("into_own_subtree");
    parentKind = parent.kind;
  }
  if (!permitsParent(item.kind, parentKind)) {
    return refuse("invalid_parent_kind", { permitted: PERMITTED_PARENTS[item.kind] });
  }

  const origin =
    newParentId === undefined ? { x: 0, y: 0 } : absolutePosition(doc, newParentId);
  const position = { x: absolute.x - origin.x, y: absolute.y - origin.y };
  const reparented = item.parentId !== newParentId;
  if (!reparented && item.position.x === position.x && item.position.y === position.y) {
    return accept(doc);
  }
  const next = patchItem(doc, id, (i) => ({
    ...without(i, "parentId"),
    ...(newParentId !== undefined ? { parentId: newParentId } : {}),
    position,
  }));
  if (!reparented) return accept(next);

  const lineage = next.edges.filter((e) => inLineage(next, e.from, e.to));
  if (lineage.length > 0) {
    return refuse("edge_within_lineage", { edgeIds: lineage.map((e) => e.id) });
  }
  const cycle = dependencyCycleEdges(next);
  if (cycle.length > 0) {
    const ids = new Set<string>();
    for (const e of next.edges) {
      if (cycle.includes(e.id)) {
        ids.add(e.from);
        ids.add(e.to);
      }
    }
    return refuse("dependency_cycle", {
      itemIds: next.items.filter((i) => ids.has(i.id)).map((i) => i.id),
      edgeIds: cycle,
    });
  }
  return accept(next);
}
