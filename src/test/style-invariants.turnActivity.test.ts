import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the timeout bar and the tool list
 * (`../../specifications/ui/CVP-conversation-presentation.md` CVP-FR-IJEY,
 * CVP-FR-UYHO; `../../specifications/ui/CTA-comment-agent-turns.md`
 * CTA-FR-FPNP). jsdom computes no layout, so these are the only checks.
 */

const style = () => sheet("kit/agent-turn-activity.css");

describe("the timeout bar", () => {
  it("CVP-FR-UYHO: stands on the field's edge, takes no space, and takes no pointer input", () => {
    const bar = blocksFor(style(), ".timeout-bar")[0];
    expect(decl(bar, "position")).toBe("absolute");
    expect(decl(bar, "pointer-events")).toBe("none");
    expect(decl(blocksFor(style(), ".comment-composer")[0], "position")).toBe("relative");
  });

  it("CVP-FR-UYHO: under a reduced-motion preference the width changes with no transition", () => {
    const css = style();
    const at = css.lastIndexOf("@media (prefers-reduced-motion: reduce)");
    expect(at).toBeGreaterThanOrEqual(0);
    expect(css.slice(at)).toMatch(/\.timeout-bar__fill\s*\{\s*transition:\s*none;/);
  });

  it("CVP-FR-IJEY: the fill is a width from the leading edge, so it empties from right to left", () => {
    const fill = blocksFor(style(), ".timeout-bar__fill")[0];
    expect(decl(fill, "height")).toBe("100%");
    expect(decl(blocksFor(style(), ".timeout-bar")[0], "inset-inline")).toBe("0");
  });
});

describe("the tool list", () => {
  it("CTA-FR-FPNP: a key input wraps rather than overflowing the row", () => {
    const input = blocksFor(style(), ".tool-calls__input")[0];
    expect(decl(input, "overflow-wrap")).toBe("anywhere");
  });
});
