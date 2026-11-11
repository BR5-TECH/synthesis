import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the run strip and the review bar of a change
 * workspace (`../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-NWRS, OCW-FR-YRZK, OCW-FR-ZPDC, OCW-FR-XJMR).
 *
 * One part of the group `./cssRules.ts` heads. jsdom applies no stylesheet, so
 * a component test passes the same with each of these rules removed. A browser
 * check found the head of a plan rest drawn under the discussion column: three
 * actions and the long state words did not fit on one line that could not
 * wrap.
 */

describe("the run strip head", () => {
  it("OCW-FR-NWRS, OCW-FR-ZPDC: the head wraps, so the actions go to a line of their own where they do not fit", () => {
    const css = sheet("components.css");
    const heads = blocksFor(css, ".openspec-runstrip__head");
    expect(heads.length, "no rule for .openspec-runstrip__head").toBeGreaterThan(0);
    // No rule of the head, in any width, sets it back to one line that cannot wrap.
    for (const head of heads) expect(decl(head, "flex-wrap")).not.toBe("nowrap");
    expect(decl(heads[0], "flex-wrap")).toBe("wrap");
  });

  it("OCW-FR-NWRS, OCW-FR-YRZK: the actions stay together at the end of the line, inside the strip", () => {
    const css = sheet("components.css");
    const [actions] = blocksFor(css, ".openspec-runstrip__actions");
    expect(actions, "no rule for .openspec-runstrip__actions").toBeDefined();
    expect(decl(actions, "display")).toBe("flex");
    expect(decl(actions, "margin-inline-start")).toBe("auto");
    // A group wider than the strip wraps inside itself rather than overflowing.
    expect(decl(actions, "flex-wrap")).toBe("wrap");
    expect(decl(actions, "max-width")).toBe("100%");
  });

  it("OCW-FR-NWRS: the title gives way before the actions leave the line, and keeps a few letters", () => {
    const css = sheet("components.css");
    const [title] = blocksFor(css, ".openspec-runstrip__title");
    expect(title, "no rule for .openspec-runstrip__title").toBeDefined();
    // The line break measures the title at its basis, so a basis of a few
    // letters lets the actions stay on the line while the title is cut.
    expect(decl(title, "flex")).toBe("1 1 3em");
    expect(decl(title, "min-width")).toBe("3em");
    expect(decl(title, "max-width")).toBe("max-content");
    expect(decl(title, "text-overflow")).toBe("ellipsis");
  });

  it("OCW-FR-ZPDC: a Continue in flight is drawn as unavailable, and keeps its focus stop", () => {
    const css = sheet("components.css");
    const [held] = blocksFor(css, '.openspec-runstrip__head .btn[aria-disabled="true"]');
    expect(held, "no rule for an unavailable strip action").toBeDefined();
    expect(decl(held, "opacity")).toBe("0.5");
  });
});

describe("the review bar of a change", () => {
  it("OCW-FR-XJMR, OCW-FR-EJGH: Reject all stays at the end of its line, also when it wraps", () => {
    const css = sheet("components.css");
    const [button] = blocksFor(css, ".openspec-review__reject-all");
    expect(button, "no rule for .openspec-review__reject-all").toBeDefined();
    expect(decl(button, "margin-inline-start")).toBe("auto");
  });
});
