import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * EDT-FR-UXZH, EDT-FR-VPDK, EDT-FR-CCKO: the heading scale of the Rich
 * Markdown surfaces. jsdom applies no stylesheet, so the rules are read from
 * the sheets. Each heading size is a ratio of the Rich Markdown role size, and
 * the tests resolve that ratio at each role size the Appearance section
 * permits (GLS-FR-19).
 */

const tokens = sheet("colors_and_type.css");
const kit = sheet("kit.css");
const components = sheet("components.css");

/** A custom property's value, wherever in the sheet it is declared. */
function token(name: string): string {
  const m = tokens.match(new RegExp(`(?:^|[;{])\\s*${name}\\s*:\\s*([^;]+)`));
  expect(m, `no ${name}`).not.toBeNull();
  return m![1].trim();
}

/**
 * The ratio of a font-size value to the Rich Markdown role size. A value that
 * is not a ratio of that role size fails, because it does not move when the
 * role size moves.
 */
function richRatio(value: string): number {
  const v = value.trim();
  if (/^var\(\s*--font-rich-size\s*\)$/.test(v)) return 1;
  const ref = v.match(/^var\(\s*(--fs-doc-[a-z0-9-]+)\s*\)$/);
  if (ref) return richRatio(token(ref[1]));
  const ratio = v.match(/^calc\(\s*var\(\s*--font-rich-size\s*\)\s*\*\s*([\d.]+)\s*\/\s*([\d.]+)\s*\)$/);
  if (ratio) return Number(ratio[1]) / Number(ratio[2]);
  throw new Error(`${v} is not a ratio of the Rich Markdown role size`);
}

/** The one rule of a selector in the tokens sheet. */
function only(selector: string): string {
  const blocks = blocksFor(tokens, selector);
  expect(blocks, `expected one rule for ${selector}`).toHaveLength(1);
  return blocks[0];
}

function headingRatio(tag: string): number {
  const size = decl(only(`.doc ${tag}`), "font-size");
  expect(size, `.doc ${tag} sets no font-size`).not.toBeNull();
  return richRatio(size!);
}

const body = richRatio(decl(only(".doc p"), "font-size")!);
const [h1, h2, h3, h4, h5, h6] = ["h1", "h2", "h3", "h4", "h5", "h6"].map(headingRatio);

/** The class list of a surface's `.doc` root, read from its source file. */
function surfaceClass(file: string, pattern: RegExp): string {
  const m = readFileSync(resolve(process.cwd(), "src", file), "utf8").match(pattern);
  expect(m, `no surface root in ${file}`).not.toBeNull();
  return m![1];
}

/** A selector that names a heading element. */
const HEADING = /(^|[\s>+~,])h[1-6]\b/;

describe("the heading scale of the Rich Markdown surfaces (EDT-FR-UXZH)", () => {
  it("EDT-FR-UXZH: the size is strictly H1 > H2 > H3 > H4 > body", () => {
    // Each size is a fixed ratio of the role size (`richRatio` fails on any
    // other value), so this order holds at each size GLS-FR-19 permits.
    expect(h1).toBeGreaterThan(h2);
    expect(h2).toBeGreaterThan(h3);
    expect(h3).toBeGreaterThan(h4);
    expect(h4).toBeGreaterThan(body);
  });

  it("EDT-FR-UXZH: each size of the scale is a ratio of the Rich Markdown role size", () => {
    for (const tag of ["h1", "h2", "h3", "h4", "h5", "h6"]) {
      expect(() => headingRatio(tag)).not.toThrow();
    }
    expect(() => richRatio(decl(only(".doc p"), "font-size")!)).not.toThrow();
  });

  it("EDT-FR-UXZH: an H5 or H6 is not larger than an H4 and not smaller than body text", () => {
    for (const r of [h5, h6]) {
      expect(r).toBeLessThanOrEqual(h4);
      expect(r).toBeGreaterThanOrEqual(body);
    }
  });

  it("EDT-FR-UXZH, DFV-FR-17, DFV-FR-38, DTV-FR-SSQI: each Rich Markdown surface roots its content in `.doc`", () => {
    const surfaces: [string, RegExp][] = [
      ["components/Editor/useEditorSurface.ts", /class:\s*"([^"]*\bdoc\b[^"]*)"/],
      ["components/DiffView/rich.tsx", /className="([^"]*\bdoc\b[^"]*)"/],
      ["components/DiffRichTarget.tsx", /class:\s*"([^"]*\bdoc\b[^"]*)"/],
      ["components/DocumentViewer/index.tsx", /className="([^"]*\bdocument-viewer__rich\b[^"]*)"/],
    ];
    for (const [file, pattern] of surfaces) {
      expect(surfaceClass(file, pattern).split(/\s+/), file).toContain("doc");
    }
  });

  it("EDT-FR-UXZH, DFV-FR-17, DFV-FR-38, DTV-FR-SSQI: no other rule sets the size of a heading, so each surface has the one scale", () => {
    // A rule of a surface that sizes a heading again would give that surface a
    // scale of its own. The scale itself, the UI-role modifier, and the `.t-h*`
    // helpers are the only rules that may size one.
    const own = /^\.(doc|doc--ui) h[1-6]$|^\.t-h[1-6]$/;
    for (const css of [tokens, kit, components]) {
      for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (!HEADING.test(m[1])) continue;
        const selectors = m[1].split(",").map((x) => x.trim().replace(/\s+/g, " "));
        if (selectors.every((x) => own.test(x))) continue;
        for (const prop of ["font-size", "font"]) {
          expect(decl(m[2], prop), `\`${m[1].trim()}\` sets ${prop}`).toBeNull();
        }
      }
    }
  });

  it("DTV-FR-SSQI: the rich view of a Document tab sets the weight of h4 to h6 but not their size or line height", () => {
    const blocks = blocksFor(components, ".document-viewer__rich h4");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "font-weight")).toBe("var(--fw-semi)");
    expect(decl(blocks[0], "font-size")).toBeNull();
    expect(decl(blocks[0], "line-height")).toBeNull();
  });
});

describe("the size of an H4 (EDT-FR-VPDK)", () => {
  it("EDT-FR-VPDK: an H4 is at least 1.1 times the body text size", () => {
    expect(h4 / body).toBeGreaterThanOrEqual(1.1);
  });

  it("EDT-FR-VPDK: the H3 to H4 ratio is not larger than the H2 to H3 ratio", () => {
    expect(h3 / h4).toBeLessThanOrEqual(h2 / h3);
  });

  it("EDT-FR-VPDK: the rule sets no weight, so H4 keeps its bold weight, and bold body text keeps its weight and size", () => {
    expect(decl(only(".doc h4"), "font-weight")).toBeNull();
    const strong = only(".doc strong");
    expect(decl(strong, "font-weight")).toBe("var(--fw-semi)");
    expect(decl(strong, "font-size")).toBeNull();
  });

  it("EDT-FR-VPDK: no surface rule changes the weight of an H4", () => {
    // The Document tab keeps its own semibold H4 to H6 (checked above). Bold
    // body text has its one rule, `.doc strong` (checked above).
    for (const css of [kit, components]) {
      for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const selectors = m[1].split(",").map((x) => x.trim().replace(/\s+/g, " "));
        if (!selectors.some((x) => /(^|[\s>+~])h4$/.test(x))) continue;
        if (selectors.every((x) => x.startsWith(".document-viewer__rich h"))) continue;
        expect(decl(m[2], "font-weight"), `\`${m[1].trim()}\` sets font-weight`).toBeNull();
      }
    }
  });
});

describe("the line height of the heading scale (EDT-FR-CCKO)", () => {
  const lineHeight = (name: string) => Number(token(name));

  it("EDT-FR-CCKO: H4 to H6 take a fixed heading line height of at least 1, which the role line height does not change", () => {
    for (const selector of [".doc h4", ".doc h5"]) {
      const value = decl(only(selector), "line-height");
      expect(value, `${selector} sets no line-height`).toBe("var(--lh-snug)");
    }
    expect(blocksFor(tokens, ".doc h6")).toEqual(blocksFor(tokens, ".doc h5"));
    // A line height of 1 or more holds the glyphs of its own line, and the
    // value does not change with the role's line height, so no permitted
    // setting makes a heading clip or overlap the next line.
    expect(lineHeight("--lh-snug")).toBeGreaterThanOrEqual(1);
  });

  it("EDT-FR-CCKO: the heading rules clip nothing and generate no content", () => {
    for (const selector of [".doc h4", ".doc h5"]) {
      const block = only(selector);
      for (const prop of ["overflow", "max-height", "height", "content"]) {
        expect(decl(block, prop), `${selector} sets ${prop}`).toBeNull();
      }
    }
  });
});
