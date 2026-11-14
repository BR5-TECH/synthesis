/**
 * The automatic layout of a plan (FLO-FR-PLUZ, FLO-FR-HBBF).
 */
import { describe, expect, it } from "vitest";

import {
  CARD_H,
  CARD_W,
  CONTAINER_PADDING,
  DEFAULT_CONTAINER_SIZES,
  MEMBER_ORIGIN,
  resolveLayout,
  type FlowLayout,
} from "./flowDocument";
import { blocks, item, layoutFor, plan, storedLayout } from "../test/flowPlanFixtures";

const xOf = (layout: FlowLayout, id: string) => layout[id].position.x;
const yOf = (layout: FlowLayout, id: string) => layout[id].position.y;

/** Whether `a` comes before `b` in reading order: by row, then left to right. */
const readsBefore = (layout: FlowLayout, a: string, b: string) =>
  yOf(layout, a) < yOf(layout, b) || (yOf(layout, a) === yOf(layout, b) && xOf(layout, a) < xOf(layout, b));

/** Whether two placed rectangles share any area. */
function overlap(layout: FlowLayout, a: string, b: string): boolean {
  const ra = { ...layout[a].position, ...(layout[a].size ?? { width: CARD_W, height: CARD_H }) };
  const rb = { ...layout[b].position, ...(layout[b].size ?? { width: CARD_W, height: CARD_H }) };
  return (
    ra.x < rb.x + rb.width &&
    rb.x < ra.x + ra.width &&
    ra.y < rb.y + rb.height &&
    rb.y < ra.y + ra.height
  );
}

describe("resolveLayout (FLO-FR-PLUZ)", () => {
  it("FLO-FR-PLUZ, FLO-FR-SQUS, FLO-FR-MCFR: top-level milestones are columns, left to right in the milestone order, tops aligned", () => {
    const doc = plan([
      item("m1", "milestone"),
      item("m2", "milestone", { due: "2027-06-30" }),
      item("m3", "milestone", { due: "2027-03-31" }),
      item("m4", "milestone"),
    ]);
    const layout = layoutFor(doc);
    // m2 and m3 swap by due date; the undated m1 and m4 keep their places.
    const xs = ["m1", "m3", "m2", "m4"].map((id) => xOf(layout, id));
    expect(xs).toEqual([...xs].sort((a, b) => a - b));
    expect(new Set(xs).size).toBe(4);
    expect(new Set(["m1", "m2", "m3", "m4"].map((id) => yOf(layout, id)))).toEqual(new Set([0]));
  });

  it("FLO-FR-PLUZ, FLO-FR-IAOQ: the other top-level items stack in the last column, after the milestones, containers first", () => {
    const doc = plan([item("t1", "task"), item("m1", "milestone"), item("f1", "feature")]);
    const layout = layoutFor(doc);
    const columnEnd = xOf(layout, "m1") + layout.m1.size!.width;
    expect(xOf(layout, "t1")).toBeGreaterThan(columnEnd);
    expect(xOf(layout, "f1")).toBe(xOf(layout, "t1"));
    // Containers before leaves, top to bottom.
    expect(yOf(layout, "f1")).toBeLessThan(yOf(layout, "t1"));
    expect(yOf(layout, "t1")).toBeGreaterThanOrEqual(yOf(layout, "f1") + layout.f1.size!.height);
  });

  it("FLO-FR-PLUZ: inside a container, containers come before leaves and document order decides the rest", () => {
    const doc = plan([
      item("m1", "milestone"),
      item("t1", "task", { parentId: "m1" }),
      item("t2", "task", { parentId: "m1" }),
      item("f1", "feature", { parentId: "m1" }),
    ]);
    const layout = layoutFor(doc);
    expect(layout.f1.position).toEqual(MEMBER_ORIGIN);
    expect(readsBefore(layout, "f1", "t1")).toBe(true);
    expect(readsBefore(layout, "t1", "t2")).toBe(true);
  });

  it("FLO-FR-PLUZ: a member comes after each member it waits for", () => {
    // t1 waits for t3 through e1, and t3 waits for t2 through e2.
    const doc = plan(
      [
        item("f1", "feature"),
        item("t1", "task", { parentId: "f1" }),
        item("t2", "task", { parentId: "f1" }),
        item("t3", "task", { parentId: "f1" }),
      ],
      [blocks("e1", "t3", "t1"), blocks("e2", "t2", "t3")],
    );
    const layout = layoutFor(doc);
    expect(readsBefore(layout, "t2", "t3")).toBe(true);
    expect(readsBefore(layout, "t3", "t1")).toBe(true);
  });

  it("FLO-FR-PLUZ: a member waits for a sibling container that holds what blocks it", () => {
    const doc = plan(
      [
        item("m1", "milestone"),
        item("t9", "task", { parentId: "m1" }),
        item("f1", "feature", { parentId: "m1" }),
        item("f2", "feature", { parentId: "m1" }),
        item("t1", "task", { parentId: "f2" }),
      ],
      [blocks("e1", "t1", "f1")],
    );
    const layout = layoutFor(doc);
    // f1 waits for f2, which holds t1.
    expect(readsBefore(layout, "f2", "f1")).toBe(true);
    expect(readsBefore(layout, "f1", "t9")).toBe(true);
  });

  it("FLO-FR-PLUZ: no two members of a container overlap, also with many members", () => {
    const leaves = Array.from({ length: 9 }, (_, i) => item(`t${i}`, "task", { parentId: "f1" }));
    const layout = layoutFor(plan([item("f1", "feature"), ...leaves]));
    for (const a of leaves) {
      for (const b of leaves) if (a !== b) expect(overlap(layout, a.id, b.id), `${a.id} ${b.id}`).toBe(false);
    }
  });

  it("FLO-FR-PLUZ: a stored position is kept, and an item without one goes below the placed ones", () => {
    const doc = plan([
      item("f1", "feature"),
      item("t1", "task", { parentId: "f1" }),
      item("t2", "task", { parentId: "f1" }),
    ]);
    const layout = layoutFor(doc, storedLayout({ t1: { x: 300, y: 200 } }));
    expect(layout.t1.position).toEqual({ x: 300, y: 200 });
    expect(yOf(layout, "t2")).toBeGreaterThanOrEqual(200 + CARD_H);
  });

  it("FLO-FR-PLUZ: a stored collapsed flag is kept", () => {
    const doc = plan([item("f1", "feature"), item("t1", "task", { parentId: "f1" })]);
    const layout = layoutFor(doc, storedLayout({ f1: { x: 0, y: 0, collapsed: true } }));
    expect(layout.f1.collapsed).toBe(true);
    expect(layout.t1.collapsed).toBeUndefined();
  });

  it("FLO-FR-PLUZ: an entry that is not finite is not a position", () => {
    const doc = plan([item("t1", "task")]);
    const layout = layoutFor(doc, { items: { t1: { position: { x: Number.NaN, y: 4 } } } });
    expect(Number.isFinite(layout.t1.position.x)).toBe(true);
  });

  it("FLO-FR-PLUZ: a wait cycle among siblings ends, and every member gets a place", () => {
    const doc = plan(
      [item("f1", "feature"), item("t1", "task", { parentId: "f1" }), item("t2", "task", { parentId: "f1" })],
      [blocks("e1", "t1", "t2"), blocks("e2", "t2", "t1")],
    );
    const layout = layoutFor(doc);
    expect(overlap(layout, "t1", "t2")).toBe(false);
  });

  it("FLO-FR-PLUZ: an item in a containment cycle gets a finite place", () => {
    const doc = plan([
      item("f1", "feature", { parentId: "f2" }),
      item("f2", "feature", { parentId: "f1" }),
    ]);
    const layout = layoutFor(doc);
    for (const id of ["f1", "f2"]) {
      expect(Number.isFinite(layout[id].position.x), id).toBe(true);
      expect(Number.isFinite(layout[id].position.y), id).toBe(true);
      expect(layout[id].size, id).toBeDefined();
    }
  });

  it("FLO-FR-PLUZ: an item whose parent is not a container still gets a place", () => {
    const doc = plan([item("t1", "task"), item("t2", "task", { parentId: "t1" })]);
    const layout = layoutFor(doc);
    expect(Number.isFinite(layout.t2.position.x)).toBe(true);
    expect(Number.isFinite(layout.t2.position.y)).toBe(true);
  });
});

describe("resolveLayout (FLO-FR-HBBF)", () => {
  it("FLO-FR-HBBF: a container without a stored size takes the size its members need", () => {
    const doc = plan([
      item("f1", "feature"),
      item("t1", "task", { parentId: "f1" }),
      item("t2", "task", { parentId: "f1" }),
    ]);
    const layout = layoutFor(doc);
    const right = xOf(layout, "t2") + CARD_W + CONTAINER_PADDING;
    const bottom = yOf(layout, "t2") + CARD_H + CONTAINER_PADDING;
    expect(layout.f1.size).toEqual({ width: right, height: bottom });
  });

  it("FLO-FR-HBBF: a container holds a member container with that container's own size", () => {
    const doc = plan([
      item("m1", "milestone"),
      item("f1", "feature", { parentId: "m1" }),
      item("t1", "task", { parentId: "f1" }),
    ]);
    const layout = layoutFor(doc);
    expect(layout.m1.size!.width).toBe(xOf(layout, "f1") + layout.f1.size!.width + CONTAINER_PADDING);
  });

  it("FLO-FR-HBBF: an empty container without a stored size takes the size of its kind", () => {
    const layout = layoutFor(plan([item("ep1", "epic")]));
    expect(layout.ep1.size).toEqual(DEFAULT_CONTAINER_SIZES.epic);
  });

  it("FLO-FR-HBBF: a stored size is kept where it holds the members", () => {
    const doc = plan([item("f1", "feature"), item("t1", "task", { parentId: "f1" })]);
    const layout = layoutFor(
      doc,
      storedLayout({ f1: { x: 0, y: 0, width: 900, height: 600 }, t1: { x: 20, y: 60 } }),
    );
    expect(layout.f1.size).toEqual({ width: 900, height: 600 });
  });
});

describe("resolveLayout with measured sizes (FLO-FR-PLUZ, FLO-FR-HBBF, MDL-FR-ECPZ)", () => {
  const doc = plan([
    item("m1", "milestone"),
    item("f1", "feature", { parentId: "m1" }),
    item("t1", "task", { parentId: "f1" }),
    item("t2", "task", { parentId: "f1" }),
    item("t3", "task", { parentId: "f1" }),
    item("t4", "task", { parentId: "f1" }),
    item("t5", "task", { parentId: "f1" }),
  ]);
  const TALL = 190;

  it("MDL-FR-ECPZ, FLO-FR-HBBF: with no measure, or a measure of 0, the layout is the one of the floors", () => {
    const floors = resolveLayout(doc, { items: {} });
    expect(resolveLayout(doc, { items: {} }, { cardHeight: () => 0, headWidth: () => 0, memberTop: () => 0 })).toEqual(floors);
  });

  it("MDL-FR-ECPZ, FLO-FR-PLUZ: tall cards get rows tall enough that no two cards overlap, and their container holds them", () => {
    const layout = resolveLayout(doc, { items: {} }, { cardHeight: () => TALL });
    const sized = (id: string) => ({ ...layout[id], size: layout[id].size ?? { width: CARD_W, height: TALL } });
    const tall: FlowLayout = Object.fromEntries(doc.items.map((i) => [i.id, sized(i.id)]));
    // t5 starts the second row of f1.
    expect(yOf(layout, "t5")).toBeGreaterThanOrEqual(yOf(layout, "t1") + TALL);
    for (const [a, b] of [["t1", "t5"], ["t2", "t5"], ["t1", "t2"]]) expect(overlap(tall, a, b)).toBe(false);
    expect(layout.f1.size!.height).toBeGreaterThanOrEqual(yOf(layout, "t5") + TALL + CONTAINER_PADDING);
    expect(layout.m1.size!.height).toBeGreaterThanOrEqual(yOf(layout, "f1") + layout.f1.size!.height + CONTAINER_PADDING);
  });

  it("MDL-FR-ECPZ: a container is as wide as its header needs to show its chips", () => {
    const layout = resolveLayout(doc, { items: {} }, { headWidth: (id) => (id === "m1" ? 2000 : undefined) });
    expect(layout.m1.size!.width).toBe(2000);
    expect(layout.f1.size!.width).toBe(resolveLayout(doc, { items: {} }).f1.size!.width);
  });

  it("MDL-FR-ECPZ, FLO-FR-HBBF, FLO-FR-SQUS: a nested container is as wide as its header, its column grows to hold it, and every column takes that width", () => {
    const wide = plan([
      item("m1", "milestone"),
      item("f1", "feature", { parentId: "m1" }),
      item("t1", "task", { parentId: "f1" }),
      item("m2", "milestone"),
    ]);
    const head = 640;
    const layout = resolveLayout(wide, { items: {} }, { headWidth: (id) => (id === "f1" ? head : undefined) });
    expect(layout.f1.size!.width).toBe(head);
    const f1Right = layout.f1.position.x + layout.f1.size!.width;
    expect(layout.m1.size!.width).toBe(f1Right + CONTAINER_PADDING);
    expect(layout.m2.size!.width).toBe(layout.m1.size!.width);
  });

  it("MDL-FR-PUCM, FLO-FR-HBBF: the members of a container start below the content above them, an error line or references", () => {
    const layout = resolveLayout(doc, { items: {} }, { memberTop: (id) => (id === "f1" ? 90 : undefined) });
    expect(yOf(layout, "t1")).toBe(90 + CONTAINER_PADDING);
    expect(xOf(layout, "t1")).toBe(MEMBER_ORIGIN.x);
    // A smaller measure than the header keeps the origin of the header.
    const low = resolveLayout(doc, { items: {} }, { memberTop: () => 10 });
    expect(yOf(low, "t1")).toBe(MEMBER_ORIGIN.y);
  });

  it("FLO-FR-HBBF: a stored position stays where it is whatever is measured", () => {
    const stored = { items: { t1: { position: { x: 300, y: 400 } } } };
    const layout = resolveLayout(doc, stored, { cardHeight: () => TALL, memberTop: () => 90 });
    expect(layout.t1.position).toEqual({ x: 300, y: 400 });
  });
});
