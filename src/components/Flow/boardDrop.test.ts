/** Where a drag of the board lands, and what a drop writes (`DBR-delivery-board.md`). */
import { describe, expect, it } from "vitest";

import {
  absolutePosition,
  boardOrders,
  findItem,
  resolveBoardLayout,
  subtreeOf,
  type FlowDocument,
  type FlowRect,
  type StoredLayout,
} from "../../state/flowDocument";
import { item, plan } from "../../test/flowPlanFixtures";
import { boardDropResult, boardTargetAt } from "./boardDrop";
import { AUTO_PAN_EDGE, AUTO_PAN_MARGIN, AUTO_PAN_MAX, autoPanStep, limitPan } from "./useBoardDrag";

const EMPTY: StoredLayout = { items: {} };

function board(): FlowDocument {
  return plan([
    item("m1", "milestone"),
    item("m2", "milestone"),
    item("e1", "epic", { parentId: "m1" }),
    item("f1", "feature", { parentId: "e1" }),
    item("t1", "task", { parentId: "f1" }),
    item("t2", "task", { parentId: "f1" }),
    item("t3", "task", { parentId: "m1" }),
    item("t4", "task", { parentId: "m2" }),
    item("x1", "defect"),
  ]);
}

/** The target of a drag of `id` with the pointer at `point`, on the board as it shows. */
function targetOf(doc: FlowDocument, id: string, point: { x: number; y: number }, stored = EMPTY) {
  const lifted = subtreeOf(doc, id);
  const b = resolveBoardLayout(doc, stored, { lifted });
  const collapsed = new Set(Object.keys(stored.items).filter((k) => stored.items[k].collapsed));
  const rectOf = (x: string): FlowRect | null =>
    b.layout[x]?.size ? { ...absolutePosition(doc, b.layout, x), ...b.layout[x].size! } : null;
  return boardTargetAt({
    doc,
    item: findItem(doc, id)!,
    lifted,
    point,
    orders: boardOrders(doc, stored),
    collapsed,
    none: b.none,
    rectOf,
    hidden: () => false,
  });
}

const middleOf = (doc: FlowDocument, id: string, lifted: string) => {
  const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set([lifted]) });
  const at = absolutePosition(doc, b.layout, id);
  const size = b.layout[id].size!;
  return { x: at.x + size.width / 2, y: at.y + size.height / 2, top: at.y, bottom: at.y + size.height };
};

describe("the target of a drag (DBR-FR-CZEU, DBR-FR-EEKD)", () => {
  it("DBR-FR-CZEU: the innermost stack under the pointer takes the drop, before the first member whose middle is below the pointer", () => {
    const doc = board();
    const t2 = middleOf(doc, "t2", "t4");
    expect(targetOf(doc, "t4", { x: t2.x, y: t2.y - 1 }).drop).toEqual({ holder: "f1", index: 1, collapsed: false });
    expect(targetOf(doc, "t4", { x: t2.x, y: t2.y + 1 }).drop).toEqual({ holder: "f1", index: 2, collapsed: false });
    const t1 = middleOf(doc, "t1", "t4");
    expect(targetOf(doc, "t4", { x: t1.x, y: t1.top + 2 }).drop).toEqual({ holder: "f1", index: 0, collapsed: false });
  });

  it("DBR-FR-CZEU: a stack whose holder cannot hold the kind passes the drop to the stack that holds it", () => {
    const doc = board();
    // An epic over the feature's stack lands in the milestone column.
    const doc2 = plan([...doc.items, item("e2", "epic", { parentId: "m2" })]);
    const t1 = middleOf(doc2, "t1", "e2");
    const target = targetOf(doc2, "e2", { x: t1.x, y: t1.y });
    expect(target.drop?.holder).toBe("m1");
    expect(target.hint).not.toBeNull();
  });

  it("DBR-FR-CZEU, DBR-FR-EEKD: over a top-level feature, an epic gets the hint and the No milestone column takes the drop", () => {
    const doc = plan([...board().items, item("f9", "feature")]);
    const b = resolveBoardLayout(doc, EMPTY, { lifted: subtreeOf(doc, "e1") });
    const at = absolutePosition(doc, b.layout, "f9");
    const target = targetOf(doc, "e1", { x: at.x + 20, y: at.y + 20 });
    expect(target.drop?.holder).toBeNull();
    expect(target.hint).toBe("An epic can sit only at the top level or in a milestone.");
  });

  it("DBR-FR-CZEU: the No milestone column takes a drop as the top level", () => {
    const doc = board();
    const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set(["t4"]) });
    const target = targetOf(doc, "t4", { x: b.none!.x + 20, y: 1 });
    expect(target.drop).toEqual({ holder: null, index: 0, collapsed: false });
  });

  it("DBR-FR-CZEU: a collapsed group takes the item at the end of its stack", () => {
    const doc = board();
    const stored: StoredLayout = { items: { f1: { collapsed: true } } };
    const b = resolveBoardLayout(doc, stored, { lifted: new Set(["t4"]) });
    const f1 = absolutePosition(doc, b.layout, "f1");
    const target = targetOf(doc, "t4", { x: f1.x + 20, y: f1.y + 10 }, stored);
    expect(target.drop).toEqual({ holder: "f1", index: 2, collapsed: true });
  });

  it("DBR-FR-CZEU, DBR-FR-SBLK: a group that holds no item takes the item as the first of its stack", () => {
    const doc = plan([...board().items, item("f9", "feature", { parentId: "m2" })]);
    const b = resolveBoardLayout(doc, EMPTY, { lifted: new Set(["t4"]) });
    const f9 = absolutePosition(doc, b.layout, "f9");
    const target = targetOf(doc, "t4", { x: f9.x + 20, y: f9.y + 10 });
    expect(target.drop).toEqual({ holder: "f9", index: 0, collapsed: false });
  });

  it("DBR-FR-EEKD: a point on open canvas has no target and no hint", () => {
    const doc = board();
    expect(targetOf(doc, "t4", { x: -500, y: -500 })).toEqual({ drop: null, hint: null });
  });

  it("DBR-FR-EEKD: a stack that cannot hold the kind, with no holder around it that can, gives the hint and no target", () => {
    // A milestone dragged over a column: a milestone sits only at the top level.
    const doc = board();
    const t3 = middleOf(doc, "t3", "m2");
    const target = targetOf(doc, "m2", { x: t3.x, y: t3.y });
    expect(target.drop).toBeNull();
    expect(target.hint).toBe("A milestone can sit only at the top level of the plan.");
  });

  it("DBR-FR-QCAJ: the dragged item and what it holds are never a target", () => {
    const doc = board();
    // Over the face of the dragged epic and over its feature, the column now shows t3.
    const b = resolveBoardLayout(doc, EMPTY);
    const e1 = absolutePosition(doc, b.layout, "e1");
    const f1 = absolutePosition(doc, b.layout, "f1");
    const target = targetOf(doc, "e1", { x: f1.x + 10, y: e1.y + 10 });
    expect(target.drop?.holder).toBe("m1");
  });
});

describe("the result of a drop (DBR-FR-KTJS)", () => {
  it("DBR-FR-KTJS: a drop in the same stack writes the new order and is no move", () => {
    const doc = board();
    const orders = boardOrders(doc, EMPTY);
    const r = boardDropResult(doc, doc, EMPTY, orders, "t2", { holder: "f1", index: 0, collapsed: false });
    expect(r.moves).toBe(false);
    expect(r.parentId).toBe("f1");
    const ys = ["t2", "t1"].map((id) => r.stored!.items[id].position!.y);
    expect(ys[0]).toBeLessThan(ys[1]);
    expect(boardOrders(doc, r.stored!).get("f1")).toEqual(["t2", "t1"]);
    expect(r.touched.sort()).toEqual(["t1", "t2"]);
  });

  it("DBR-FR-KTJS: a drop at the slot the item already has changes nothing", () => {
    const doc = board();
    const r = boardDropResult(doc, doc, EMPTY, boardOrders(doc, EMPTY), "t1", { holder: "f1", index: 0, collapsed: false });
    expect(r.stored).toBeNull();
    expect(r.touched).toEqual([]);
  });

  it("DBR-FR-KTJS: a drop into another stack is a move, and the slot positions of both stacks are written", () => {
    const doc = board();
    const after = plan(doc.items.map((i) => (i.id === "t1" ? { ...i, parentId: "m2" } : i)));
    const stored: StoredLayout = { items: { x1: { position: { x: 5, y: 5 } } } };
    const r = boardDropResult(doc, after, stored, boardOrders(doc, stored), "t1", {
      holder: "m2",
      index: 0,
      collapsed: false,
    });
    expect(r.moves).toBe(true);
    expect(r.parentId).toBe("m2");
    expect(r.touched.sort()).toEqual(["t1", "t2", "t4"]);
    expect(boardOrders(after, r.stored!).get("m2")).toEqual(["t1", "t4"]);
    expect(r.stored!.items.x1).toEqual({ position: { x: 5, y: 5 } });
  });

  it("DBR-FR-KTJS: a drop into the No milestone column moves the item to the top level", () => {
    const doc = board();
    const r = boardDropResult(doc, doc, EMPTY, boardOrders(doc, EMPTY), "t4", { holder: null, index: 1, collapsed: false });
    expect(r.moves).toBe(true);
    expect(r.parentId).toBeUndefined();
  });
});

describe("the pan near an edge (DBR-FR-IQOM)", () => {
  const rect = { left: 100, top: 50, width: 800, height: 600 };

  it("DBR-FR-IQOM: away from the edges the view does not pan", () => {
    expect(autoPanStep({ x: 500, y: 350 }, rect)).toEqual({ dx: 0, dy: 0 });
  });

  it("DBR-FR-IQOM: near an edge the view pans toward it, faster as the pointer comes nearer", () => {
    const nearRight = autoPanStep({ x: 900 - AUTO_PAN_EDGE / 2, y: 350 }, rect);
    const atRight = autoPanStep({ x: 899, y: 350 }, rect);
    expect(nearRight.dx).toBeLessThan(0);
    expect(atRight.dx).toBeLessThan(nearRight.dx);
    expect(atRight.dx).toBeGreaterThanOrEqual(-AUTO_PAN_MAX);
    expect(autoPanStep({ x: 101, y: 350 }, rect).dx).toBeGreaterThan(0);
    expect(autoPanStep({ x: 500, y: 51 }, rect).dy).toBeGreaterThan(0);
    expect(autoPanStep({ x: 500, y: 649 }, rect).dy).toBeLessThan(0);
  });

  it("DBR-FR-IQOM: the pan stops before the board leaves the canvas", () => {
    const extent = { width: 2000, height: 900 };
    const canvas = { width: 800, height: 600 };
    // The far right edge of the board is 40px from where the pan stops.
    const view = { x: 800 - AUTO_PAN_MARGIN - 2000 + 40, y: 0, scale: 1 };
    expect(limitPan({ dx: -18, dy: 0 }, view, extent, canvas).dx).toBe(-18);
    expect(limitPan({ dx: -18, dy: 0 }, { ...view, x: view.x - 30 }, extent, canvas).dx).toBe(-10);
    expect(limitPan({ dx: -18, dy: 0 }, { ...view, x: view.x - 40 }, extent, canvas).dx).toBe(0);
    // Toward the start, the pan stops once the board starts the margin inside the canvas.
    expect(limitPan({ dx: 18, dy: 0 }, { x: AUTO_PAN_MARGIN - 5, y: 0, scale: 1 }, extent, canvas).dx).toBe(5);
    expect(limitPan({ dx: 0, dy: 18 }, { x: 0, y: AUTO_PAN_MARGIN, scale: 1 }, extent, canvas).dy).toBe(0);
  });
});
