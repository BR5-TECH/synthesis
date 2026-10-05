import { describe, expect, it } from "vitest";
import { buildTree } from "../../state/specMap/tree";
import { applyEdit } from "../../state/specMap/organize";
import type { MapRef, SpecificationIndex } from "../../state/specMap/types";
import { layoutMap, scopeOf } from "./layout";
import { hitTestDrop } from "./dnd";
import { smallIndex } from "../../test/specMapFixtures";

function setup(level: number, index: SpecificationIndex = smallIndex()) {
  const tree = buildTree(index);
  const layout = layoutMap(scopeOf(tree, null), level, 1200);
  const at = (id: string, dx: number, dy: number) => {
    const b = layout.box.get(id) ?? layout.chip.get(id)!;
    return { x: b.x + dx, y: b.y + dy };
  };
  const drop = (ref: MapRef, point: { x: number; y: number }) => hitTestDrop(tree, layout, ref, point);
  return { tree, layout, at, drop };
}

describe("drop targets (SMO-FR-WGOF)", () => {
  it("SMO-FR-WGOF: a spec chip lands on the leaf group under the pointer, not on the boxes around it", () => {
    const { at, drop } = setup(3);
    const target = drop({ kind: "spec", code: "A1" }, at("g4", 20, 70));
    expect(target).toMatchObject({ parentId: "g4", highlightId: "g4" });
    // Over the feature box but outside any group: no valid target.
    expect(drop({ kind: "spec", code: "A1" }, at("f3", 5, 20))).toBeNull();
  });

  it("SMO-FR-WGOF: an index node lands only on a node one level up", () => {
    const { at, drop } = setup(2);
    expect(drop({ kind: "index", id: "g1" }, at("f3", 30, 60))).toMatchObject({ parentId: "f3" });
    // Over d2's header, outside f3: d2 is two levels up.
    expect(drop({ kind: "index", id: "g1" }, at("d2", 30, 10))).toBeNull();
  });

  it("SMO-FR-WGOF: no node of the dragged subtree is a target, so a node over its own child lands one level up", () => {
    const { at, drop } = setup(2);
    // f1 over its own group g1: neither g1 nor f1 itself takes the drop.
    const target = drop({ kind: "index", id: "f1" }, at("g1", 30, 30));
    expect(target).toMatchObject({ parentId: "d1", highlightId: "d1" });
    // A group over itself: its own box is not a target either.
    expect(drop({ kind: "index", id: "g2" }, at("g2", 30, 30))).toMatchObject({ parentId: "f1" });
  });

  it("SMO-FR-WGOF, SMD-FR-QPAM: a planned chip lands on any index node, at the end of its placements", () => {
    const index = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "g1",
      draft: { draftId: "p", name: "Plan" },
    });
    const { at, drop } = setup(1, index);
    expect(drop({ kind: "planned", draftId: "p" }, at("f3", 20, 20))).toMatchObject({
      parentId: "f3",
      index: 0,
      bar: null,
    });
  });

  it("SMO-FR-WGOF: a root node lands only among the root nodes", () => {
    const { at, drop } = setup(0);
    expect(drop({ kind: "index", id: "d1" }, at("d2", 150, 150))).toMatchObject({
      parentId: null,
      index: 1,
      highlightId: null,
    });
    expect(drop({ kind: "index", id: "d1" }, at("d2", 150, 10))).toMatchObject({ parentId: null, index: 0 });
  });
});

describe("drop position (SMO-FR-ARQV)", () => {
  it("SMO-FR-ARQV: the position among a stack is the number of siblings above the pointer", () => {
    const { at, drop } = setup(2);
    expect(drop({ kind: "index", id: "g2" }, at("g1", 30, 10))).toMatchObject({ parentId: "f1", index: 0 });
    expect(drop({ kind: "index", id: "g2" }, at("g1", 30, 60))).toMatchObject({ parentId: "f1", index: 1 });
  });

  it("SMO-FR-ARQV: the position in a chip grid is the cell under the pointer, and the bar marks it", () => {
    const { at, drop } = setup(3);
    const first = drop({ kind: "spec", code: "A3" }, at("g4", 12, 64));
    expect(first).toMatchObject({ parentId: "g4", index: 0 });
    expect(first!.bar).toMatchObject({ w: 2, h: 22 });
    expect(drop({ kind: "spec", code: "A3" }, at("g4", 120, 64))).toMatchObject({ index: 1 });
    // Past the last chip, the position is the end of the list.
    expect(drop({ kind: "spec", code: "A3" }, at("g4", 300, 90))).toMatchObject({ index: 2 });
  });
});
