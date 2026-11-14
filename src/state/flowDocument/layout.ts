/**
 * Membership, position, and size (FLO-FR-21, FLO-FR-42, FLO-FR-44,
 * FLO-FR-OAOZ, FLO-FR-TDHR, FLO-FR-ZASL).
 *
 * A change of membership is an edit of the document (`reparentItem`). A
 * position, a size, and a collapsed state are the layout alone: the functions
 * that change them return a new `FlowLayout` and never touch the document.
 */

import {
  absolutePosition,
  ancestorsOf,
  childrenOf,
  findItem,
  inLineage,
  itemExtent,
  permitsParent,
  relativePosition,
  subtreeOf,
  type FlowRect,
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
  type FlowLayout,
  type FlowPosition,
  type FlowSize,
  type PlacedItem,
  type StoredLayout,
} from "./types";
import { dependencyCycleEdges } from "./waitGraph";

/** The smallest size of a container. */
export const MIN_CONTAINER_SIZE: FlowSize = {
  width: 160,
  height: CONTAINER_HEADER_H + CONTAINER_PADDING * 2,
};

/**
 * FLO-FR-44: the smallest a container may be made: the bounds of its members,
 * plus the padding the container keeps around them. A leaf gives its card size.
 */
export function minContainerSize(
  doc: FlowDocument,
  layout: FlowLayout,
  id: string,
): FlowSize {
  const item = findItem(doc, id);
  if (!item || !isContainerKind(item.kind)) return { width: CARD_W, height: CARD_H };
  let width = MIN_CONTAINER_SIZE.width;
  let height = MIN_CONTAINER_SIZE.height;
  for (const m of childrenOf(doc, id)) {
    const at = relativePosition(layout, m.id);
    const extent = itemExtent(layout, m);
    width = Math.max(width, at.x + extent.width + CONTAINER_PADDING);
    height = Math.max(height, at.y + extent.height + CONTAINER_PADDING);
  }
  return { width, height };
}

/** One layout entry replaced; the same layout when nothing changes. */
function withEntry(layout: FlowLayout, id: string, entry: PlacedItem): FlowLayout {
  const was = layout[id];
  if (
    was &&
    was.position.x === entry.position.x &&
    was.position.y === entry.position.y &&
    was.size?.width === entry.size?.width &&
    was.size?.height === entry.size?.height &&
    (was.collapsed ?? false) === (entry.collapsed ?? false)
  ) {
    return layout;
  }
  return { ...layout, [id]: entry };
}

/**
 * FLO-FR-44: grow each container that holds `id`, the nearest first, so that
 * it holds what it holds. A container never becomes smaller here.
 */
export function growAncestors(doc: FlowDocument, layout: FlowLayout, id: string): FlowLayout {
  let next = layout;
  for (const a of ancestorsOf(doc, id)) {
    const entry = next[a];
    if (!entry) continue;
    const min = minContainerSize(doc, next, a);
    const size = entry.size ?? min;
    const grown = {
      width: Math.max(size.width, min.width),
      height: Math.max(size.height, min.height),
    };
    next = withEntry(next, a, { ...entry, size: grown });
  }
  return next;
}

/**
 * FLO-FR-21: put an item at `absolute` (canvas coordinates). The position is
 * stored in the coordinates of the container that holds the item in `doc`, and
 * the containers around it grow to hold it (FLO-FR-44). Its members move with
 * it, because their positions are measured from its origin.
 */
export function placeItem(
  doc: FlowDocument,
  layout: FlowLayout,
  id: string,
  absolute: FlowPosition,
): FlowLayout {
  const item = findItem(doc, id);
  if (!item) return layout;
  const origin =
    item.parentId === undefined
      ? { x: 0, y: 0 }
      : absolutePosition(doc, layout, item.parentId);
  const entry = layout[id] ?? { position: { x: 0, y: 0 } };
  const placed = withEntry(layout, id, {
    ...entry,
    position: { x: Math.round(absolute.x - origin.x), y: Math.round(absolute.y - origin.y) },
  });
  return growAncestors(doc, placed, id);
}

/**
 * FLO-FR-44: give a container the rectangle `rect` (canvas coordinates). The
 * members keep their place on the canvas when the left or the top side moves,
 * and the size is never below `minContainerSize`, so a resize never puts a
 * member outside it. A leaf has no size to change.
 */
export function resizeContainer(
  doc: FlowDocument,
  layout: FlowLayout,
  id: string,
  rect: FlowRect,
): FlowLayout {
  const item = findItem(doc, id);
  if (!item || !isContainerKind(item.kind)) return layout;
  const at = absolutePosition(doc, layout, id);
  const dx = Math.round(rect.x - at.x);
  const dy = Math.round(rect.y - at.y);
  let next = layout;
  if (dx !== 0 || dy !== 0) {
    next = placeItem(doc, next, id, { x: at.x + dx, y: at.y + dy });
    for (const m of childrenOf(doc, id)) {
      const p = relativePosition(next, m.id);
      next = withEntry(next, m.id, {
        ...(next[m.id] ?? { position: p }),
        position: { x: p.x - dx, y: p.y - dy },
      });
    }
  }
  const min = minContainerSize(doc, next, id);
  const entry = next[id] ?? { position: { x: 0, y: 0 } };
  next = withEntry(next, id, {
    ...entry,
    size: {
      width: Math.max(min.width, Math.round(rect.width)),
      height: Math.max(min.height, Math.round(rect.height)),
    },
  });
  return growAncestors(doc, next, id);
}

/** FLO-FR-ZASL: collapse or expand a container. */
export function setCollapsed(
  doc: FlowDocument,
  layout: FlowLayout,
  id: string,
  collapsed: boolean,
): FlowLayout {
  const item = findItem(doc, id);
  const entry = layout[id];
  if (!item || !entry || !isContainerKind(item.kind)) return layout;
  const { collapsed: _was, ...rest } = entry;
  return withEntry(layout, id, collapsed ? { ...rest, collapsed: true } : rest);
}

/**
 * DMS-FR-DMWD: the layout as the machine stores it. A collapsed flag is stored
 * only while it is set.
 */
export function toStoredLayout(layout: FlowLayout): StoredLayout {
  const items: StoredLayout["items"] = {};
  for (const [id, entry] of Object.entries(layout)) {
    items[id] = {
      position: { x: entry.position.x, y: entry.position.y },
      ...(entry.size ? { size: { width: entry.size.width, height: entry.size.height } } : {}),
      ...(entry.collapsed ? { collapsed: true } : {}),
    };
  }
  return { items };
}

/**
 * FLO-FR-42 / FLO-FR-OAOZ / FLO-FR-GZCH: move an item into `newParentId`, or
 * to the top level when it is undefined. Every edge stays. An item that stays
 * in its container gives back `doc` itself: a move inside a container is the
 * layout alone (FLO-FR-TDHR).
 *
 * Refused when the container does not permit the kind (the refusal names the
 * containers the kind can join), when the item would go into itself or into an
 * item under it, when an edge would then join an item to its ancestor or
 * descendant (FGV-FR-KBLT), and when the new membership gives the plan a
 * dependency cycle (FGV-FR-PMTY), because a `blocks` edge into a container
 * blocks everything in it.
 */
export function reparentItem(
  doc: FlowDocument,
  id: string,
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
  if (item.parentId === newParentId) return accept(doc);
  const next = patchItem(doc, id, (i) => ({
    ...without(i, "parentId"),
    ...(newParentId !== undefined ? { parentId: newParentId } : {}),
  }));

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
