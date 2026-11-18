import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../../test/cssRules";

/**
 * The requirements of the issue view that live in the stylesheet alone
 * (`MIV-map-issue-view.md`). Vitest applies no CSS, so a component test
 * cannot see any of these.
 */
const css = sheet("kit.css");

function first(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks.length, `expected a rule for ${selector}`).toBeGreaterThan(0);
  return blocks[0];
}

describe("the issue view stylesheet (MIV)", () => {
  it("MIV-FR-HGGS: the view is centred over the canvas at 90% of its width and height, on the elevated ground, with no scrim", () => {
    const rule = first(".miv-sheet");
    expect(decl(rule, "position")).toBe("absolute");
    expect(decl(rule, "width")).toBe("90%");
    expect(decl(rule, "height")).toBe("90%");
    expect(decl(rule, "top")).toBe("50%");
    expect(decl(rule, "left")).toBe("50%");
    expect(decl(rule, "transform")).toBe("translate(-50%, -50%)");
    expect(decl(rule, "background")).toBe("var(--bg-elevated)");
    expect(decl(rule, "border")).toBe("1px solid var(--border-2)");
    expect(sheet("kit/issue-view.css")).not.toMatch(/::backdrop|scrim/);
  });

  it("MIV-FR-HGAQ: the dialogs of the canvas open above the view", () => {
    const view = Number(decl(first(".miv-sheet"), "z-index"));
    const dialog = Number(decl(first(".flow-dialog"), "z-index"));
    expect(dialog).toBeGreaterThan(view);
  });

  it("MIV-FR-THGX: a main column and a 280px sidebar, and one column where the view is narrower than 720px", () => {
    expect(decl(first(".miv-sheet"), "container-type")).toBe("inline-size");
    expect(decl(first(".miv-body-grid"), "grid-template-columns")).toBe("minmax(0, 1fr) 280px");
    expect(css).toMatch(/@container \(max-width: 720px\)\s*\{\s*\.miv-body-grid \{ grid-template-columns: minmax\(0, 1fr\);/);
  });

  it("MIV-FR-DDAF: a fenced code block keeps its whitespace, scrolls in place of a wrap, and labels its language at the trailing edge", () => {
    const pre = first(".doc pre.code-fence");
    expect(decl(pre, "white-space")).toBe("pre");
    expect(decl(pre, "overflow-x")).toBe("auto");
    expect(decl(blocksFor(sheet("colors_and_type.css"), ".doc pre")[0], "background")).toBe("var(--bg-sunken)");
    const label = first(".code-fence__lang");
    expect(decl(label, "position")).toBe("absolute");
    expect(decl(label, "right")).toBe("var(--sp-2)");
  });

  it("MIV layout notes: the header is 40px on the panel ground; the title is 20px of the UI role with weight 600; the sidebar has a border at its leading edge", () => {
    expect(decl(first(".miv-head"), "min-height")).toBe("40px");
    expect(decl(first(".miv-head"), "background")).toBe("var(--bg-panel)");
    expect(decl(first(".miv-main__inner"), "max-width")).toBe("760px");
    expect(decl(first(".miv-title"), "font-size")).toBe("calc(var(--font-ui-size) * 20 / 13)");
    expect(decl(first(".miv-title"), "font-weight")).toBe("600");
    const side = blocksFor(css, ".miv-side").find((b) => decl(b, "overflow") === "auto") ?? "";
    expect(decl(side, "border-left")).toBe("1px solid var(--border-1)");
    expect(sheet("kit/issue-view.css")).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(/);
  });
});
