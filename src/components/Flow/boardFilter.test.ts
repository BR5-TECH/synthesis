/** The status filter of the board (`MDL-map-delivery-lens.md` MDL-FR-WDMP, MDL-FR-ZKXX). */
import { describe, expect, it } from "vitest";

import { item, plan } from "../../test/flowPlanFixtures";
import { filterIsEmpty, NO_OMITTED, omittedItems, SHOW_ALL, type BoardStatusFilter } from "./boardFilter";
import type { BoardDot } from "./boardStatus";

/**
 * Beta holds an epic with a feature of two tasks, and a task; the epic e2
 * holds nothing; x1 is a top-level defect; g1 is a top-level epic of one task.
 */
function doc() {
  return plan([
    item("m1", "milestone"),
    item("e1", "epic", { parentId: "m1" }),
    item("f1", "feature", { parentId: "e1" }),
    item("t1", "task", { parentId: "f1" }),
    item("t2", "task", { parentId: "f1" }),
    item("t3", "task", { parentId: "m1" }),
    item("e2", "epic", { parentId: "m1" }),
    item("x1", "defect"),
    item("g1", "epic"),
    item("t4", "task", { parentId: "g1" }),
  ]);
}

const DOTS: Record<string, BoardDot | null> = {
  m1: "in progress",
  e1: "in progress",
  f1: "in progress",
  t1: "done",
  t2: "in progress",
  t3: "done",
  e2: "done",
  x1: "backlog",
  g1: "ready",
  t4: "ready",
};

function omitted(
  filter: BoardStatusFilter,
  extra: { dots?: Record<string, BoardDot | null>; blocked?: string[]; keep?: string[] } = {},
) {
  const dots = { ...DOTS, ...extra.dots };
  const blocked = new Set(extra.blocked ?? []);
  return [
    ...omittedItems({
      doc: doc(),
      filter,
      dotOf: (id) => dots[id] ?? null,
      blockedOf: (id) => blocked.has(id),
      keep: new Set(extra.keep ?? []),
    }),
  ].sort();
}

describe("omittedItems (MDL-FR-WDMP, MDL-FR-ZKXX)", () => {
  it("MDL-FR-WDMP: the empty filter hides nothing", () => {
    expect(filterIsEmpty(SHOW_ALL)).toBe(true);
    expect(filterIsEmpty(undefined)).toBe(true);
    const out = omittedItems({ doc: doc(), filter: SHOW_ALL, dotOf: () => "done", blockedOf: () => false, keep: new Set() });
    expect(out).toBe(NO_OMITTED);
  });

  it("MDL-FR-WDMP: an item whose status is hidden does not show, and an item whose status is pressed shows", () => {
    expect(omitted({ hidden: ["done"], blockedOnly: false })).toEqual(["e2", "t1", "t3"]);
  });

  it("MDL-FR-WDMP: a group shows where a descendant shows, also when its own status is hidden", () => {
    // g1 is ready, and so is t4. Hide ready: both go.
    expect(omitted({ hidden: ["ready"], blockedOnly: false })).toEqual(["g1", "t4"]);
    // The task shows, so its group shows with it.
    expect(omitted({ hidden: ["ready"], blockedOnly: false }, { dots: { t4: "backlog" } })).toEqual([]);
  });

  it("MDL-FR-WDMP: a group shows where it passes the filter, also when its members are all hidden", () => {
    expect(omitted({ hidden: ["done"], blockedOnly: false }, { dots: { t2: "done" } })).toEqual([
      "e2",
      "t1",
      "t2",
      "t3",
    ]);
  });

  it("MDL-FR-WDMP: each ancestor of a shown item shows, even where every ancestor's status is hidden", () => {
    expect(omitted({ hidden: ["in progress", "ready", "backlog", "in review"], blockedOnly: false })).toEqual([
      "g1",
      "t2",
      "t4",
      "x1",
    ]);
  });

  it("MDL-FR-WDMP: Blocked only keeps the blocked items whose status is pressed, and their ancestors", () => {
    expect(omitted({ hidden: [], blockedOnly: true }, { blocked: ["t2"] })).toEqual([
      "e2",
      "g1",
      "t1",
      "t3",
      "t4",
      "x1",
    ]);
    // A blocked item whose status is hidden does not show.
    expect(omitted({ hidden: ["in progress"], blockedOnly: true }, { blocked: ["t2", "x1"] })).toEqual([
      "e1",
      "e2",
      "f1",
      "g1",
      "t1",
      "t2",
      "t3",
      "t4",
    ]);
  });

  it("MDL-FR-WDMP: a milestone and an item with no status dot always show", () => {
    const all: BoardStatusFilter = { hidden: ["backlog", "ready", "in progress", "in review", "done"], blockedOnly: true };
    const out = omitted(all, { dots: { m1: "done", x1: null } });
    expect(out).not.toContain("m1");
    expect(out).not.toContain("x1");
    expect(out).toEqual(["e1", "e2", "f1", "g1", "t1", "t2", "t3", "t4"]);
  });

  it("MDL-FR-ZKXX: a kept item shows, and its ancestors show with it", () => {
    const out = omitted({ hidden: ["done", "in progress"], blockedOnly: false }, { keep: ["t1"] });
    expect(out).not.toContain("t1");
    expect(out).not.toContain("f1");
    expect(out).not.toContain("e1");
    expect(out).toEqual(["e2", "t2", "t3"]);
  });

  it("MDL-FR-WDMP: a filter that hides nothing of the plan gives the shared empty set", () => {
    expect(
      omittedItems({ doc: doc(), filter: { hidden: ["in review"], blockedOnly: false }, dotOf: (id) => DOTS[id] ?? null, blockedOf: () => false, keep: new Set() }),
    ).toBe(NO_OMITTED);
  });
});
