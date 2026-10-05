import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

/**
 * Structural guards on the shell's grid (`specifications/ui/SNV-shell-navigation.md`
 * SNV-FR-55, SNV-FR-44, SNV-FR-42).
 *
 * These read the stylesheet as text rather than rendering it, which needs
 * justifying. Vitest runs with `css: false` (see `vitest.config.ts`), so jsdom
 * never applies the stylesheet and no rendering test in this repo can observe a
 * computed layout. The zone adjacency SNV-FR-55 requires lives entirely in
 * `grid-template-areas`, and its failure mode is silent to every other test:
 * the DOM is identical either way, `data-active` still flips, every panel test
 * stays green — and the activity bar quietly loses its bottom third the moment
 * the bottom panel opens, taking the toggles that open it out of reach.
 *
 * This is the same bargain `test/ci-workflow.test.ts` strikes for the CI YAML:
 * a low-power change-detector is worth having where the alternative is no
 * coverage at all of a requirement whose breakage is invisible.
 */

const CSS = readStylesheet("kit.css");

/** The `grid-template-areas` block of a rule, as a list of row strings. */
function gridAreas(selector: string): string[] {
  const rule = CSS.split(selector)[1] ?? "";
  const body = rule.slice(0, rule.indexOf("}"));
  const decl = /grid-template-areas:\s*([^;]+);/.exec(body);
  if (!decl) throw new Error(`no grid-template-areas in ${selector}`);
  return [...decl[1].matchAll(/"([^"]+)"/g)].map((m) =>
    m[1].trim().replace(/\s+/g, " "),
  );
}

describe("the shell's zone adjacency (SNV-FR-55)", () => {
  const rows = gridAreas("\n.shell {");

  it("lays out three rows: chrome, content, bottom panel", () => {
    expect(rows).toHaveLength(3);
    expect(rows[0]).toBe("topchrome topchrome topchrome");
  });

  it("runs the activity bar down both content rows", () => {
    // SNV-FR-55: the strip spans the shell's full height, so opening the
    // bottom panel never shortens it.
    expect(rows[1].split(" ")[0]).toBe("activity");
    expect(rows[2].split(" ")[0]).toBe("activity");
  });

  it("starts the bottom panel at the strip's inner edge, not beneath it", () => {
    // The regression: `"bottom bottom bottom"` spans the strip too, which cuts
    // the activity bar off at the panel's top edge and lifts its trailing
    // cluster of toggles away from the foot of the window (SNV-FR-44).
    expect(rows[2]).toBe("activity bottom bottom");
    expect(rows[2]).not.toContain("bottom bottom bottom");
  });

  it("shortens the vertical panel and the viewport instead", () => {
    // Both live in the content row only, so the bottom panel takes its height
    // from them (SNV-FR-55).
    expect(rows[1]).toBe("activity panel viewport");
    expect(rows[2]).not.toContain("panel");
    expect(rows[2]).not.toContain("viewport");
  });

  it("keeps the strip out of the top-chrome row", () => {
    // SNV-FR-01: the chrome spans the full width above every other zone.
    expect(rows[0]).not.toContain("activity");
  });
});
