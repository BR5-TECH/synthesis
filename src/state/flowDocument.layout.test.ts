/**
 * Membership, position, size, and collapse (FLO-FR-21, FLO-FR-42, FLO-FR-44,
 * FLO-FR-OAOZ, FLO-FR-TDHR, FLO-FR-ZASL, FGV-FR-YHFX).
 */
import { describe, expect, it } from "vitest";

import {
  absolutePosition,
  CARD_H,
  CARD_W,
  findItem,
  minContainerSize,
  MIN_CONTAINER_SIZE,
  placeItem,
  reparentItem,
  resizeContainer,
  setCollapsed,
  toStoredLayout,
  type FlowDocument,
  type FlowEditResult,
} from "./flowDocument";
import {
  blocks,
  item,
  layoutFor,
  relates,
  sampleLayout,
  samplePlan,
  storedLayout,
  withItems,
} from "../test/flowPlanFixtures";

function ok(r: FlowEditResult): FlowDocument {
  if (!r.ok) throw new Error(`expected the edit to be accepted, got ${r.refusal.code}`);
  return r.doc;
}

function refusal(r: FlowEditResult) {
  if (r.ok) throw new Error("expected the edit to be refused");
  return r.refusal;
}

const sample = () => layoutFor(samplePlan(), sampleLayout());

describe("reparentItem (FLO-FR-42, FLO-FR-OAOZ, FGV-FR-YHFX)", () => {
  it("FLO-FR-42: a drop into another container changes the membership and no edge", () => {
    const edges = [blocks("e1", "t1", "t3"), relates("e2", "x1", "t1")];
    const doc = ok(reparentItem(samplePlan(edges), "t1", "f2"));
    expect(findItem(doc, "t1")?.parentId).toBe("f2");
    expect(doc.edges).toEqual(edges);
  });

  it("FLO-FR-TDHR: an item that stays in its container gives back the same plan", () => {
    const doc = samplePlan();
    expect(ok(reparentItem(doc, "t1", "f1"))).toBe(doc);
    expect(ok(reparentItem(doc, "m1"))).toBe(doc);
  });

  it("FGV-FR-YHFX: a task, a defect, a feature, and an epic may sit at the top level", () => {
    for (const id of ["t1", "x1", "f2", "ep1"]) {
      const doc = ok(reparentItem(samplePlan(), id));
      expect("parentId" in findItem(doc, id)!, id).toBe(false);
    }
  });

  it("FGV-FR-YHFX: a leaf may sit in any container of a higher rank", () => {
    for (const parent of ["f1", "f2", "ep1", "m1"]) {
      expect(reparentItem(samplePlan(), "t1", parent).ok, parent).toBe(true);
      expect(reparentItem(samplePlan(), "x1", parent).ok, parent).toBe(true);
    }
  });

  it("FLO-FR-OAOZ: a container the kind does not permit is refused and names the permitted places", () => {
    expect(refusal(reparentItem(samplePlan(), "ep1", "f1"))).toEqual({
      code: "invalid_parent_kind",
      permitted: [null, "milestone"],
    });
    expect(refusal(reparentItem(samplePlan(), "m1", "ep1")).code).toBe("into_own_subtree");
    const doc = withItems(samplePlan(), item("m2", "milestone"));
    expect(refusal(reparentItem(doc, "m2", "m1"))).toEqual({
      code: "invalid_parent_kind",
      permitted: [null],
    });
    expect(refusal(reparentItem(samplePlan(), "x1", "t1")).code).toBe("invalid_parent_kind");
  });

  it("FGV-FR-13: a container cannot go into itself or into an item under it", () => {
    expect(refusal(reparentItem(samplePlan(), "ep1", "ep1")).code).toBe("into_own_subtree");
    expect(refusal(reparentItem(samplePlan(), "m1", "f1")).code).toBe("into_own_subtree");
  });

  it("FLO-FR-OAOZ, FGV-FR-KBLT: a drop that makes an edge join an item to its ancestor is refused", () => {
    const doc = samplePlan([blocks("e1", "x1", "f2"), blocks("e2", "t1", "t2")]);
    expect(refusal(reparentItem(doc, "x1", "f2"))).toEqual({
      code: "edge_within_lineage",
      edgeIds: ["e1"],
    });
  });

  it("FGV-FR-PMTY: a drop that gives the plan a dependency cycle is refused and names its items", () => {
    const doc = samplePlan([blocks("e1", "t3", "f1"), blocks("e2", "x1", "t3")]);
    expect(refusal(reparentItem(doc, "x1", "f1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["f1", "t3", "x1"],
      edgeIds: ["e1", "e2"],
    });
  });

  it("FLO-FR-42: an unknown item or container is refused", () => {
    expect(refusal(reparentItem(samplePlan(), "nope")).code).toBe("unknown_item");
    expect(refusal(reparentItem(samplePlan(), "t1", "nope")).code).toBe("unknown_parent");
  });
});

describe("placeItem (FLO-FR-21, FLO-FR-44)", () => {
  it("FLO-FR-21: a position is stored in the coordinates of the item's container", () => {
    const layout = placeItem(samplePlan(), sample(), "t1", { x: 80, y: 140 });
    expect(layout.t1.position).toEqual({ x: 60, y: 80 });
  });

  it("FLO-FR-21: a position after a drop is measured from the new container", () => {
    const doc = ok(reparentItem(samplePlan(), "t1", "f2"));
    const layout = placeItem(doc, sample(), "t1", { x: 580, y: 190 });
    expect(layout.t1.position).toEqual({ x: 100, y: 70 });
    expect(absolutePosition(doc, layout, "t1")).toEqual({ x: 580, y: 190 });
  });

  it("FLO-FR-21: moving a container moves what it holds and changes one entry", () => {
    const before = sample();
    const layout = placeItem(samplePlan(), before, "f1", { x: 120, y: 60 });
    expect(layout.f1.position).toEqual({ x: 120, y: 60 });
    expect(layout.t1).toBe(before.t1);
    expect(absolutePosition(samplePlan(), layout, "t1")).toEqual({ x: 140, y: 120 });
  });

  it("FLO-FR-44: the containers around a placed item grow to hold it", () => {
    const layout = placeItem(samplePlan(), sample(), "t1", { x: 580, y: 120 });
    // t1 at (560,60) in f1: 560 + 170 + 16.
    expect(layout.f1.size?.width).toBe(560 + CARD_W + 16);
    expect(layout.m1.size).toEqual(sample().m1.size);
  });

  it("FLO-FR-21: a place where the item already is changes nothing", () => {
    const before = sample();
    expect(placeItem(samplePlan(), before, "t1", { x: 40, y: 120 })).toBe(before);
    expect(placeItem(samplePlan(), before, "nope", { x: 1, y: 1 })).toBe(before);
  });
});

describe("minContainerSize and resizeContainer (FLO-FR-44)", () => {
  it("FLO-FR-44: a container's floor is the bounds of its members plus the padding", () => {
    expect(minContainerSize(samplePlan(), sample(), "f1")).toEqual({
      width: 220 + CARD_W + 16,
      height: 60 + CARD_H + 16,
    });
    // A member container counts with its own size: ep1 at (460,60) 500×400.
    expect(minContainerSize(samplePlan(), sample(), "m1")).toEqual({ width: 976, height: 512 });
  });

  it("FLO-FR-44: an empty container's floor is the minimum container size", () => {
    const doc = withItems(samplePlan(), item("f9", "feature", { parentId: "m1" }));
    expect(minContainerSize(doc, layoutFor(doc), "f9")).toEqual(MIN_CONTAINER_SIZE);
    expect(minContainerSize(doc, layoutFor(doc), "t1")).toEqual({ width: CARD_W, height: CARD_H });
  });

  it("FLO-FR-44: a resize below the members' bounds stops at them", () => {
    const layout = resizeContainer(samplePlan(), sample(), "f1", {
      x: 20,
      y: 60,
      width: 100,
      height: 100,
    });
    expect(layout.f1.size).toEqual(minContainerSize(samplePlan(), sample(), "f1"));
  });

  it("FLO-FR-44: a resize from the left or the top keeps the members where they are on the canvas", () => {
    const layout = resizeContainer(samplePlan(), sample(), "f1", {
      x: 10,
      y: 50,
      width: 430,
      height: 310,
    });
    expect(layout.f1.position).toEqual({ x: 10, y: 50 });
    expect(absolutePosition(samplePlan(), layout, "t1")).toEqual({ x: 40, y: 120 });
    expect(layout.f1.size).toEqual({ width: 430, height: 310 });
  });

  it("FLO-FR-44: a resize of a leaf or of an unknown item changes nothing", () => {
    const before = sample();
    const rect = { x: 0, y: 0, width: 999, height: 999 };
    expect(resizeContainer(samplePlan(), before, "t1", rect)).toBe(before);
    expect(resizeContainer(samplePlan(), before, "nope", rect)).toBe(before);
  });
});

describe("setCollapsed and toStoredLayout (FLO-FR-ZASL, DMS-FR-DMWD)", () => {
  it("FLO-FR-ZASL: a container collapses and expands in the layout", () => {
    const collapsed = setCollapsed(samplePlan(), sample(), "f1", true);
    expect(collapsed.f1.collapsed).toBe(true);
    const expanded = setCollapsed(samplePlan(), collapsed, "f1", false);
    expect("collapsed" in expanded.f1).toBe(false);
    expect(setCollapsed(samplePlan(), expanded, "f1", false)).toBe(expanded);
  });

  it("FLO-FR-ZASL: a leaf has no collapsed state", () => {
    const before = sample();
    expect(setCollapsed(samplePlan(), before, "t1", true)).toBe(before);
  });

  it("DMS-FR-DMWD: the stored layout holds a position for each item, a size for each container, and a set collapsed flag", () => {
    const stored = toStoredLayout(setCollapsed(samplePlan(), sample(), "ep1", true));
    expect(stored.items.t1).toEqual({ position: { x: 20, y: 60 } });
    expect(stored.items.f1).toEqual({
      position: { x: 20, y: 60 },
      size: { width: 420, height: 300 },
    });
    expect(stored.items.ep1.collapsed).toBe(true);
    expect("collapsed" in stored.items.f1).toBe(false);
  });

  it("FLO-FR-21: a stored layout read back gives the same layout", () => {
    const layout = sample();
    expect(layoutFor(samplePlan(), toStoredLayout(layout))).toEqual(layout);
  });

  it("FLO-FR-44: a stored size smaller than the members need is grown on read", () => {
    const stored = sampleLayout();
    stored.items.f1 = { position: { x: 20, y: 60 }, size: { width: 10, height: 10 } };
    expect(layoutFor(samplePlan(), stored).f1.size).toEqual(
      minContainerSize(samplePlan(), sample(), "f1"),
    );
  });

  it("DMS-FR-EKKY: an entry for an item the plan does not hold is ignored", () => {
    const stored = storedLayout({ gone: { x: 5, y: 5 } });
    expect("gone" in layoutFor(samplePlan(), stored)).toBe(false);
  });
});
