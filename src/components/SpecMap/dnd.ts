/**
 * Where a drag of a map node would land (`SMO-specification-map-organization.md`
 * SMO-FR-WGOF, SMO-FR-ARQV). Pure: a world point in, a drop target out.
 */
import { canMove } from "../../state/specMap/organize";
import { specChildren, type TreeIndex } from "../../state/specMap/tree";
import type { MapRef } from "../../state/specMap/types";
import {
  CHIP_GAP,
  CHIP_H,
  CHIP_ROW,
  CHIP_TOP,
  CHIP_W,
  CHIPS_PER_ROW,
  INSET,
  type LayoutBox,
  type MapLayout,
  type Rect,
} from "./layout";

export interface DropTarget {
  parentId: string | null;
  /** The position among the target's children with the dragged node already out. */
  index: number;
  /** The box that shows the drop border, or null for a drop among the root nodes. */
  highlightId: string | null;
  /** SMO-FR-ARQV: the insertion bar, in world coordinates. */
  bar: Rect | null;
}

const inside = (b: Rect, p: { x: number; y: number }) =>
  p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;

export function hitTestDrop(
  tree: TreeIndex,
  layout: MapLayout,
  dragged: MapRef,
  point: { x: number; y: number },
): DropTarget | null {
  if (dragged.kind === "index" && tree.depth.get(dragged.id) === 0) {
    return rootDrop(tree, layout, dragged.id, point);
  }
  // Innermost first, so a drop lands on the deepest box under the pointer.
  const candidates = layout.boxes
    .filter((b) => inside(b, point))
    .sort((a, b) => b.depth - a.depth);
  for (const box of candidates) {
    // `canMove` alone keeps the dragged subtree out: its parent depth rule
    // admits no node of that subtree as a target.
    if (!canMove(tree, dragged, box.id)) continue;
    return dropInto(tree, layout, dragged, box, point);
  }
  return null;
}

function dropInto(
  tree: TreeIndex,
  layout: MapLayout,
  dragged: MapRef,
  box: LayoutBox,
  point: { x: number; y: number },
): DropTarget {
  const node = tree.nodes.get(box.id)!;
  if (dragged.kind === "planned") {
    const others = (node.planned ?? []).filter((d) => d.draftId !== dragged.draftId);
    return { parentId: box.id, index: others.length, highlightId: box.id, bar: null };
  }
  if (dragged.kind === "spec") {
    const others = specChildren(node).filter((s) => s.code !== dragged.code);
    const left = box.x + INSET;
    const top = box.y + CHIP_TOP;
    const column = Math.max(
      0,
      Math.min(CHIPS_PER_ROW - 1, Math.floor((point.x - left + CHIP_GAP / 2) / (CHIP_W + CHIP_GAP))),
    );
    const row = Math.max(0, Math.floor((point.y - top) / CHIP_ROW));
    const index = Math.min(row * CHIPS_PER_ROW + column, others.length);
    return {
      parentId: box.id,
      index,
      highlightId: box.id,
      bar: {
        x: left + (index % CHIPS_PER_ROW) * (CHIP_W + CHIP_GAP) - 4,
        y: top + Math.floor(index / CHIPS_PER_ROW) * CHIP_ROW,
        w: 2,
        h: CHIP_H,
      },
    };
  }
  const siblings = layout.boxes
    .filter((b) => b.parentId === box.id && b.id !== dragged.id)
    .sort((a, b) => a.y - b.y);
  const index = siblings.filter((b) => b.y + b.h / 2 < point.y).length;
  const barY =
    index < siblings.length
      ? siblings[index].y - 5
      : siblings.length > 0
        ? siblings[siblings.length - 1].y + siblings[siblings.length - 1].h + 3
        : box.y + (box.relDepth === 0 ? 46 : 52) - 5;
  return {
    parentId: box.id,
    index,
    highlightId: box.id,
    bar: { x: box.x + INSET, y: barY, w: box.w - 2 * INSET, h: 2 },
  };
}

/** SMO-FR-WGOF: a root node drops only among the root nodes. */
function rootDrop(
  tree: TreeIndex,
  layout: MapLayout,
  draggedId: string,
  point: { x: number; y: number },
): DropTarget | null {
  const others = tree.index.roots.filter((r) => r.id !== draggedId);
  const rendered = others
    .map((r) => layout.box.get(r.id))
    .filter((b): b is LayoutBox => b !== undefined);
  if (rendered.length === 0) return null;
  let nearest = rendered[0];
  let best = Infinity;
  for (const b of rendered) {
    const d = Math.hypot(b.x + b.w / 2 - point.x, b.y + b.h / 2 - point.y);
    if (d < best) {
      best = d;
      nearest = b;
    }
  }
  const after = point.y > nearest.y + nearest.h / 2;
  const index = others.findIndex((r) => r.id === nearest.id) + (after ? 1 : 0);
  return {
    parentId: null,
    index,
    highlightId: null,
    bar: {
      x: nearest.x,
      y: after ? nearest.y + nearest.h + 4 : nearest.y - 6,
      w: nearest.w,
      h: 2,
    },
  };
}
