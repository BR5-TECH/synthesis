import { describe, expect, it } from "vitest";
import { buildTree } from "../../state/specMap/tree";
import { applyEdit } from "../../state/specMap/organize";
import {
  columnsFor,
  holderRect,
  layoutMap,
  levelFor,
  scopeOf,
  type MapLayout,
} from "./layout";
import { LEVELS_4, node, smallIndex, spec } from "../../test/specMapFixtures";

const rel = (layout: MapLayout, id: string, from: string) => {
  const a = layout.box.get(id) ?? layout.chip.get(id)!;
  const b = layout.box.get(from)!;
  return { dx: a.x - b.x, dy: a.y - b.y };
};

function layoutOf(level: number, width = 1200, index = smallIndex(), focus: string | null = null) {
  const tree = buildTree(index);
  return layoutMap(scopeOf(tree, focus), level, width);
}

describe("the overview (SMN-FR-XCOK)", () => {
  it("SMN-FR-XCOK: the column count follows the canvas width", () => {
    expect(columnsFor(1020)).toBe(3);
    expect(columnsFor(1019)).toBe(2);
    expect(columnsFor(660)).toBe(2);
    expect(columnsFor(659)).toBe(1);
  });

  it("SMN-FR-XCOK: root nodes are 300 × 176 cards, 30px apart, centred on the content", () => {
    const layout = layoutOf(0, 1200);
    const d1 = layout.box.get("d1")!;
    const d2 = layout.box.get("d2")!;
    expect([d1.w, d1.h, d1.shape]).toEqual([300, 176, "overview"]);
    expect(d2.x - d1.x).toBe(330);
    expect(d2.y).toBe(d1.y);
    expect([layout.width, layout.height]).toEqual([630, 176]);
    expect(d1.x).toBe(-315);
    expect(layout.boxes).toHaveLength(2);
  });

  it("SMN-FR-XCOK: in one column the n-th root stacks below the previous one", () => {
    const layout = layoutOf(0, 500);
    expect(rel(layout, "d2", "d1")).toEqual({ dx: 0, dy: 206 });
  });
});

describe("expanded levels (SMZ-FR-RNIT, SMN-FR-TNDA)", () => {
  it("SMZ-FR-RNIT, SMN-FR-JEQO, SMN-FR-PFYW: level 1 expands roots and collapses depth 1 at 116px", () => {
    const layout = layoutOf(1, 1200);
    const d1 = layout.box.get("d1")!;
    expect([d1.shape, d1.w]).toEqual(["expanded", 360]);
    expect(layout.box.get("f1")).toMatchObject({ shape: "collapsed", w: 340, h: 116 });
    expect(rel(layout, "f1", "d1")).toEqual({ dx: 10, dy: 46 });
    expect(rel(layout, "f2", "d1")).toEqual({ dx: 10, dy: 172 });
    expect(d1.h).toBe(304);
    expect(rel(layout, "d2", "d1")).toEqual({ dx: 394, dy: 0 });
    expect(layout.box.has("g1")).toBe(false);
    expect(layout.chips).toHaveLength(0);
  });

  it("SMN-FR-TNDA, SMN-FR-UJRG: level 2 collapses leaf groups at 64px, 52px under their branch and 8px apart", () => {
    const layout = layoutOf(2);
    expect(layout.box.get("g1")).toMatchObject({ shape: "collapsed", w: 320, h: 64 });
    expect(rel(layout, "g1", "f1")).toEqual({ dx: 10, dy: 52 });
    expect(rel(layout, "g2", "f1")).toEqual({ dx: 10, dy: 124 });
    expect(layout.box.get("f1")!.h).toBe(204);
    expect(layout.chips).toHaveLength(0);
  });

  it("SMN-FR-ISBE, SMZ-FR-RNIT: the last level expands leaf groups with their spec chips", () => {
    const layout = layoutOf(3);
    const g1 = layout.box.get("g1")!;
    expect([g1.shape, g1.h]).toEqual(["expanded", 96]);
    expect(layout.chip.get("A1")).toMatchObject({ kind: "spec", w: 96, h: 22 });
    expect(rel(layout, "A1", "g1")).toEqual({ dx: 10, dy: 62 });
    expect(rel(layout, "A2", "g1")).toEqual({ dx: 112, dy: 62 });
    expect(layout.box.get("f1")!.h).toBe(268);
    expect(layout.box.get("d1")!.h).toBe(504);
  });

  it("SMN-FR-ISBE: a fourth chip starts a new row 28px lower", () => {
    const index = smallIndex();
    const layout = layoutOf(
      3,
      1200,
      applyEdit(
        applyEdit(index, { kind: "move", ref: { kind: "spec", code: "C1" }, parentId: "g1", index: 2 }),
        { kind: "move", ref: { kind: "spec", code: "C2" }, parentId: "g1", index: 3 },
      ),
    );
    expect(rel(layout, "C2", "g1")).toEqual({ dx: 10, dy: 90 });
    expect(layout.box.get("g1")!.h).toBe(124);
  });

  it("SMN-FR-TNDA: a 5-level index widens every level by 20px per depth", () => {
    const five = {
      levels: [...LEVELS_4.slice(0, 3), { plural: "parts", singular: "part" }, LEVELS_4[3]],
      roots: [node("r", [node("b1", [node("b2", [node("g", [spec("Y1", "gap")])])])])],
      dependencies: [],
    };
    const layout = layoutOf(4, 1200, five);
    expect(["r", "b1", "b2", "g"].map((id) => layout.box.get(id)!.w)).toEqual([380, 360, 340, 320]);
    expect(layout.chip.has("Y1")).toBe(true);
  });
});

describe("planned chips (SMD-FR-CGNW, SMD-FR-ULTF)", () => {
  const planned = () =>
    applyEdit(
      applyEdit(smallIndex(), { kind: "attachDraft", nodeId: "g1", draft: { draftId: "p1", name: "One" } }),
      { kind: "attachDraft", nodeId: "f2", draft: { draftId: "p2", name: "Two" } },
    );

  it("SMD-FR-CGNW: in a leaf group, planned chips follow the spec chips in the same grid", () => {
    const layout = layoutOf(3, 1200, planned());
    expect(layout.chip.get("p1")).toMatchObject({ kind: "planned", w: 96, h: 22 });
    expect(rel(layout, "p1", "g1")).toEqual({ dx: 214, dy: 62 });
  });

  it("SMD-FR-ULTF: an expanded branch places planned chips after its children and grows", () => {
    const layout = layoutOf(2, 1200, planned());
    // f2 holds one collapsed group (64px): 52 + 64 + 8, then one chip row.
    expect(rel(layout, "p2", "f2")).toEqual({ dx: 10, dy: 124 });
    expect(layout.box.get("f2")!.h).toBe(124 + 28 + 8);
    // Collapsed at level 1, the chip does not render.
    expect(layoutOf(1, 1200, planned()).chip.has("p2")).toBe(false);
  });
});

describe("the focused subtree (SMZ-FR-FOXU)", () => {
  it("SMZ-FR-FOXU: a focused node is the only root, and the levels start at its depth", () => {
    const tree = buildTree(smallIndex());
    const scope = scopeOf(tree, "f1");
    expect(scope.roots.map((r) => r.id)).toEqual(["f1"]);
    expect(scope.levels.map((l) => l.plural)).toEqual(["features", "groups", "specs"]);
    const overview = layoutMap(scope, 0, 1200);
    expect(overview.boxes.map((b) => b.id)).toEqual(["f1"]);
    expect(overview.box.get("f1")).toMatchObject({ shape: "overview", w: 300, h: 176 });
    const expanded = layoutMap(scope, 1, 1200);
    expect(expanded.box.get("f1")!.w).toBe(340);
    expect(expanded.box.get("g1")).toMatchObject({ shape: "collapsed", h: 64 });
    expect(expanded.box.has("d2")).toBe(false);
  });

  it("SMI-FR-RPCO: a node's own level is its depth in the scope, and a chip's is the last level", () => {
    const tree = buildTree(smallIndex());
    const whole = scopeOf(tree, null);
    expect(levelFor(tree, whole, { kind: "index", id: "g4" })).toBe(2);
    expect(levelFor(tree, whole, { kind: "spec", code: "B1" })).toBe(3);
    const focused = scopeOf(tree, "d1");
    expect(levelFor(tree, focused, { kind: "index", id: "f2" })).toBe(1);
    expect(levelFor(tree, focused, { kind: "spec", code: "C1" })).toBeNull();
  });

  it("SMZ-FR-EPTR: the holder of a ref is its chip, or the deepest rendered box above it", () => {
    const tree = buildTree(smallIndex());
    const scope = scopeOf(tree, null);
    expect(holderRect(tree, layoutMap(scope, 3, 1200), { kind: "spec", code: "A3" })?.key).toBe("chip:A3");
    expect(holderRect(tree, layoutMap(scope, 1, 1200), { kind: "spec", code: "A3" })?.key).toBe("box:f1");
  });
});
