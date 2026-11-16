import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../../test/cssRules";

/**
 * The Discussion column's requirements that live in the stylesheet alone.
 * Vitest applies no CSS, so a component test cannot see any of these.
 */
const css = sheet("components.css");

function only(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks, `expected one rule for ${selector}`).toHaveLength(1);
  return blocks[0];
}

describe("the Discussion column (SMP-FR-SWCL)", () => {
  it("SMP-FR-SWCL: the column keeps the width of the session, and the canvas takes the remaining width", () => {
    const column = only(".map-discussion");
    expect(decl(column, "flex")).toBe("none");
    expect(decl(only(".smap-main"), "flex")).toBe("1");
    expect(decl(only(".smap-main"), "min-width")).toBe("0");
  });

  it("SMP-FR-SWCL: the overlay inspector stands left of the column, so the column covers no part of the canvas", () => {
    expect(decl(only(".smap-body"), "--smap-column")).toBe("0px");
    expect(decl(only(".smap-inspector[data-overlay]"), "right")).toBe("var(--smap-column, 0px)");
  });
});
