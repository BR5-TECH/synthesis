/**
 * EDT-FR-QJJP, EDT-FR-DUSC: the hint sets against the bundled Mermaid version.
 *
 * This file uses the real Mermaid library, not the stand-in of the other
 * diagram tests: the list of chart types and the check that each example
 * parses must come from the version that the application bundles.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { HINT_SETS, RICH_SETS, hintSetFor } from "./index";

type MermaidApi = (typeof import("mermaid"))["default"];

/**
 * The id of each chart type that the bundled Mermaid version registers. Each
 * detector of the library is a section of its core bundle that starts with
 * its source path and declares its id.
 */
function bundledChartTypeIds(): string[] {
  const bundle = readFileSync(
    join(process.cwd(), "node_modules", "mermaid", "dist", "mermaid.core.mjs"),
    "utf8",
  );
  const ids = new Set<string>();
  const detector = /\/\/ src\/diagrams\/[^\n]*\n(?:[^\n]*\n){0,2}?var id\d* = "([^"]+)";/g;
  for (const match of bundle.matchAll(detector)) ids.add(match[1]);
  return [...ids].sort();
}

const RICH_KEYS = [
  "flowchart",
  "sequence",
  "class",
  "state",
  "er",
  "gantt",
  "pie",
  "mindmap",
  "timeline",
  "journey",
];

/** What the bundled Mermaid version says about the example of one hint set. */
interface ExampleCheck {
  /** The parse error, or null when the example parses. */
  error: string | null;
  /** The chart type that Mermaid detects in the example. */
  detected: string | null;
}

const checks = new Map<string, ExampleCheck>();

// Mermaid loads the chunk of each chart type at its first parse, and that load
// takes real time. All of the loads happen here, under one generous limit, so
// no single test depends on how fast the runner is. The tests below only read
// the results.
beforeAll(async () => {
  const mermaid: MermaidApi = (await import("mermaid")).default;
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
  for (const set of HINT_SETS) {
    let error: string | null = null;
    try {
      if (!(await mermaid.parse(set.example))) error = "no parse result";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
    let detected: string | null = null;
    try {
      detected = mermaid.detectType(set.example);
    } catch (cause) {
      error ??= cause instanceof Error ? cause.message : String(cause);
    }
    checks.set(set.key, { error, detected });
  }
}, 300_000);

describe("hint sets against the bundled Mermaid version", () => {
  const ids = bundledChartTypeIds();

  it("EDT-FR-QJJP: the bundled version lists its chart types", () => {
    // A change of the bundle layout must not make this check empty.
    expect(ids.length).toBeGreaterThan(20);
    expect(ids).toContain("flowchart-v2");
    expect(ids).toContain("sequence");
  });

  it("EDT-FR-QJJP: each chart type of the bundled version has a hint set", () => {
    const covered = new Set(HINT_SETS.flatMap((set) => set.mermaidIds));
    expect(ids.filter((id) => !covered.has(id))).toEqual([]);
  });

  it("EDT-FR-QJJP: each id that a hint set names is a chart type of the bundled version", () => {
    const known = new Set(ids);
    const unknown = HINT_SETS.flatMap((set) =>
      set.mermaidIds.filter((id) => !known.has(id)).map((id) => `${set.key}:${id}`),
    );
    expect(unknown).toEqual([]);
  });

  it("EDT-FR-QJJP: the keys of the hint sets are unique", () => {
    const keys = HINT_SETS.map((set) => set.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("EDT-FR-DUSC: the ten most important chart types have rich sets", () => {
    expect(RICH_SETS.map((set) => set.key).sort()).toEqual([...RICH_KEYS].sort());
  });

  for (const set of HINT_SETS) {
    it(`EDT-FR-QJJP: the ${set.key} set has its declaration, its main element forms, and an example`, () => {
      expect(set.declaration.code.trim()).not.toBe("");
      expect(set.declaration.text.trim()).not.toBe("");
      const hints = set.groups.flatMap((group) => group.hints);
      expect(hints.length).toBeGreaterThanOrEqual(2);
      for (const hint of hints) {
        expect(hint.code.trim()).not.toBe("");
        expect(hint.text.trim()).not.toBe("");
      }
      expect(set.example.split("\n").length).toBeGreaterThanOrEqual(2);
    });

    it(`EDT-FR-QJJP, EDT-FR-NWZX: the ${set.key} example parses as its own chart type`, () => {
      const check = checks.get(set.key);
      expect(check?.error).toBeNull();
      expect(set.mermaidIds).toContain(check?.detected);
      expect(hintSetFor(set.example)?.key).toBe(set.key);
      expect(hintSetFor(set.declaration.code)?.key).toBe(set.key);
    });
  }

  for (const set of RICH_SETS) {
    it(`EDT-FR-DUSC: the ${set.key} set is rich`, () => {
      expect(set.groups.length).toBeGreaterThanOrEqual(5);
      const hints = set.groups.flatMap((group) => group.hints);
      expect(hints.length).toBeGreaterThanOrEqual(12);
      expect(hints.some((hint) => hint.code.trimStart().startsWith("%%"))).toBe(true);
    });
  }
});
