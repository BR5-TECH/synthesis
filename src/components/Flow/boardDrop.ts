/**
 * `DBR-delivery-board.md`: where a drag on the board lands, and what a drop
 * writes. Pure, so each rule is tested without a canvas.
 */
import {
  ancestorsOf,
  boardHolderOf,
  boardHolds,
  findItem,
  isColumnMilestone,
  isContainerKind,
  PERMITTED_PARENTS,
  resolveBoardLayout,
  type BoardHolder,
  type FlowDocument,
  type FlowItem,
  type FlowPosition,
  type FlowRect,
  type StoredLayout,
} from "../../state/flowDocument";
import { permittedText } from "./refusals";

/** DBR-FR-CZEU: the slot a drag would land in. */
export interface BoardDrop {
  holder: BoardHolder;
  /** The place in the stack, counted without the dragged item. */
  index: number;
  /** A collapsed group takes the item at the end of its stack. */
  collapsed: boolean;
}

/** DBR-FR-CZEU / DBR-FR-EEKD: the target of a drag at a point, or why there is none. */
export interface BoardTarget {
  drop: BoardDrop | null;
  /** DBR-FR-EEKD: the hint of a stack under the pointer that cannot hold the kind. */
  hint: string | null;
}

const inside = (r: FlowRect | null, p: FlowPosition) =>
  !!r && p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;

/**
 * DBR-FR-CZEU / DBR-FR-EEKD: the target stack of a drag of `item` with the
 * pointer at `point`: the innermost stack under the pointer whose holder can
 * hold the kind, and the slot before the first member whose vertical middle is
 * below the pointer. `rectOf` gives the rectangles the board shows now, and
 * `hidden` tells which items a collapsed group hides.
 */
export function boardTargetAt(ctx: {
  doc: FlowDocument;
  item: FlowItem;
  lifted: ReadonlySet<string>;
  point: FlowPosition;
  orders: ReadonlyMap<BoardHolder, readonly string[]>;
  collapsed: ReadonlySet<string>;
  none: FlowRect | null;
  rectOf: (id: string) => FlowRect | null;
  hidden: (id: string) => boolean;
}): BoardTarget {
  const { doc, item, lifted, point, orders, collapsed, none, rectOf, hidden } = ctx;
  let innermost: string | null = null;
  let depth = -1;
  for (const c of doc.items) {
    if (!isContainerKind(c.kind) || lifted.has(c.id) || hidden(c.id)) continue;
    const chain = ancestorsOf(doc, c.id);
    if (chain.some((a) => lifted.has(a))) continue;
    if (!inside(rectOf(c.id), point)) continue;
    if (chain.length > depth) {
      innermost = c.id;
      depth = chain.length;
    }
  }
  // The holders under the pointer, innermost first. A top-level group sits in
  // the No milestone column, so that column holds what the group cannot.
  const chain: BoardHolder[] = innermost ? [innermost, ...ancestorsOf(doc, innermost)] : [];
  const outermost = chain.length > 0 ? findItem(doc, chain[chain.length - 1]!) : undefined;
  const inNone = outermost ? !isColumnMilestone(outermost) : inside(none, point);
  const candidates: BoardHolder[] = inNone ? [...chain, null] : chain;
  const holder = candidates.find((h) => boardHolds(doc, h, item.kind));
  const hint =
    candidates.length > 0 && !boardHolds(doc, candidates[0], item.kind)
      ? permittedText(item.kind, PERMITTED_PARENTS[item.kind])
      : null;
  if (holder === undefined) return { drop: null, hint };
  const members = (orders.get(holder) ?? []).filter((id) => !lifted.has(id));
  if (holder !== null && collapsed.has(holder)) {
    return { drop: { holder, index: members.length, collapsed: true }, hint };
  }
  let index = members.length;
  for (let k = 0; k < members.length; k += 1) {
    const r = rectOf(members[k]);
    if (r && r.y + r.height / 2 > point.y) {
      index = k;
      break;
    }
  }
  return { drop: { holder, index, collapsed: false }, hint };
}

/** DBR-FR-KTJS: what a drop changes: the container, and the stored stack order. */
export interface BoardDropResult {
  /** The new container of the item, or undefined for the top level. */
  parentId: string | undefined;
  /** Whether the container changes, which is an edit (FLO-FR-42). */
  moves: boolean;
  /** The stored layout after the drop, or null where the order does not change. */
  stored: StoredLayout | null;
  /** The ids whose stored entries the drop wrote. */
  touched: string[];
}

/**
 * DBR-FR-KTJS: the result of a drop of `itemId` into `drop`. `after` is the
 * plan as it is once the item is in its new container. The stored layout
 * takes the slot position of each member of the stacks the drop changed.
 */
export function boardDropResult(
  doc: FlowDocument,
  after: FlowDocument,
  stored: StoredLayout,
  orders: ReadonlyMap<BoardHolder, readonly string[]>,
  itemId: string,
  drop: BoardDrop,
): BoardDropResult {
  const item = findItem(doc, itemId)!;
  const from = boardHolderOf(doc, item);
  const moves = from !== drop.holder;
  const source = (orders.get(from) ?? []).filter((id) => id !== itemId);
  const target = moves ? (orders.get(drop.holder) ?? []).filter((id) => id !== itemId) : [...source];
  target.splice(Math.min(drop.index, target.length), 0, itemId);
  const before = orders.get(drop.holder) ?? [];
  if (!moves && before.length === target.length && before.every((id, k) => id === target[k])) {
    return { parentId: item.parentId, moves, stored: null, touched: [] };
  }
  const next = new Map<BoardHolder, readonly string[]>([[drop.holder, target]]);
  if (moves) next.set(from, source);
  const laid = resolveBoardLayout(after, stored, { orders: next }).layout;
  const touched = [...new Set([...source, ...target])];
  const items = { ...stored.items };
  for (const id of touched) {
    const at = laid[id]?.position;
    if (at) items[id] = { ...items[id], position: { x: at.x, y: at.y } };
  }
  return { parentId: drop.holder ?? undefined, moves, stored: { items }, touched };
}

/**
 * DBR-FR-BBDZ: whether a screen point is outside the measured rectangle of the
 * canvas. A canvas with no measured size holds every point.
 */
export function pointOutside(
  r: { left: number; top: number; right: number; bottom: number; width: number; height: number } | undefined,
  c: FlowPosition | undefined,
): boolean {
  if (!r || !c || r.width <= 0 || r.height <= 0) return false;
  return c.x < r.left || c.x > r.right || c.y < r.top || c.y > r.bottom;
}
