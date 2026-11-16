/** FLO-FR-44: the resize of a container, from its edges and its corners. */
import {
  absolutePosition,
  CARD_H,
  CARD_W,
  childrenOf,
  CONTAINER_HEADER_H,
  CONTAINER_PADDING,
  MIN_CONTAINER_SIZE,
  relativePosition,
  type FlowDocument,
  type FlowLayout,
} from "../../state/flowDocument";
import type { Resize, ResizeSides } from "./geometry";

/**
 * FLO-FR-44: a resize of `id` from `sides`, with the pointer down at
 * (`clientX`, `clientY`), or null for an item with no size. A side cannot
 * pass the members, so a resize never puts a member outside the container.
 */
export function resizeFrom(
  doc: FlowDocument,
  layout: FlowLayout,
  heights: Readonly<Record<string, number>>,
  id: string,
  sides: ResizeSides,
  clientX: number,
  clientY: number,
): Resize | null {
  const size = layout[id]?.size;
  if (!size) return null;
  const at = absolutePosition(doc, layout, id);
  const members = childrenOf(doc, id);
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  for (const m of members) {
    const ext = layout[m.id]?.size ?? { width: CARD_W, height: heights[m.id] || CARD_H };
    const p = relativePosition(layout, m.id);
    left = Math.min(left, p.x);
    top = Math.min(top, p.y);
    right = Math.max(right, p.x + ext.width);
    bottom = Math.max(bottom, p.y + ext.height);
  }
  const any = members.length > 0;
  const from = { x: at.x, y: at.y, width: size.width, height: size.height };
  return {
    itemId: id,
    sides,
    startX: clientX,
    startY: clientY,
    from,
    maxLeft: any ? at.x + left - CONTAINER_PADDING : Infinity,
    maxTop: any ? at.y + top - CONTAINER_HEADER_H : Infinity,
    minRight: any ? at.x + right + CONTAINER_PADDING : -Infinity,
    minBottom: any ? at.y + bottom + CONTAINER_PADDING : -Infinity,
    minWidth: MIN_CONTAINER_SIZE.width,
    minHeight: MIN_CONTAINER_SIZE.height,
    rect: from,
  };
}

/** FLO-FR-44: the rectangle a resize gives for a pointer at (clientX, clientY). */
export function resizedRect(r: Resize, clientX: number, clientY: number, scale: number) {
  const dx = (clientX - r.startX) / scale;
  const dy = (clientY - r.startY) / scale;
  let left = r.from.x;
  let top = r.from.y;
  let right = r.from.x + r.from.width;
  let bottom = r.from.y + r.from.height;
  if (r.sides.left) left = Math.min(r.from.x + dx, r.maxLeft, right - r.minWidth);
  if (r.sides.right) right = Math.max(right + dx, r.minRight, left + r.minWidth);
  if (r.sides.top) top = Math.min(r.from.y + dy, r.maxTop, bottom - r.minHeight);
  if (r.sides.bottom) bottom = Math.max(bottom + dy, r.minBottom, top + r.minHeight);
  return {
    x: Math.round(left),
    y: Math.round(top),
    width: Math.round(right - left),
    height: Math.round(bottom - top),
  };
}
