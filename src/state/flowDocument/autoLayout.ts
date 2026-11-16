/**
 * FLO-FR-PLUZ / FLO-FR-HBBF: the layout of a plan, from the layout the machine
 * stores for its scope and an automatic layout for every item the stored
 * layout does not place.
 *
 * Top-level milestones are columns of one width, tops aligned, left to right
 * in the milestone order (FLO-FR-SQUS, FLO-FR-MCFR), and each stacks its
 * members top to bottom. The other top-level items stack in the last column,
 * No milestone (FLO-FR-IAOQ). Inside any other container, containers come
 * before leaves, a member comes after each member it waits for, and document
 * order decides the rest. A container without a stored size takes the size
 * its members need. A stored size is never smaller than that (FLO-FR-44). A
 * milestone column has no stored position or size (FLO-FR-OGPF).
 */

import { ancestorsOf, findItem } from "./containment";
import {
  CARD_H,
  CARD_W,
  CONTAINER_HEADER_H,
  CONTAINER_PADDING,
  isContainerKind,
  type FlowDocument,
  type FlowItem,
  type FlowLayout,
  type FlowPosition,
  type FlowSize,
  type PlacedItem,
  type StoredLayout,
} from "./types";
import { MIN_CONTAINER_SIZE } from "./layout";
import { isColumnMilestone, milestoneOrder } from "./milestoneOrder";

/** The room between two items that the automatic layout places. */
export const AUTO_GAP = 32;
/** FLO-FR-SQUS: the room between two columns, which the sequence arrow crosses. */
export const COLUMN_GAP = 56;
/**
 * The height the automatic layout gives a card row. A card grows with its
 * content, so a row takes more than the card floor.
 */
export const AUTO_CARD_H = CARD_H + 32;
/** Where the first member of a container goes, from the container's origin. */
export const MEMBER_ORIGIN: FlowPosition = {
  x: CONTAINER_PADDING,
  y: CONTAINER_HEADER_H + CONTAINER_PADDING,
};

const finite = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n);

function storedPosition(stored: StoredLayout, id: string): FlowPosition | undefined {
  const p = stored.items[id]?.position;
  return p && finite(p.x) && finite(p.y) ? { x: p.x, y: p.y } : undefined;
}

function storedSize(stored: StoredLayout, id: string): FlowSize | undefined {
  const s = stored.items[id]?.size;
  return s && finite(s.width) && finite(s.height) && s.width > 0 && s.height > 0
    ? { width: s.width, height: s.height }
    : undefined;
}

/**
 * For each item, the siblings it waits for directly through a `blocks` edge:
 * an edge from an item at or under A to an item at or under B makes B wait for
 * A, when A and B share a container (or both sit at the top level).
 */
function siblingWaits(doc: FlowDocument): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  const pathOf = (id: string) => [...ancestorsOf(doc, id)].reverse().concat(id);
  for (const e of doc.edges) {
    if (e.type !== "blocks" || !findItem(doc, e.from) || !findItem(doc, e.to)) continue;
    const a = pathOf(e.from);
    const b = pathOf(e.to);
    let k = 0;
    while (k < a.length && k < b.length && a[k] === b[k]) k += 1;
    // One end holds the other: the edge says nothing about an order of siblings.
    if (k >= a.length || k >= b.length) continue;
    const set = out.get(b[k]) ?? new Set<string>();
    set.add(a[k]);
    out.set(b[k], set);
  }
  return out;
}

/**
 * Containers before leaves, a member after each member it waits for, document
 * order for the rest. A cycle (which a valid plan does not have) is broken in
 * document order.
 */
function memberOrder(
  members: readonly FlowItem[],
  waits: Map<string, Set<string>>,
  index: Map<string, number>,
): FlowItem[] {
  const key = (i: FlowItem) => (isContainerKind(i.kind) ? 0 : 1) * 1e9 + (index.get(i.id) ?? 0);
  const ids = new Set(members.map((m) => m.id));
  const left = new Set(members);
  const placed = new Set<string>();
  const out: FlowItem[] = [];
  while (left.size > 0) {
    const ready = [...left].filter((m) =>
      [...(waits.get(m.id) ?? [])].every((w) => !ids.has(w) || placed.has(w)),
    );
    const pool = ready.length > 0 ? ready : [...left];
    const next = pool.reduce((best, m) => (key(m) < key(best) ? m : best));
    left.delete(next);
    placed.add(next.id);
    out.push(next);
  }
  return out;
}

/** FLO-FR-SQUS / FLO-FR-IAOQ: put `items` one below the other from `origin`. */
function stack(
  items: readonly FlowItem[],
  origin: FlowPosition,
  extent: (i: FlowItem) => FlowSize,
  put: (i: FlowItem, at: FlowPosition) => void,
): void {
  let y = origin.y;
  for (const item of items) {
    put(item, { x: origin.x, y });
    y += extent(item).height + AUTO_GAP;
  }
}

/**
 * FLO-FR-PLUZ / FLO-FR-HBBF / MDL-FR-ECPZ: what the canvas measured of the
 * items as its binding renders them. A card of a GitHub plan carries a Status
 * chip and a tracker row, and a container can carry an error line and its
 * references, so the floors alone would make items overlap. A value that is
 * missing or not above zero was not measured, and the floor applies.
 */
export interface LayoutMeasures {
  /** The height of a card. */
  cardHeight?: (id: string) => number | undefined;
  /** The width the header of a container needs to show its chips. */
  headWidth?: (id: string) => number | undefined;
  /** Where the content above a container's members ends, from its top. */
  memberTop?: (id: string) => number | undefined;
}

const measured = (n: number | undefined): number | undefined => (finite(n) && n > 0 ? n : undefined);

/** FLO-FR-PLUZ / FLO-FR-HBBF: the layout of `doc`, with `stored` applied. */
export function resolveLayout(
  doc: FlowDocument,
  stored: StoredLayout,
  measures: LayoutMeasures = {},
): FlowLayout {
  const cardHeight = (id: string) => measured(measures.cardHeight?.(id));
  /** Where the first member of `holder` goes. */
  const originOf = (holder: string): FlowPosition => {
    const top = measured(measures.memberTop?.(holder));
    return {
      x: MEMBER_ORIGIN.x,
      y: top === undefined ? MEMBER_ORIGIN.y : Math.max(MEMBER_ORIGIN.y, Math.ceil(top) + CONTAINER_PADDING),
    };
  };
  const out: Record<string, PlacedItem> = {};
  const index = new Map(doc.items.map((item, i) => [item.id, i]));
  const waits = siblingWaits(doc);
  // A parent that names no container reads as the top level, so every item
  // gets a place, also in a plan the backend would refuse.
  const holderOf = (item: FlowItem): string | undefined => {
    if (item.parentId === undefined) return undefined;
    const parent = findItem(doc, item.parentId);
    return parent && isContainerKind(parent.kind) && parent.id !== item.id
      ? parent.id
      : undefined;
  };
  const members = new Map<string | undefined, FlowItem[]>();
  for (const item of doc.items) {
    const holder = holderOf(item);
    const list = members.get(holder) ?? [];
    list.push(item);
    members.set(holder, list);
  }
  const extent = (i: FlowItem): FlowSize =>
    isContainerKind(i.kind)
      ? (out[i.id]?.size ?? MIN_CONTAINER_SIZE)
      : { width: CARD_W, height: Math.max(AUTO_CARD_H, Math.ceil(cardHeight(i.id) ?? 0)) };
  const put = (i: FlowItem, at: FlowPosition) => {
    out[i.id] = { ...out[i.id], position: at };
  };
  const visiting = new Set<string>();
  /** FLO-FR-SQUS: the milestone columns. */
  const columns = new Set(doc.items.filter(isColumnMilestone).map((m) => m.id));

  /** Place the members of `holder` that the stored layout does not place. */
  const placeMembers = (holder: string | undefined) => {
    const list = members.get(holder) ?? [];
    for (const m of list) if (isContainerKind(m.kind)) sizeContainer(m);
    const origin = holder === undefined ? { x: 0, y: 0 } : originOf(holder);
    let bottom = origin.y;
    let anyStored = false;
    const auto: FlowItem[] = [];
    for (const m of list) {
      // FLO-FR-OGPF: a milestone column has no stored position.
      const p = holder === undefined && isColumnMilestone(m) ? undefined : storedPosition(stored, m.id);
      if (!p) {
        auto.push(m);
        continue;
      }
      put(m, p);
      anyStored = true;
      bottom = Math.max(bottom, p.y + extent(m).height);
    }
    const startY = anyStored ? bottom + AUTO_GAP : bottom;
    const startX = origin.x;
    if (holder === undefined) {
      placeColumns(auto, anyStored ? startY : MEMBER_ORIGIN.y);
      return;
    }
    // FLO-FR-SQUS: a column stacks its members top to bottom, and so does
    // each container in it, as the wireframe of MDL shows, so a column is as
    // narrow as its widest member.
    stack(memberOrder(auto, waits, index), { x: startX, y: startY }, extent, put);
  };

  /**
   * FLO-FR-SQUS / FLO-FR-IAOQ: the milestone columns, one width, tops at 0,
   * in the milestone order, and the top-level items that are not milestones
   * stacked in the last column from `othersY`.
   */
  const placeColumns = (auto: readonly FlowItem[], othersY: number) => {
    const order = milestoneOrder(doc).filter((m) => out[m.id]);
    const others = memberOrder(
      auto.filter((m) => !isColumnMilestone(m)),
      waits,
      index,
    );
    const width = columnWidth(
      order.map((m) => extent(m).width),
      others.map((m) => extent(m).width),
    );
    order.forEach((m, k) => {
      out[m.id] = {
        ...out[m.id],
        position: { x: k * (width + COLUMN_GAP), y: 0 },
        size: { width, height: extent(m).height },
      };
    });
    stack(others, { x: noMilestoneX(order.length, width) + CONTAINER_PADDING, y: othersY }, extent, put);
  };

  /** Place a container's members first, then give it the size they need. */
  const sizeContainer = (c: FlowItem) => {
    if (visiting.has(c.id)) return;
    visiting.add(c.id);
    placeMembers(c.id);
    const list = members.get(c.id) ?? [];
    // FLO-FR-HBBF: an empty container takes the room its header and its
    // excerpt need, measured below, and no more.
    let need: FlowSize = { ...MIN_CONTAINER_SIZE };
    // MDL-FR-ECPZ: the header shows its chips, and the content above the
    // members shows in full.
    const head = measured(measures.headWidth?.(c.id));
    if (head !== undefined) need.width = Math.max(need.width, Math.ceil(head));
    const top = measured(measures.memberTop?.(c.id));
    if (top !== undefined) need.height = Math.max(need.height, Math.ceil(top) + CONTAINER_PADDING);
    for (const m of list) {
      const at = out[m.id]?.position ?? { x: 0, y: 0 };
      const size = isContainerKind(m.kind)
        ? extent(m)
        : { width: CARD_W, height: Math.ceil(cardHeight(m.id) ?? CARD_H) };
      need = {
        width: Math.max(need.width, at.x + size.width + CONTAINER_PADDING),
        height: Math.max(need.height, at.y + size.height + CONTAINER_PADDING),
      };
    }
    // FLO-FR-OGPF: a milestone column has no stored size.
    const kept = columns.has(c.id) ? undefined : storedSize(stored, c.id);
    const size = kept
      ? {
          width: Math.max(kept.width, list.length === 0 ? MIN_CONTAINER_SIZE.width : need.width),
          height: Math.max(kept.height, list.length === 0 ? MIN_CONTAINER_SIZE.height : need.height),
        }
      : need;
    out[c.id] = {
      ...out[c.id],
      position: out[c.id]?.position ?? { x: 0, y: 0 },
      size,
      ...(stored.items[c.id]?.collapsed === true ? { collapsed: true } : {}),
    };
  };

  placeMembers(undefined);
  // An item inside a containment cycle is reached by no walk from the top.
  for (const item of doc.items) {
    if (!out[item.id]) {
      if (isContainerKind(item.kind)) sizeContainer(item);
      out[item.id] = { ...out[item.id], position: out[item.id]?.position ?? { x: 0, y: 0 } };
    }
  }
  return out;
}

/**
 * FLO-FR-SQUS / FLO-FR-IAOQ: the one width of the columns: the width the
 * widest milestone column needs, or the No milestone column, which holds its
 * items with the padding of a container on both sides.
 */
export function columnWidth(milestones: readonly number[], others: readonly number[]): number {
  const none = others.length > 0 ? Math.max(...others) + 2 * CONTAINER_PADDING : 0;
  return Math.max(MIN_CONTAINER_SIZE.width, none, ...milestones);
}

/** FLO-FR-IAOQ: where the No milestone column starts, after `count` milestone columns. */
export function noMilestoneX(count: number, width: number): number {
  return count * (width + COLUMN_GAP);
}
