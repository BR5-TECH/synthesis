import { describe, expect, it } from "vitest";
import { buildTree } from "../../state/specMap/tree";
import { applyEdit } from "../../state/specMap/organize";
import type { Dependency, SpecificationIndex } from "../../state/specMap/types";
import { layoutMap, scopeOf } from "./layout";
import {
  aggregateLinks,
  anchor,
  computeEdges,
  curve,
  linkLimit,
  weightOf,
} from "./edges";
import type { MapRef } from "../../state/specMap/types";
import { smallIndex } from "../../test/specMapFixtures";

function edgesOf(
  index: SpecificationIndex,
  level: number,
  active: MapRef | null = null,
  showDeps = true,
  focus: string | null = null,
) {
  const tree = buildTree(index);
  const scope = scopeOf(tree, focus);
  const layout = layoutMap(scope, level, 1200);
  return computeEdges({ tree, layout, scope, level, active, showDeps });
}

describe("aggregation (SME-FR-FKIW)", () => {
  it("SME-FR-NQAB, SME-FR-FKIW: sums citations between the nodes at one depth and drops links inside one node", () => {
    const tree = buildTree(
      smallIndex([
        { from: "A1", to: "C1", citations: 5 },
        { from: "A2", to: "C2", citations: 3 },
        { from: "A1", to: "B1", citations: 4 },
        { from: "A1", to: "A3", citations: 9 },
      ]),
    );
    expect(aggregateLinks(tree, 0)).toEqual([
      { from: "d1", to: "d2", citations: 8 },
    ]);
    expect(aggregateLinks(tree, 1)).toEqual([
      { from: "f1", to: "f3", citations: 8 },
      { from: "f1", to: "f2", citations: 4 },
    ]);
  });

  it("SME-FR-FKIW, SME-FR-VAEC: an unresolved dependency adds to no link and draws only as its dashed edge", () => {
    const tree = buildTree(smallIndex());
    expect(aggregateLinks(tree, 0)).toEqual([{ from: "d1", to: "d2", citations: 5 }]);
    const edges = edgesOf(smallIndex(), 0);
    expect(edges.light.length + edges.medium.length + edges.heavy.length).toBe(1);
    expect(edges.unresolved).toHaveLength(1);
  });

  it("SME-FR-JRCY: aggregation reads the current tree after an organization edit", () => {
    const moved = applyEdit(smallIndex([{ from: "A1", to: "C1", citations: 5 }]), {
      kind: "move",
      ref: { kind: "spec", code: "C1" },
      parentId: "g3",
      index: 0,
    });
    expect(aggregateLinks(buildTree(moved), 1)).toEqual([
      { from: "f1", to: "f2", citations: 5 },
    ]);
    expect(aggregateLinks(buildTree(moved), 0)).toEqual([]);
  });
});

describe("limits and weights (SME-FR-LBVO, SME-FR-SGUC)", () => {
  it("SME-FR-SGUC: heavy, medium and light thresholds differ at level 0", () => {
    expect([weightOf(60, true), weightOf(59, true), weightOf(25, true), weightOf(24, true)]).toEqual([
      "heavy",
      "medium",
      "medium",
      "light",
    ]);
    expect([weightOf(30, false), weightOf(29, false), weightOf(14, false), weightOf(13, false)]).toEqual([
      "heavy",
      "medium",
      "medium",
      "light",
    ]);
  });

  it("SME-FR-LBVO: level 0 draws the 10 strongest links and other levels the 14 strongest", () => {
    expect(linkLimit(true)).toBe(10);
    expect(linkLimit(false)).toBe(14);
    // Sixteen roots with one spec each, linked in a ring whose citations are out
    // of index order and hold one tie.
    const citations = [5, 90, 30, 30, 70, 12, 44, 8, 61, 27, 3, 50, 19, 33, 2, 40];
    const n = citations.length;
    const roots = Array.from({ length: n }, (_, i) => ({
      id: `r${i}`,
      label: `r${i}`,
      summary: "",
      children: [{ id: `f${i}`, label: "", summary: "", children: [{ id: `g${i}`, label: "", summary: "", children: [
        { code: `S${i}`, path: `ui/S${i}-s.md`, label: "", summary: "", requirements: 1, scenarios: 1, state: "built" as const },
      ] }] }],
    }));
    const deps: Dependency[] = citations.map((c, i) => ({ from: `S${i}`, to: `S${(i + 1) % n}`, citations: c }));
    const index = { ...smallIndex(), roots, dependencies: deps };
    const tree = buildTree(index);
    const scope = scopeOf(tree, null);
    const rootLinks = aggregateLinks(tree, 0);
    // Most citations first, whatever the index order.
    expect(rootLinks.map((l) => l.citations)).toEqual([...citations].sort((a, b) => b - a));
    // Equal citations keep the order of the index: r2→r3 before r3→r4.
    expect(rootLinks.filter((l) => l.citations === 30).map((l) => l.from)).toEqual(["r2", "r3"]);
    const centre = (r: { x: number; y: number; w: number; h: number }) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });
    const pathOf = (layout: ReturnType<typeof layoutMap>, l: { from: string; to: string }) => {
      const a = layout.box.get(l.from)!;
      const b = layout.box.get(l.to)!;
      return curve(anchor(a, centre(b)), anchor(b, centre(a)));
    };
    const cases = [
      { level: 0, links: rootLinks, limit: 10 },
      { level: 1, links: aggregateLinks(tree, 1), limit: 14 },
    ];
    for (const { level, links, limit } of cases) {
      const layout = layoutMap(scope, level, 1200);
      const drawn = computeEdges({ tree, layout, scope, level, active: null, showDeps: true });
      const all = [...drawn.light, ...drawn.medium, ...drawn.heavy];
      expect(links.length).toBeGreaterThan(limit);
      expect(all).toHaveLength(limit);
      // The weakest link kept is drawn, and the strongest link dropped is not.
      expect(all).toContain(pathOf(layout, links[limit - 1]));
      expect(all).not.toContain(pathOf(layout, links[limit]));
    }
  });
});

describe("the active node and the geometry (SME-FR-TJMY, SME-FR-DZHE)", () => {
  it("SME-FR-TJMY: a link whose end node contains the active node draws in the active class", () => {
    const dimmed = edgesOf(smallIndex([{ from: "A1", to: "C1", citations: 5 }]), 0);
    expect(dimmed.light).toHaveLength(1);
    expect(dimmed.active).toHaveLength(0);
    const hot = edgesOf(smallIndex([{ from: "A1", to: "C1", citations: 5 }]), 0, { kind: "spec", code: "C2" });
    expect(hot.active).toHaveLength(1);
    expect(hot.light).toHaveLength(0);
  });

  it("SME-FR-DZHE: an end sits 6px outside the box, and the control point is offset by 0.11", () => {
    const box = { x: 0, y: 0, w: 100, h: 40 };
    expect(anchor(box, { x: 500, y: 20 })).toEqual({ x: 106, y: 20 });
    expect(anchor(box, { x: 50, y: 500 })).toEqual({ x: 50, y: 46 });
    expect(curve({ x: 0, y: 0 }, { x: 100, y: 0 })).toBe("M0.0 0.0Q50.0 11.0 100.0 0.0");
  });
});

describe("the last level (SME-FR-WRPX)", () => {
  it("SME-FR-WRPX: only the active spec's resolved dependencies draw, chip to chip", () => {
    const edges = edgesOf(smallIndex(), 3, { kind: "spec", code: "A1" });
    expect(edges.active).toHaveLength(1);
    expect(edges.light.length + edges.medium.length + edges.heavy.length).toBe(0);
    expect([...edges.related!].sort()).toEqual(["A1", "C1", "C2"]);
  });

  it("SME-FR-WRPX: with no active spec, the last level draws no resolved edge and dims nothing", () => {
    const edges = edgesOf(smallIndex(), 3, { kind: "index", id: "d1" });
    expect(edges.active).toHaveLength(0);
    expect(edges.related).toBeNull();
  });

  it("SME-FR-WRPX: an incoming resolved dependency of the active spec draws too", () => {
    const edges = edgesOf(smallIndex(), 3, { kind: "spec", code: "C1" });
    expect(edges.active).toHaveLength(1);
    expect([...edges.related!].sort()).toEqual(["A1", "C1"]);
  });
});

describe("unresolved citations (SME-FR-VAEC)", () => {
  it("SME-FR-VAEC: draw at every level, between the rendered nodes holding the two specs", () => {
    for (const level of [0, 1, 2, 3]) {
      expect(edgesOf(smallIndex(), level).unresolved).toHaveLength(1);
    }
  });

  it("SME-FR-VAEC: draws nothing when one rendered node holds both specs", () => {
    const index = smallIndex([{ from: "A1", to: "A2", citations: 1, unresolved: true }]);
    expect(edgesOf(index, 1).unresolved).toHaveLength(0);
    expect(edgesOf(index, 3).unresolved).toHaveLength(1);
  });
});

describe("the dependencies toggle (SME-FR-OKUH)", () => {
  it("SME-FR-OKUH: while off, no resolved dependency draws", () => {
    const off = edgesOf(smallIndex(), 0, { kind: "index", id: "d1" }, false);
    expect(off.light.length + off.medium.length + off.heavy.length + off.active.length).toBe(0);
    const lastOff = edgesOf(smallIndex(), 3, { kind: "spec", code: "A1" }, false);
    expect(lastOff.active).toHaveLength(0);
    expect(lastOff.unresolved).toHaveLength(1);
  });
});
