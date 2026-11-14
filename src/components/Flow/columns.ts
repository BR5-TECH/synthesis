/**
 * FLO-FR-SQUS / FLO-FR-IAOQ / FLO-FR-OGPF: what the canvas draws around the
 * milestone columns: the sequence arrows between them, the No milestone
 * column, and where a moved column lands.
 */
import {
  CONTAINER_HEADER_H,
  CONTAINER_PADDING,
  MIN_CONTAINER_SIZE,
  columnWidth,
  isColumnMilestone,
  milestoneOrder,
  noMilestoneX,
  type FlowDocument,
  type FlowLayout,
  type FlowRect,
} from "../../state/flowDocument";

/** FLO-FR-SQUS: an arrow from the right side of one column to the left of the next. */
export interface SequenceArrow {
  from: string;
  to: string;
  x1: number;
  x2: number;
  y: number;
}

export interface ColumnsView {
  /** The milestone columns in the milestone order (FLO-FR-MCFR). */
  order: string[];
  arrows: SequenceArrow[];
  /** FLO-FR-IAOQ: the No milestone column, or null for a plan with no item. */
  none: FlowRect | null;
}

/**
 * The columns of `doc` as `layout` places them. `rectOf` gives the rectangle
 * of an item on the canvas.
 */
export function columnsOf(
  doc: FlowDocument,
  layout: FlowLayout,
  rectOf: (id: string) => FlowRect | null,
): ColumnsView {
  const order = milestoneOrder(doc)
    .map((m) => m.id)
    .filter((id) => layout[id]);
  const arrows: SequenceArrow[] = [];
  for (let k = 1; k < order.length; k += 1) {
    const a = rectOf(order[k - 1]);
    const b = rectOf(order[k]);
    if (!a || !b) continue;
    arrows.push({
      from: order[k - 1],
      to: order[k],
      x1: a.x + a.width,
      x2: b.x,
      y: Math.min(a.y, b.y) + CONTAINER_HEADER_H / 2,
    });
  }
  if (doc.items.length === 0) return { order, arrows, none: null };
  const others = doc.items
    .filter((i) => i.parentId === undefined && !isColumnMilestone(i))
    .map((i) => rectOf(i.id))
    .filter((r): r is FlowRect => r !== null);
  const first = order.length > 0 ? layout[order[0]]?.size?.width : undefined;
  const width = first ?? columnWidth([], others.map((r) => r.width));
  const x = noMilestoneX(order.length, width);
  let bottom = MIN_CONTAINER_SIZE.height;
  for (const r of others) {
    if (r.x >= x) bottom = Math.max(bottom, r.y + r.height + CONTAINER_PADDING);
  }
  return { order, arrows, none: { x, y: 0, width, height: bottom } };
}

/**
 * FLO-FR-OGPF: the place in the milestone order of a column dropped with the
 * pointer at `pointerX`: before each other column whose middle is right of
 * the pointer.
 */
export function columnDropIndex(
  view: ColumnsView,
  rectOf: (id: string) => FlowRect | null,
  moved: string,
  pointerX: number,
): number {
  let index = 0;
  for (const id of view.order) {
    if (id === moved) continue;
    const r = rectOf(id);
    if (r && r.x + r.width / 2 < pointerX) index += 1;
  }
  return index;
}
