/**
 * The milestone order and the move of a milestone column
 * (`../../specifications/ui/FLO-flow.md` FLO-FR-MCFR, FLO-FR-OGPF,
 * FLO-FR-SQUS; `MDL-map-delivery-lens.md` MDL-FR-XJBV).
 */
import { describe, expect, it } from "vitest";

import {
  milestoneOrder,
  moveMilestone,
  orderMilestones,
  planEditsBetween,
  touchedItems,
} from "./flowDocument";
import { item, plan } from "../test/flowPlanFixtures";

const ids = (doc: ReturnType<typeof plan>) => milestoneOrder(doc).map((m) => m.id);

describe("milestoneOrder (FLO-FR-MCFR)", () => {
  it("FLO-FR-MCFR: with no due date, the order is the plan's order of its milestone items", () => {
    const doc = plan([item("m2", "milestone"), item("t1", "task"), item("m1", "milestone"), item("m3", "milestone")]);
    expect(ids(doc)).toEqual(["m2", "m1", "m3"]);
  });

  it("FLO-FR-MCFR: dated milestones take the places of the dated ones, by due date; each undated one keeps its place", () => {
    const doc = plan([
      item("a", "milestone", { due: "2027-06-30" }),
      item("u1", "milestone"),
      item("b", "milestone", { due: "2027-03-31" }),
      item("u2", "milestone"),
      item("c", "milestone", { due: "2027-01-15" }),
    ]);
    expect(ids(doc)).toEqual(["c", "u1", "b", "u2", "a"]);
  });

  it("FLO-FR-MCFR: two milestones of one due date keep the plan's order", () => {
    const doc = plan([
      item("b", "milestone", { due: "2027-03-31" }),
      item("a", "milestone", { due: "2027-03-31" }),
      item("z", "milestone", { due: "2027-01-01" }),
    ]);
    expect(ids(doc)).toEqual(["z", "b", "a"]);
  });

  it("FLO-FR-MCFR, FLO-FR-SQUS: only top-level milestones are columns; a plan with none has an empty order", () => {
    expect(ids(plan([item("t1", "task"), item("f1", "feature")]))).toEqual([]);
    expect(ids(plan([]))).toEqual([]);
  });
});

describe("moving a milestone column (FLO-FR-OGPF)", () => {
  const doc = plan([
    item("m1", "milestone"),
    item("t1", "task", { parentId: "m1" }),
    item("m2", "milestone"),
    item("x1", "defect"),
    item("m3", "milestone"),
  ]);

  it("FLO-FR-OGPF: a move puts the milestone items in the new order, in the places milestone items held, and every other item keeps its place", () => {
    const moved = moveMilestone(doc, "m3", 0);
    expect(moved.items.map((i) => i.id)).toEqual(["m3", "t1", "m1", "x1", "m2"]);
    expect(ids(moved)).toEqual(["m3", "m1", "m2"]);
    expect(moved.edges).toBe(doc.edges);
  });

  it("FLO-FR-OGPF: a move to the place the column has, or of an item that is no column, gives the same plan", () => {
    expect(moveMilestone(doc, "m2", 1)).toBe(doc);
    expect(moveMilestone(doc, "t1", 0)).toBe(doc);
    expect(moveMilestone(doc, "nope", 0)).toBe(doc);
  });

  it("FLO-FR-OGPF, FLO-FR-MCFR: a move that the due dates give back changes nothing", () => {
    const dated = plan([item("a", "milestone", { due: "2027-01-01" }), item("b", "milestone", { due: "2027-02-01" })]);
    expect(moveMilestone(dated, "b", 0)).toBe(dated);
  });

  it("FLO-FR-OGPF: orderMilestones refuses an order that does not name each column once", () => {
    expect(orderMilestones(doc, ["m1", "m2"])).toBe(doc);
    expect(orderMilestones(doc, ["m1", "m1", "m2"])).toBe(doc);
    expect(orderMilestones(doc, ["m1", "m2", "t1"])).toBe(doc);
  });

  it("FLO-FR-OGPF, MDL-FR-XJBV: in a GitHub plan the move is the edit set_milestone_order, first to last, and each column carries the pending mark", () => {
    const moved = moveMilestone(doc, "m1", 2);
    expect(planEditsBetween(doc, moved, () => "")).toEqual([
      { kind: "set_milestone_order", milestoneIds: ["m2", "m3", "m1"] },
    ]);
    expect(touchedItems(doc, moved).sort()).toEqual(["m1", "m2", "m3"]);
    // Another edit sends no order.
    expect(planEditsBetween(doc, { ...doc, items: [...doc.items, item("m4", "milestone")] }, () => "")).toEqual([
      { kind: "create_item", itemKind: "milestone", name: "M4", containerId: null },
    ]);
  });
});
