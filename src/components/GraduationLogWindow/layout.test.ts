import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../../test/readStylesheet";

/**
 * Structural guards on the log window's layout
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-SNDH).
 *
 * These read the stylesheet as text rather than rendering it, on the same terms
 * `../../styles/shellGrid.test.ts` sets out: Vitest runs with `css: false`, so
 * jsdom applies no stylesheet and no rendering test in this repo can observe a
 * computed layout. What GLW-FR-SNDH requires — that the viewport is the one
 * region that scrolls, that it never scrolls sideways, and that a long line
 * wraps — lives entirely in three declarations whose failure is invisible to
 * every other test: the DOM is identical either way, and a run whose agent
 * printed one very long line quietly puts the window on a horizontal
 * scrollbar.
 */

const CSS = readStylesheet("components.css");
/** The Agent Output row's own rules, which the window's rows take. */
const KIT = readStylesheet("kit.css");

/** The declarations of one rule, by selector. */
function rule(selector: string, sheet: string = CSS): string {
  const at = sheet.indexOf(`\n${selector} {`);
  if (at < 0) throw new Error(`no rule for ${selector}`);
  const body = sheet.slice(at + selector.length + 3);
  return body.slice(0, body.indexOf("}"));
}

describe("the log window's layout (GLW-FR-SNDH)", () => {
  it("GLW-FR-SNDH: the viewport is the one region that scrolls, and never sideways", () => {
    const viewport = rule(".glw__viewport");
    expect(viewport).toMatch(/overflow-y:\s*auto/);
    expect(viewport).toMatch(/overflow-x:\s*hidden/);
  });

  it("GLW-FR-SNDH, GLW-FR-KHGP: a long summary wraps rather than widening the region", () => {
    // A row is the Agent Output row, so its summary takes that row's own rule.
    const text = rule(".runs-line__msg", KIT);
    expect(text).toMatch(/white-space:\s*pre-wrap/);
    expect(text).toMatch(/word-break:\s*break-word/);
  });

  it("GLW-FR-FPUX: the stylesheet holds no rule of a stream toggle or of the old row parts", () => {
    expect(CSS).not.toContain(".glw__streams");
    expect(CSS).not.toContain(".glw__meta");
    expect(CSS).not.toContain(".glw__text");
  });

  it("GLW-FR-SNDH: the window is legible at the smallest supported window size", () => {
    const modal = rule(".modal.glw");
    // It never demands more width or height than the window has.
    expect(modal).toMatch(/max-width:\s*calc\(100vw - 32px\)/);
    expect(modal).toMatch(/max-height:\s*min\(/);
  });
});
