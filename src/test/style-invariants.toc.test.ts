import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * EDT-FR-EKCY: the text size of the entries of the table of contents. The
 * sizes are CSS only, and jsdom applies no stylesheet, so the rules are read
 * from the sheets. Each size token resolves at the default sizes of the UI and
 * Rich Markdown roles.
 */

const tokens = sheet("colors_and_type.css");
const widgets = sheet("kit/dashboard-widgets.css");

/** A custom property's value, wherever in the sheet it is declared. */
function token(name: string): string {
  const m = tokens.match(new RegExp(`(?:^|[;{])\\s*${name}\\s*:\\s*([^;]+)`));
  expect(m, `no ${name}`).not.toBeNull();
  return m![1].trim();
}

/** The pixel size of a font-size value: a length, a token, or a ratio of a role size. */
function px(value: string): number {
  const v = value.trim();
  const length = v.match(/^([\d.]+)px$/);
  if (length) return Number(length[1]);
  const ref = v.match(/^var\(\s*(--[A-Za-z0-9-]+)\s*\)$/);
  if (ref) return px(token(ref[1]));
  const ratio = v.match(/^calc\(\s*var\(\s*(--[A-Za-z0-9-]+)\s*\)\s*\*\s*([\d.]+)\s*\/\s*([\d.]+)\s*\)$/);
  if (ratio) return (px(token(ratio[1])) * Number(ratio[2])) / Number(ratio[3]);
  throw new Error(`cannot resolve ${v}`);
}

/** The size of a level's entry. Its rule has to exist, so the test cannot pass on nothing. */
function entrySize(level: number): number {
  const blocks = blocksFor(widgets, `.toc__panel .fsel__option[data-level="${level}"]`);
  expect(blocks, `no rule for level ${level}`).toHaveLength(1);
  const size = decl(blocks[0], "font-size");
  expect(size, `level ${level} sets no font-size`).not.toBeNull();
  return px(size!);
}

/** The size of the surface heading of a level. The surface sizes h1 to h3. */
function headingSize(tag: string): number {
  const blocks = blocksFor(tokens, `.doc ${tag}`);
  expect(blocks, `no rule for .doc ${tag}`).toHaveLength(1);
  return px(decl(blocks[0], "font-size")!);
}

describe("the text size of the table of contents entries (EDT-FR-EKCY)", () => {
  const sizes = [1, 2, 3, 4, 5, 6].map(entrySize);

  it("EDT-FR-EKCY: the size gets smaller at each level from 1 to 4", () => {
    expect(sizes[0]).toBeGreaterThan(sizes[1]);
    expect(sizes[1]).toBeGreaterThan(sizes[2]);
    expect(sizes[2]).toBeGreaterThan(sizes[3]);
  });

  it("EDT-FR-EKCY: a level 5 or 6 entry is not larger than a level 4 entry", () => {
    expect(sizes[4]).toBeLessThanOrEqual(sizes[3]);
    expect(sizes[5]).toBeLessThanOrEqual(sizes[3]);
  });

  it("EDT-FR-EKCY: at the default role sizes, each entry of levels 1 to 4 is smaller than its heading on the surface", () => {
    expect(sizes[0]).toBeLessThan(headingSize("h1"));
    expect(sizes[1]).toBeLessThan(headingSize("h2"));
    expect(sizes[2]).toBeLessThan(headingSize("h3"));
    // The surface does not size h4, so it has at least the size of the body
    // text it is set in.
    expect(blocksFor(tokens, ".doc h4")).toHaveLength(0);
    expect(sizes[3]).toBeLessThan(px(token("--fs-doc-md")));
  });

  it("EDT-FR-EBNU: a panel with no heading hides the count row", () => {
    const blocks = blocksFor(widgets, ".toc__panel--empty .fsel__count");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "display")).toBe("none");
  });
});
