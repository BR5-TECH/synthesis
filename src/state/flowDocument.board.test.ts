/** The board layout of the Delivery lens (`DBR-delivery-board.md`). */
import { describe, expect, it } from "vitest";

import {
  absolutePosition,
  BOARD_CARD_H,
  BOARD_CARD_W,
  BOARD_EMPTY_H,
  BOARD_FACE_H,
  BOARD_GAP,
  BOARD_HEAD_H,
  BOARD_INSET,
  boardOrders,
  COLUMN_GAP,
  resolveBoardLayout,
  toBoardStoredLayout,
  type FlowDocument,
  type StoredLayout,
} from "./flowDocument";
import { blocks, item, plan } from "../test/flowPlanFixtures";

const EMPTY: StoredLayout = { items: {} };
/** DBR-FR-SBLK: the top of the stack of a group: its face, the divider, and the inset. */
const STACK = BOARD_FACE_H + 1 + BOARD_INSET;

/** Beta holds an epic with a feature of two tasks, and a task; GA holds a task. */
function board(): FlowDocument {
  return plan([
    item("m1", "milestone"),
    item("m2", "milestone"),
    item("e1", "epic", { parentId: "m1" }),
    item("f1", "feature", { parentId: "e1" }),
    item("t1", "task", { parentId: "f1", name: "A name that is much longer than one line of a card can hold" }),
    item("t2", "task", { parentId: "f1" }),
    item("t3", "task", { parentId: "m1" }),
    item("t4", "task", { parentId: "m2" }),
    item("x1", "defect"),
  ]);
}

const rectOf = (doc: FlowDocument, layout: ReturnType<typeof resolveBoardLayout>["layout"], id: string) => {
  const at = absolutePosition(doc, layout, id);
  return { ...at, ...layout[id].size! };
};

describe("columns, groups, and stacks (DBR-FR-ZGNB, DBR-FR-IQBN, DBR-FR-FISH, DBR-FR-LNZI)", () => {
  it("DBR-FR-ZGNB: every column has one width, the tops align, and No milestone is the last column", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY);
    const m1 = b.layout.m1;
    const m2 = b.layout.m2;
    expect(m1.position).toEqual({ x: 0, y: 0 });
    expect(m2.position).toEqual({ x: b.columnWidth + COLUMN_GAP, y: 0 });
    expect(m1.size!.width).toBe(b.columnWidth);
    expect(m2.size!.width).toBe(b.columnWidth);
    expect(b.none).toMatchObject({ x: 2 * (b.columnWidth + COLUMN_GAP), y: 0, width: b.columnWidth });
  });

  it("DBR-FR-ZGNB: the column width is what the deepest nesting needs around a card of the board width", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY);
    // Two groups (an epic and a feature) around t1.
    expect(b.layout.t1.size!.width).toBe(BOARD_CARD_W);
    expect(b.columnWidth).toBe(BOARD_CARD_W + 2 * BOARD_INSET * 2 + 2 * BOARD_INSET);
    const flat = resolveBoardLayout(plan([item("m1", "milestone"), item("t1", "task", { parentId: "m1" })]), EMPTY);
    expect(flat.columnWidth).toBe(BOARD_CARD_W + 2 * BOARD_INSET);
  });

  it("DBR-FR-IQBN, DBR-FR-LNZI: a group fills the inner width of its stack, a card fills its stack, and a card with no measure has the default height", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY);
    const inner = b.columnWidth - 2 * BOARD_INSET;
    expect(b.layout.e1.size!.width).toBe(inner);
    expect(b.layout.t3.size!.width).toBe(inner);
    expect(b.layout.f1.size!.width).toBe(inner - 2 * BOARD_INSET);
    expect(b.layout.t1.size!.width).toBe(inner - 4 * BOARD_INSET);
    for (const id of ["t1", "t2", "t3", "t4", "x1"]) expect(b.layout[id].size!.height).toBe(BOARD_CARD_H);
  });

  it("DBR-FR-FISH: a stack puts its members one below the other with one gap, and no member overlaps another", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY);
    expect(b.layout.e1.position).toEqual({ x: BOARD_INSET, y: BOARD_HEAD_H });
    expect(b.layout.f1.position).toEqual({ x: BOARD_INSET, y: STACK });
    expect(b.layout.t1.position).toEqual({ x: BOARD_INSET, y: STACK });
    expect(b.layout.t2.position).toEqual({ x: BOARD_INSET, y: STACK + BOARD_CARD_H + BOARD_GAP });
    // The group comes before the card in the automatic order (FLO-FR-PLUZ).
    const e1 = rectOf(doc, b.layout, "e1");
    const t3 = rectOf(doc, b.layout, "t3");
    expect(t3.y).toBe(e1.y + e1.height + BOARD_GAP);
    expect(t3.x).toBe(e1.x);
    // The group holds its stack, its inset, and its bottom border.
    const t2 = rectOf(doc, b.layout, "t2");
    const f1 = rectOf(doc, b.layout, "f1");
    expect(f1.y + f1.height).toBe(t2.y + t2.height + BOARD_INSET + 1);
  });

  it("DBR-FR-SBLK: a group that holds no item is its face alone, and an empty column keeps room for a drop", () => {
    const doc = plan([item("m1", "milestone"), item("e1", "epic", { parentId: "m1" }), item("m2", "milestone")]);
    const b = resolveBoardLayout(doc, EMPTY);
    expect(b.layout.e1.size!.height).toBe(BOARD_FACE_H + 1);
    expect(b.layout.m2.size!.height).toBe(BOARD_HEAD_H + BOARD_EMPTY_H + BOARD_INSET);
  });

  it("DBR-FR-SBLK, DBR-FR-CZEU: a group that holds no item opens its stack for the placeholder of a drag", () => {
    const doc = plan([item("m1", "milestone"), item("e1", "epic", { parentId: "m1" }), item("f9", "feature", { parentId: "m1" })]);
    const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set(["f9"]), gap: { holder: "e1", index: 0, height: 50 } });
    expect(b.layout.e1.size!.height).toBe(STACK + 50 + BOARD_INSET + 1);
    expect(b.gapRect).toEqual({
      x: b.layout.e1.position.x + BOARD_INSET,
      y: b.layout.e1.position.y + STACK,
      width: b.layout.e1.size!.width - 2 * BOARD_INSET,
      height: 50,
    });
  });

  it("DBR-FR-SBLK: a collapsed group takes the height of its face alone", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, { items: { f1: { collapsed: true } } });
    expect(b.layout.f1.size!.height).toBe(BOARD_FACE_H + 1);
    expect(b.layout.f1.collapsed).toBe(true);
  });

  it("DBR-FR-LNZI, DBR-FR-SBTR, DBR-FR-BYQX: a measured card, face, or column header takes its measured height; a measure that is not positive is ignored", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { heights: { t1: 104, t2: 0, f1: 90, m1: 70, t3: Number.NaN } });
    expect(b.layout.t1.size!.height).toBe(104);
    expect(b.layout.t2.size!.height).toBe(BOARD_CARD_H);
    expect(b.layout.t3.size!.height).toBe(BOARD_CARD_H);
    // The stack of f1 starts below its measured face, and t2 below the taller t1.
    expect(b.layout.t1.position.y).toBe(90 + 1 + BOARD_INSET);
    expect(b.layout.t2.position.y).toBe(90 + 1 + BOARD_INSET + 104 + BOARD_GAP);
    // The stack of m1 starts below its measured header.
    expect(b.layout.e1.position.y).toBe(70);
    expect(b.layout.t4.position.y).toBe(BOARD_HEAD_H);
  });
});

describe("the stack order (DBR-FR-HTCE, DBR-FR-BKWE)", () => {
  it("DBR-FR-HTCE: the stored positions give the order, smallest vertical position first, and the rest follow in the automatic order", () => {
    const doc = plan([
      item("m1", "milestone"),
      item("a", "task", { parentId: "m1" }),
      item("b", "task", { parentId: "m1" }),
      item("c", "task", { parentId: "m1" }),
      item("d", "task", { parentId: "m1" }),
    ]);
    const stored: StoredLayout = {
      items: { c: { position: { x: 900, y: 10 } }, a: { position: { x: 0, y: 500 } } },
    };
    expect(boardOrders(doc, stored).get("m1")).toEqual(["c", "a", "b", "d"]);
    const b = resolveBoardLayout(doc, stored);
    expect(b.layout.c.position.y).toBeLessThan(b.layout.a.position.y);
    expect(b.layout.c.position.x).toBe(BOARD_INSET);
  });

  it("DBR-FR-HTCE: equal stored positions sort by x and then by plan order, and a position that is not finite counts as none", () => {
    const doc = plan([
      item("m1", "milestone"),
      item("a", "task", { parentId: "m1" }),
      item("b", "task", { parentId: "m1" }),
      item("c", "task", { parentId: "m1" }),
      item("d", "task", { parentId: "m1" }),
    ]);
    const stored: StoredLayout = {
      items: {
        b: { position: { x: 0, y: 10 } },
        a: { position: { x: 0, y: 10 } },
        c: { position: { x: -5, y: 10 } },
        d: { position: { x: 0, y: Number.NaN } },
      },
    };
    expect(boardOrders(doc, stored).get("m1")).toEqual(["c", "a", "b", "d"]);
  });

  it("DBR-FR-BYQX, DBR-FR-BKWE: a collapsed column takes the height of its header and stores its flag with no position", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, { items: { m1: { collapsed: true } } });
    expect(b.layout.m1.size!.height).toBe(BOARD_HEAD_H + 1);
    expect(toBoardStoredLayout(doc, b.layout).items.m1).toEqual({ collapsed: true });
  });

  it("DBR-FR-ZGNB: an empty plan has no No milestone column", () => {
    expect(resolveBoardLayout(plan([]), EMPTY).none).toBeNull();
  });

  it("DBR-FR-HTCE: the automatic order puts a member after each member it waits for", () => {
    const doc = plan(
      [item("m1", "milestone"), item("a", "task", { parentId: "m1" }), item("b", "task", { parentId: "m1" })],
      [blocks("e", "b", "a")],
    );
    expect(boardOrders(doc, EMPTY).get("m1")).toEqual(["b", "a"]);
  });

  it("DBR-FR-BKWE: a stored size is not read, and the stored layout holds no group size and no column position", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, { items: { e1: { size: { width: 999, height: 999 } } } });
    expect(b.layout.e1.size!.width).not.toBe(999);
    const stored = toBoardStoredLayout(doc, b.layout);
    expect(stored.items.e1).toEqual({ position: b.layout.e1.position });
    expect(stored.items.t1).toEqual({ position: b.layout.t1.position });
    expect(stored.items.m1).toBeUndefined();
    const collapsed = toBoardStoredLayout(doc, { ...b.layout, m1: { ...b.layout.m1, collapsed: true } });
    expect(collapsed.items.m1).toEqual({ collapsed: true });
  });
});

describe("a drag of the board (DBR-FR-QCAJ, DBR-FR-CZEU)", () => {
  it("DBR-FR-QCAJ: a lifted item takes no slot, and the members below it move up", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set(["t1"]) });
    expect(b.layout.t2.position.y).toBe(STACK);
  });

  it("DBR-FR-CZEU: the placeholder opens at its slot, the members after it move down, and its rectangle is in canvas units", () => {
    const doc = board();
    const gap = { holder: "m2", index: 0, height: BOARD_CARD_H };
    const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set(["t1"]), gap });
    expect(b.layout.t4.position.y).toBe(BOARD_HEAD_H + BOARD_CARD_H + BOARD_GAP);
    expect(b.gapRect).toEqual({
      x: b.layout.m2.position.x + BOARD_INSET,
      y: BOARD_HEAD_H,
      width: b.columnWidth - 2 * BOARD_INSET,
      height: BOARD_CARD_H,
    });
  });

  it("DBR-FR-CZEU: a placeholder at the end of a nested stack has the coordinates of that stack", () => {
    const doc = board();
    const gap = { holder: "f1", index: 2, height: 30 };
    const b = resolveBoardLayout(doc, EMPTY, { gap });
    const t2 = rectOf(doc, b.layout, "t2");
    expect(b.gapRect).toEqual({ x: t2.x, y: t2.y + t2.height + BOARD_GAP, width: t2.width, height: 30 });
  });

  it("DBR-FR-CZEU: a placeholder in the No milestone column sits in that column", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { gap: { holder: null, index: 0, height: BOARD_CARD_H } });
    expect(b.gapRect?.x).toBe(b.none!.x + BOARD_INSET);
    expect(b.layout.x1.position.y).toBe(BOARD_HEAD_H + BOARD_CARD_H + BOARD_GAP);
  });

  it("DBR-FR-HTCE: an order given for a stack replaces the stored order of that stack alone", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { orders: new Map([["f1", ["t2", "t1"]]]) });
    expect(b.orders.get("f1")).toEqual(["t2", "t1"]);
    expect(b.layout.t2.position.y).toBeLessThan(b.layout.t1.position.y);
    expect(b.orders.get("m1")).toEqual(["e1", "t3"]);
  });
});

describe("hidden items of the board (DBR-FR-SKSO, FLO-FR-OHZB)", () => {
  it("DBR-FR-SKSO: a hidden item takes no slot, its stack closes up, and the stack order does not change", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { omitted: new Set(["t1"]) });
    expect(b.layout.t2.position.y).toBe(STACK);
    expect(b.orders.get("f1")).toEqual(["t1", "t2"]);
    // The hidden item has no slot of its own.
    expect(b.layout.t1.size).toBeUndefined();
  });

  it("DBR-FR-SKSO: a group whose members are all hidden shows its face alone", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { omitted: new Set(["t1", "t2"]) });
    expect(b.layout.f1.size!.height).toBe(BOARD_FACE_H + 1);
    const full = resolveBoardLayout(doc, EMPTY);
    expect(b.layout.e1.size!.height).toBeLessThan(full.layout.e1.size!.height);
  });

  it("DBR-FR-SKSO: a stack whose members are all hidden keeps the room of an empty stack", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { omitted: new Set(["t4"]) });
    expect(b.layout.m2.size!.height).toBe(BOARD_HEAD_H + BOARD_EMPTY_H + BOARD_INSET);
  });

  it("DBR-FR-SKSO: the members after a hidden member in a stored order keep their order", () => {
    const doc = board();
    const full = resolveBoardLayout(doc, EMPTY);
    const stored = toBoardStoredLayout(doc, full.layout);
    // Store the order t3 before e1 in m1, then hide e1.
    stored.items.t3 = { position: { x: 0, y: -10 } };
    const b = resolveBoardLayout(doc, stored, { omitted: new Set(["e1", "f1", "t1", "t2"]) });
    expect(b.orders.get("m1")).toEqual(["t3", "e1"]);
    expect(b.layout.t3.position.y).toBe(BOARD_HEAD_H);
  });

  it("DBR-FR-CZEU: a placeholder index counts the visible members of the stack", () => {
    const doc = board();
    const gap = { holder: "f1", index: 1, height: 30 };
    const b = resolveBoardLayout(doc, EMPTY, { omitted: new Set(["t1"]), gap });
    expect(b.gapRect?.y).toBe(rectOf(doc, b.layout, "t2").y + BOARD_CARD_H + BOARD_GAP);
  });
});
