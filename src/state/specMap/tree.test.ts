import { describe, expect, it } from "vitest";
import {
  ancestorAtDepth,
  ancestorIds,
  buildTree,
  isWithin,
  plannedUnder,
  roleOf,
  specsUnder,
} from "./tree";
import { EMPTY_ROLLUP, rollupAll, rollupTotal, segments } from "./rollups";
import { applyEdit } from "./organize";
import { LEVELS_4, node, smallIndex, spec } from "../../test/specMapFixtures";

describe("the index tree (SMP-FR-FEJV)", () => {
  it("SMP-FR-FEJV: a 4-level index has root nodes, branch nodes and leaf groups by depth", () => {
    const tree = buildTree(smallIndex());
    expect(tree.leafDepth).toBe(2);
    expect(roleOf(tree, "d1")).toBe("root");
    expect(roleOf(tree, "f1")).toBe("branch");
    expect(roleOf(tree, "g1")).toBe("leafGroup");
    expect(tree.specs.size).toBe(6);
    expect(tree.specParent.get("B1")).toBe("g3");
    expect(tree.specByPath.get("specifications/core/A2-a2.md")).toBe("A2");
    expect(tree.order).toEqual(["d1", "f1", "g1", "g2", "f2", "g3", "d2", "f3", "g4"]);
  });

  it("SMP-FR-FEJV: 3-level and 5-level indexes keep every spec node at one depth", () => {
    const three = buildTree({
      levels: [LEVELS_4[0], LEVELS_4[2], LEVELS_4[3]],
      roots: [node("r", [node("g", [spec("X1", "built")])])],
      dependencies: [],
    });
    expect(three.leafDepth).toBe(1);
    expect(roleOf(three, "g")).toBe("leafGroup");

    const five = buildTree({
      levels: [...LEVELS_4.slice(0, 3), { plural: "parts", singular: "part" }, LEVELS_4[3]],
      roots: [node("r", [node("b1", [node("b2", [node("g", [spec("Y1", "gap")])])])])],
      dependencies: [],
    });
    expect(five.leafDepth).toBe(3);
    expect(roleOf(five, "b2")).toBe("branch");
    expect(five.depth.get(five.specParent.get("Y1")!)).toBe(five.leafDepth);
  });

  it("finds the ancestor of a ref at a depth, and nothing shallower than the ref", () => {
    const tree = buildTree(smallIndex());
    expect(ancestorAtDepth(tree, { kind: "spec", code: "A3" }, 1)).toBe("f1");
    expect(ancestorAtDepth(tree, { kind: "spec", code: "A3" }, 0)).toBe("d1");
    expect(ancestorAtDepth(tree, { kind: "index", id: "d2" }, 1)).toBeNull();
    expect(ancestorIds(tree, "g4")).toEqual(["d2", "f3"]);
    expect(isWithin(tree, { kind: "spec", code: "C1" }, "d2")).toBe(true);
    expect(isWithin(tree, { kind: "spec", code: "C1" }, "d1")).toBe(false);
    expect(specsUnder(tree, "f1").map((s) => s.code)).toEqual(["A1", "A2", "A3"]);
  });
});

describe("roll-ups (SMN-FR-OJAY)", () => {
  it("SMN-FR-OJAY: counts an index node from the spec nodes beneath it", () => {
    const tree = buildTree(smallIndex());
    const all = rollupAll(tree);
    expect(all.get("d1")).toEqual({
      specs: 4,
      requirements: 28,
      scenarios: 14,
      verified: 1,
      built: 1,
      drafted: 1,
      gap: 1,
    });
    expect(segments(all.get("d1")!)).toEqual({ verified: 25, built: 25, drafted: 25, gap: 25 });
    expect(rollupTotal(tree).specs).toBe(6);
  });

  it("SMN-FR-OJAY: an empty node has no share rather than a division by zero", () => {
    expect(segments(EMPTY_ROLLUP)).toEqual({ verified: 0, built: 0, drafted: 0, gap: 0 });
  });

  it("SMN-FR-OJAY: a move changes the counts of both the old and the new holder", () => {
    const moved = applyEdit(smallIndex(), {
      kind: "move",
      ref: { kind: "spec", code: "A2" },
      parentId: "g4",
      index: 0,
    });
    const all = rollupAll(buildTree(moved));
    expect(all.get("d1")!.gap).toBe(0);
    expect(all.get("d2")!.gap).toBe(1);
    expect(all.get("d2")!.specs).toBe(3);
  });

  it("SMD-FR-ULTF: counts planned drafts on a node and beneath it", () => {
    const planned = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "g1",
      draft: { draftId: "x", name: "Untitled" },
    });
    const tree = buildTree(planned);
    expect(plannedUnder(tree, "d1")).toBe(1);
    expect(plannedUnder(tree, "d2")).toBe(0);
  });

  it("SMD-FR-NQGE: a planned draft adds nothing to spec counts, totals, bars or dependencies", () => {
    const before = buildTree(smallIndex());
    const planned = buildTree(
      applyEdit(
        applyEdit(smallIndex(), { kind: "attachDraft", nodeId: "g1", draft: { draftId: "x", name: "One" } }),
        { kind: "attachDraft", nodeId: "d2", draft: { draftId: "y", name: "Two" } },
      ),
    );
    expect(rollupAll(planned)).toEqual(rollupAll(before));
    expect(rollupTotal(planned)).toEqual(rollupTotal(before));
    expect(planned.specs.size).toBe(before.specs.size);
    expect(planned.index.dependencies).toEqual(before.index.dependencies);
  });
});
