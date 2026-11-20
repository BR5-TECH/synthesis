/**
 * The stylesheet half of the editor zoom (`EZM-editor-zoom.md`). jsdom loads no
 * stylesheet (`css: false`), so these read the real CSS as text, as
 * `DiffView.layout.test.tsx` does.
 */
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

const tokens = readStylesheet("colors_and_type.css");
const components = readStylesheet("components.css");
const kit = readStylesheet("kit.css");

/** The body of the first rule whose selector list is exactly `selector`. */
function block(css: string, selector: string): string {
  const at = css.indexOf(`${selector} {`);
  expect(at, `rule ${selector}`).toBeGreaterThanOrEqual(0);
  const open = css.indexOf("{", at);
  return css.slice(open + 1, css.indexOf("}", open));
}

/** Every `--fs-doc-*` and `--fs-src-*` token that `:root` declares. */
function ladderTokens(): string[] {
  const root = block(tokens, ":root");
  return [...root.matchAll(/(--fs-(?:doc|src)-[a-z0-9]+)\s*:/g)].map((m) => m[1]);
}

describe("editor zoom stylesheet", () => {
  const region = block(tokens, ".editor-zoom,\n.editor-zoom .diagram-viewer__area");

  it("EZM-FR-LRVS: a region declares every Rich Markdown and Source code token again, times the zoom", () => {
    const names = ladderTokens();
    expect(names.length).toBe(11);
    for (const name of names) {
      const decl = region.match(new RegExp(`${name}\\s*:([^;]+);`));
      expect(decl, name).not.toBeNull();
      expect(decl![1], name).toMatch(/var\(--editor-zoom\)/);
    }
  });

  it("EZM-FR-LRVS: UI-role tokens and line heights are not declared again", () => {
    expect(region).not.toMatch(/--fs-ui-/);
    expect(region).not.toMatch(/line-height/);
  });

  it("EZM-FR-LRVS, EZM-FR-RWKC: the page width tokens grow with the zoom", () => {
    for (const name of ["--doc-page-width", "--doc-page-box"]) {
      const decl = region.match(new RegExp(`${name}\\s*:([^;]+);`));
      expect(decl![1], name).toMatch(/var\(--editor-zoom\)/);
    }
    expect(region).toMatch(/--doc-page-outer\s*:\s*calc\(var\(--doc-page-box\)/);
  });

  it("EZM-FR-LRVS, EZM-FR-RWKC: the Document and OpenSpec reading pages grow with the zoom inside a region", () => {
    for (const [selector, base] of [
      [".document-viewer__rich", "760px"],
      [".openspec-file__text", "760px"],
      [".openspec-deltas", "1100px"],
    ]) {
      const scoped = block(tokens, `.editor-zoom ${selector}`);
      expect(scoped, selector).toMatch(new RegExp(`max-width:\\s*calc\\(${base} \\* var\\(--editor-zoom\\)\\)`));
      // A max-width and no width: the page is never wider than its region.
      expect(scoped, selector).not.toMatch(/(^|[^-])width\s*:/);
    }
  });

  it("EZM-FR-RWKC: the Editor page, its frontmatter, and the Diff page cap their width at the tab width less the inset", () => {
    const cap = /max-width:\s*min\(\s*var\(--doc-page-outer\),\s*calc\(100% - var\(--doc-page-inset\) \* 2\)/;
    const sheets = kit + components;
    for (const selector of [".editor", ".editor__frontmatter", ".diff-rich--page"]) {
      expect(block(sheets, selector), selector).toMatch(cap);
    }
  });

  it("EZM-FR-SOZM: the factor is 1 on the root", () => {
    expect(tokens).toMatch(/:root\s*\{\s*--editor-zoom:\s*1;\s*\}/);
  });

  it("EZM-FR-BNFJ: a diagram area keeps the factor at 1", () => {
    expect(tokens).toMatch(/\.editor-zoom \.diagram-viewer__area\s*\{\s*--editor-zoom:\s*1;\s*\}/);
  });

  it("EZM-FR-VQHI: a region and the children of a contents region keep a pan but not the native pinch", () => {
    expect(region).toMatch(/touch-action:\s*pan-x pan-y/);
    expect(tokens).toMatch(/\.editor-zoom--contents > \*\s*\{\s*touch-action:\s*pan-x pan-y;\s*\}/);
    expect(tokens).toMatch(/\.editor-zoom--contents\s*\{\s*display:\s*contents;\s*\}/);
  });

  it("DFV-FR-37, EZM-FR-LRVS: the diff gutter and sign columns grow with the zoom", () => {
    expect(block(components, ".diff-line__gutter")).toMatch(/width:\s*calc\(var\(--fs-src-md\) \* 48 \/ 13\)/);
    expect(components).toMatch(/\.diff-line__sign\s*\{\s*width:\s*calc\(var\(--fs-src-md\) \* 12 \/ 13\)/);
  });
});
