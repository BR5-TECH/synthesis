/**
 * The stylesheet half of the page width (`EPW-editor-page-width.md`). jsdom
 * loads no stylesheet, so these read the real CSS as text, as
 * `editorZoom.test.ts` does.
 */
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

const tokens = readStylesheet("colors_and_type.css");
const components = readStylesheet("components.css");

function block(css: string, selector: string): string {
  const at = css.indexOf(`${selector} {`);
  expect(at, `rule ${selector}`).toBeGreaterThanOrEqual(0);
  const open = css.indexOf("{", at);
  return css.slice(open + 1, css.indexOf("}", open));
}

const decl = (body: string, name: string) =>
  body.match(new RegExp(`(?:^|\\n)\\s*${name}\\s*:([^;]+);`))?.[1].trim();

describe("page width stylesheet", () => {
  const root = block(tokens, ":root");
  const region = block(tokens, ".editor-zoom,\n.editor-zoom .diagram-viewer__area");

  it("EPW-FR-QGUZ, EPW-FR-NSNH: the root declares the base width once, and the page tokens are built on it", () => {
    expect(decl(root, "--doc-page-base-width")).toBe("820px");
    expect(decl(root, "--doc-page-width")).toBe("var(--doc-page-base-width)");
    expect(decl(root, "--doc-page-box")).toMatch(/^calc\(var\(--doc-page-base-width\) \+ 96px\)$/);
  });

  it("EPW-FR-GNNZ, EZM-FR-LRVS: inside a zoom region the base width is multiplied by the zoom", () => {
    expect(decl(region, "--doc-page-width")).toBe(
      "calc(var(--doc-page-base-width) * var(--editor-zoom))",
    );
    expect(decl(region, "--doc-page-box")).toBe(
      "calc(var(--doc-page-base-width) * var(--editor-zoom) + 96px)",
    );
    expect(region).not.toMatch(/820px/);
  });

  it("EPW-FR-ADPJ, ACT-FR-09: the ruler is a fixed strip as tall as the action control's gutter", () => {
    const ruler = block(components, ".page-ruler");
    expect(decl(ruler, "--page-ruler-height")).toBe("var(--sp-4)");
    expect(decl(ruler, "height")).toBe("var(--page-ruler-height)");
    expect(decl(ruler, "flex")).toBe("none");
    // The hairline on top is inside the height, so the strip is not taller.
    expect(decl(ruler, "box-sizing")).toBe("border-box");
    // The control stands that same gutter above the tab's foot.
    expect(decl(block(components, ".draft-actions__toggle"), "bottom")).toBe("var(--sp-4)");
  });
});
