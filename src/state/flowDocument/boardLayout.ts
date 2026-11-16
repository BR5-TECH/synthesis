/**
 * `DBR-delivery-board.md`: the board layout of the Delivery lens.
 *
 * Each top-level milestone is a column and the last column is No milestone
 * (DBR-FR-ZGNB). An epic and a feature are groups: a header and a stack of
 * their members (DBR-FR-IQBN). A stack puts its members one below the other
 * with one gap, and a member fills the inner width of its stack
 * (DBR-FR-FISH). Every card has one height (DBR-FR-LNZI). The stored
 * positions give the stack order alone (DBR-FR-HTCE, DBR-FR-BKWE).
 *
 * Pure: the drag of the canvas asks for a layout with the dragged item lifted
 * out and a gap at the slot where it would land (DBR-FR-QCAJ, DBR-FR-CZEU).
 */

import { ancestorsOf, findItem, permitsParent } from "./containment";
import { COLUMN_GAP, memberOrder, siblingWaits } from "./autoLayout";
import { isColumnMilestone, milestoneOrder } from "./milestoneOrder";
import {
  CONTAINER_HEADER_H,
  isContainerKind,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
  type FlowLayout,
  type PlacedItem,
  type StoredLayout,
} from "./types";
import type { FlowRect } from "./containment";

/** DBR-FR-ZGNB: the width of a card at the deepest nesting of the plan. */
export const BOARD_CARD_W = 288;
/** DBR-FR-LNZI: the one height of every card: three lines and the padding. */
export const BOARD_CARD_H = 72;
/** DBR-FR-FISH: the one gap between two members of a stack. */
export const BOARD_GAP = 8;
/** DBR-FR-IQBN: the inset between the border of a holder and its stack. */
export const BOARD_INSET = 8;
/** DBR-FR-SBTR: the header line of a group and of a column. */
export const BOARD_HEAD_H = CONTAINER_HEADER_H;
/** The room of a stack that holds nothing, so a drop can reach it. */
export const BOARD_EMPTY_H = 40;
/** DBR-FR-SBTR / FLO-FR-ZASL: a collapsed group: its header and one line of counts. */
export const BOARD_COLLAPSED_H = CONTAINER_HEADER_H + 26;

/**
 * The holder of a stack: the id of a column or a group, or null for the No
 * milestone column, which is not an item (FLO-FR-IAOQ).
 */
export type BoardHolder = string | null;

/** DBR-FR-CZEU: the placeholder of a drag: a slot of `height` at `index` of a stack. */
export interface BoardGap {
  holder: BoardHolder;
  index: number;
  height: number;
}

export interface BoardOptions {
  /** A stack order to use in place of the stored one, by holder. */
  orders?: ReadonlyMap<BoardHolder, readonly string[]>;
  /** DBR-FR-QCAJ: the items that take no slot: the dragged item and what it holds. */
  lifted?: ReadonlySet<string>;
  /** DBR-FR-CZEU: the placeholder of the drag. */
  gap?: BoardGap | null;
}

export interface BoardLayout {
  layout: FlowLayout;
  /** DBR-FR-HTCE: the order of each stack, the lifted items included. */
  orders: Map<BoardHolder, string[]>;
  /** The placeholder, in canvas coordinates, or null. */
  gapRect: FlowRect | null;
  /** FLO-FR-IAOQ: the No milestone column, or null for a plan with no item. */
  none: FlowRect | null;
  /** DBR-FR-ZGNB: the one width of every column. */
  columnWidth: number;
}

const finite = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n);

/** The holder of an item on the board. A parent that is not a container reads as the top level. */
export function boardHolderOf(doc: FlowDocument, item: FlowItem): BoardHolder {
  if (item.parentId === undefined) return null;
  const parent = findItem(doc, item.parentId);
  return parent && isContainerKind(parent.kind) && parent.id !== item.id ? parent.id : null;
}

/** FGV-FR-YHFX: whether a holder can hold an item of `kind`. */
export function boardHolds(doc: FlowDocument, holder: BoardHolder, kind: FlowItemKind): boolean {
  if (holder === null) return permitsParent(kind, null);
  const parent = findItem(doc, holder);
  return !!parent && isContainerKind(parent.kind) && permitsParent(kind, parent.kind);
}

/**
 * DBR-FR-HTCE: the stack order of every holder: the members with a stored
 * position by their vertical position, then the rest in the automatic order
 * of FLO-FR-PLUZ. A milestone column is not a member of a stack.
 */
export function boardOrders(doc: FlowDocument, stored: StoredLayout): Map<BoardHolder, string[]> {
  const index = new Map(doc.items.map((item, i) => [item.id, i]));
  const waits = siblingWaits(doc);
  const members = new Map<BoardHolder, FlowItem[]>();
  for (const item of doc.items) {
    if (isColumnMilestone(item)) continue;
    const holder = boardHolderOf(doc, item);
    const list = members.get(holder) ?? [];
    list.push(item);
    members.set(holder, list);
  }
  const out = new Map<BoardHolder, string[]>();
  for (const [holder, list] of members) {
    const placed: { id: string; y: number; x: number; i: number }[] = [];
    const auto: FlowItem[] = [];
    for (const m of list) {
      const p = stored.items[m.id]?.position;
      if (p && finite(p.x) && finite(p.y)) placed.push({ id: m.id, y: p.y, x: p.x, i: index.get(m.id) ?? 0 });
      else auto.push(m);
    }
    placed.sort((a, b) => a.y - b.y || a.x - b.x || a.i - b.i);
    out.set(holder, [...placed.map((p) => p.id), ...memberOrder(auto, waits, index).map((m) => m.id)]);
  }
  return out;
}

/**
 * DBR-FR-ZGNB: the deepest nesting of groups: the count of groups around a
 * card, and around the stack of a group.
 */
function deepestNesting(doc: FlowDocument): number {
  let deepest = 0;
  for (const item of doc.items) {
    if (isColumnMilestone(item)) continue;
    const groups = ancestorsOf(doc, item.id).filter((id) => {
      const a = findItem(doc, id);
      return a && !isColumnMilestone(a);
    }).length;
    deepest = Math.max(deepest, groups + (isContainerKind(item.kind) ? 1 : 0));
  }
  return deepest;
}

/** DBR-FR-ERHW .. DBR-FR-BKWE: the board layout of `doc`. */
export function resolveBoardLayout(
  doc: FlowDocument,
  stored: StoredLayout,
  options: BoardOptions = {},
): BoardLayout {
  const orders = boardOrders(doc, stored);
  for (const [holder, order] of options.orders ?? []) {
    const known = new Set(orders.get(holder) ?? []);
    const pinned = order.filter((id) => findItem(doc, id) && boardHolderOf(doc, findItem(doc, id)!) === holder);
    const rest = [...known].filter((id) => !pinned.includes(id));
    orders.set(holder, [...pinned, ...rest]);
  }
  const lifted = options.lifted ?? new Set<string>();
  const gap = options.gap ?? null;
  const collapsed = (id: string) => stored.items[id]?.collapsed === true;
  const out: Record<string, PlacedItem> = {};
  /** The placeholder, relative to its holder. */
  let gapAt: { holder: BoardHolder; x: number; y: number; width: number; height: number } | null = null;

  /**
   * Lay out the stack of `holder` from (`x`, `y`), relative to the holder, at
   * the inner width `width`. Returns the bottom of the stack.
   */
  const stack = (holder: BoardHolder, x: number, y: number, width: number): number => {
    const order = (orders.get(holder) ?? []).filter((id) => !lifted.has(id));
    let at = y;
    let any = false;
    const putGap = () => {
      if (any) at += BOARD_GAP;
      gapAt = { holder, x, y: at, width, height: gap!.height };
      at += gap!.height;
      any = true;
    };
    order.forEach((id, k) => {
      if (gap && gap.holder === holder && gap.index === k) putGap();
      const item = findItem(doc, id)!;
      if (any) at += BOARD_GAP;
      const height = isContainerKind(item.kind) ? group(item, width) : BOARD_CARD_H;
      out[id] = {
        ...out[id],
        position: { x, y: at },
        size: { width, height },
      };
      at += height;
      any = true;
    });
    if (gap && gap.holder === holder && gap.index >= order.length) putGap();
    if (!any) at += BOARD_EMPTY_H;
    return at;
  };

  /** A group: its header and its stack, and the height they take. */
  const group = (item: FlowItem, width: number): number => {
    const inner = width - 2 * BOARD_INSET;
    const bottom = stack(item.id, BOARD_INSET, BOARD_HEAD_H, inner) + BOARD_INSET;
    if (collapsed(item.id)) out[item.id] = { ...out[item.id], position: { x: 0, y: 0 }, collapsed: true };
    return collapsed(item.id) ? BOARD_COLLAPSED_H : bottom;
  };

  const columnInner = BOARD_CARD_W + 2 * BOARD_INSET * deepestNesting(doc);
  const columnWidth = columnInner + 2 * BOARD_INSET;
  const columns = milestoneOrder(doc);
  columns.forEach((m, k) => {
    const bottom = stack(m.id, BOARD_INSET, BOARD_HEAD_H, columnInner) + BOARD_INSET;
    out[m.id] = {
      position: { x: k * (columnWidth + COLUMN_GAP), y: 0 },
      size: { width: columnWidth, height: collapsed(m.id) ? BOARD_COLLAPSED_H : bottom },
      ...(collapsed(m.id) ? { collapsed: true } : {}),
    };
  });
  const noneX = columns.length * (columnWidth + COLUMN_GAP);
  const noneBottom = stack(null, noneX + BOARD_INSET, BOARD_HEAD_H, columnInner) + BOARD_INSET;
  const none = doc.items.length > 0 ? { x: noneX, y: 0, width: columnWidth, height: noneBottom } : null;

  // An item no walk reaches (a containment cycle, or a lifted item) keeps a place.
  for (const item of doc.items) {
    if (!out[item.id]) out[item.id] = { position: { x: 0, y: 0 } };
  }
  const layout: FlowLayout = out;
  let gapRect: FlowRect | null = null;
  const g = gapAt as { holder: BoardHolder; x: number; y: number; width: number; height: number } | null;
  if (g) {
    let ox = 0;
    let oy = 0;
    if (g.holder !== null) {
      for (const id of [g.holder, ...ancestorsOf(doc, g.holder)]) {
        ox += out[id]?.position.x ?? 0;
        oy += out[id]?.position.y ?? 0;
      }
    }
    gapRect = { x: ox + g.x, y: oy + g.y, width: g.width, height: g.height };
  }
  return { layout, orders, gapRect, none, columnWidth };
}

/**
 * DBR-FR-BKWE / DBR-FR-KTJS: the layout as the machine stores it for the
 * board: a position for each member of a stack, which gives its order, and a
 * collapsed flag. A group has no stored size, and a milestone column no
 * stored position (FLO-FR-OGPF).
 */
export function toBoardStoredLayout(doc: FlowDocument, layout: FlowLayout): StoredLayout {
  const items: StoredLayout["items"] = {};
  for (const item of doc.items) {
    const entry = layout[item.id];
    if (!entry) continue;
    const column = isColumnMilestone(item);
    if (column && !entry.collapsed) continue;
    items[item.id] = {
      ...(column ? {} : { position: { x: entry.position.x, y: entry.position.y } }),
      ...(entry.collapsed ? { collapsed: true } : {}),
    };
  }
  return { items };
}
