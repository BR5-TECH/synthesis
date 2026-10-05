import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { DEMO_SPECIFICATION_INDEX } from "./fixture";
import { buildTree } from "./tree";
import { COMPLETENESS_STATES } from "./types";
import { loadSpecificationMap } from "../../api/specMap";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const tree = buildTree(DEMO_SPECIFICATION_INDEX);

describe("the demo index (SMP-FR-HWIC)", () => {
  it("SMP-FR-HWIC: every spec node names a spec file that exists", () => {
    const missing = [...tree.specs.values()]
      .filter((s) => !existsSync(resolve(process.cwd(), "specifications", s.path)))
      .map((s) => s.path);
    expect(tree.specs.size).toBeGreaterThan(100);
    expect(missing).toEqual([]);
  });

  it("SMP-FR-HWIC: every spec code is its file's code, and appears once", () => {
    const codes: string[] = [];
    const walk = (n: (typeof DEMO_SPECIFICATION_INDEX.roots)[number]) => {
      for (const c of n.children) {
        if ("code" in c) codes.push(c.code);
        else walk(c);
      }
    };
    DEMO_SPECIFICATION_INDEX.roots.forEach(walk);
    expect(new Set(codes).size).toBe(codes.length);
    for (const s of tree.specs.values()) {
      expect(s.path.split("/")[1].startsWith(`${s.code}-`)).toBe(true);
    }
  });

  it("SMP-FR-HWIC: every dependency names two spec nodes of the demo index", () => {
    const strangers = DEMO_SPECIFICATION_INDEX.dependencies.filter(
      (d) => !tree.specs.has(d.from) || !tree.specs.has(d.to),
    );
    expect(strangers).toEqual([]);
    expect(DEMO_SPECIFICATION_INDEX.dependencies.some((d) => d.unresolved)).toBe(true);
  });

  it("SMP-FR-TGYU, SMP-FR-FEJV: every spec node has one of four states, at the one spec depth", () => {
    for (const s of tree.specs.values()) {
      expect(COMPLETENESS_STATES).toContain(s.state);
      expect(tree.depth.get(tree.specParent.get(s.code)!)).toBe(tree.leafDepth);
    }
    expect(DEMO_SPECIFICATION_INDEX.levels.length).toBeGreaterThanOrEqual(3);
    expect(DEMO_SPECIFICATION_INDEX.levels.length).toBeLessThanOrEqual(5);
  });

  it("SMP-FR-QMRE: every level has a plural and a singular name", () => {
    for (const level of DEMO_SPECIFICATION_INDEX.levels) {
      expect(level.plural.length).toBeGreaterThan(0);
      expect(level.singular.length).toBeGreaterThan(0);
    }
  });

  it("SMP-FR-HWIC: the stub serves a fresh copy, so an edit to one load never reaches the next", async () => {
    const first = await loadSpecificationMap();
    first.roots[0].label = "changed";
    const second = await loadSpecificationMap();
    expect(second.roots[0].label).toBe(DEMO_SPECIFICATION_INDEX.roots[0].label);
  });
});
