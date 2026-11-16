import { describe, expect, it } from "vitest";

import type { FlowDocument } from "../../state/flowDocument";
import type { ItemDeliveryState } from "../../types";
import { leafRollup } from "./Rollup";

const doc = (items: Array<{ id: string; kind: string; parentId?: string }>) =>
  ({ version: 3, items: items.map((i) => ({ name: i.id, ...i })), edges: [] }) as unknown as FlowDocument;

const state = (itemId: string, s: ItemDeliveryState["state"], blocked = false) =>
  ({ itemId, state: s, blocked, blockedBy: [], draft: null, draftMissing: false, run: null, leafCounts: null }) as ItemDeliveryState;

describe("leafRollup (MDL-FR-GDUP, FDL-FR-KWMT)", () => {
  it("MDL-FR-GDUP, FDL-FR-KWMT: the counts are of work items: a task, and a container that holds no item", () => {
    const plan = doc([
      { id: "m1", kind: "milestone" },
      { id: "e1", kind: "epic", parentId: "m1" },
      { id: "f1", kind: "feature", parentId: "e1" },
      { id: "f2", kind: "feature", parentId: "m1" },
      { id: "t1", kind: "task", parentId: "f2" },
    ]);
    const states = new Map(
      [
        state("m1", "in_progress"),
        state("e1", "not_started", true),
        state("f1", "not_started", true),
        state("f2", "done"),
        state("t1", "done"),
      ].map((s) => [s.itemId, s]),
    );
    const rollup = leafRollup(plan, states)!;
    expect(rollup.leaves).toBe(2);
    expect(rollup.counts.done).toBe(1);
    expect(rollup.counts.not_started).toBe(1);
    expect(rollup.blocked).toBe(1);
  });

  it("MDL-FR-GDUP: no plan or no states gives no roll-up", () => {
    expect(leafRollup(null, new Map())).toBeNull();
    expect(leafRollup(doc([]), null)).toBeNull();
  });

  it("MDL-FR-GDUP: a blocked item counts in the blocked segment alone, and the legend counts every item in progress", () => {
    const plan = doc([
      { id: "a", kind: "task" },
      { id: "b", kind: "task" },
      { id: "c", kind: "task" },
      { id: "d", kind: "task" },
      { id: "e", kind: "task" },
    ]);
    const states = new Map(
      [
        state("a", "in_progress", true),
        state("b", "in_progress"),
        state("c", "drafting", true),
        state("d", "done"),
        state("e", "not_started"),
      ].map((s) => [s.itemId, s]),
    );
    const rollup = leafRollup(plan, states)!;
    expect(rollup.segments).toEqual({ done: 1, active: 1, blocked: 2 });
    expect(rollup.counts.in_progress).toBe(2);
    expect(rollup.blocked).toBe(2);
  });
});
