import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

/**
 * ESH-FR-YNIV / DFV-FR-57: a source file's token colours are legible against the
 * page, in the light theme and in the dark one.
 *
 * Nothing else in the suite can see this. Vitest runs with `css: false`
 * (`vitest.config.ts`), so jsdom applies no stylesheet and no rendering test can
 * compute a colour — a token painted in a tone two steps from the page's
 * background leaves every test green and the file unreadable. This is the same
 * bargain `attentionContrast.test.ts` strikes: a text-level guard where the
 * alternative is no coverage at all of a requirement whose breakage nothing else
 * can see.
 *
 * Two claims are checked. Each role clears the WCAG **4.5:1** floor against the
 * page it is read on, so no token is faint; and each is far enough from plain
 * text's own tone that a token reads as one rather than as a shade of the prose
 * around it.
 */

const CSS = readFileSync(
  resolve(process.cwd(), "src/styles/colors_and_type.css"),
  "utf8",
);

// The light palette is declared first and the dark one redefines the same names,
// so the file splits in two at that selector — anchored on the rule at the start
// of a line, since the header comment names it too.
const DARK_AT = CSS.search(/^\[data-theme="dark"\]\s*\{/m);
const REGION = {
  light: CSS.slice(0, DARK_AT),
  dark: CSS.slice(DARK_AT),
} as const;

/** The last declaration wins, as it does in CSS. */
function token(region: string, name: string): string {
  const all = [
    ...region.matchAll(new RegExp(`${name}\\s*:\\s*(#[0-9a-fA-F]{6})`, "g")),
  ];
  expect(all.length, `${name} is missing or not a 6-digit hex`).toBeGreaterThan(
    0,
  );
  return all[all.length - 1][1];
}

/** WCAG relative luminance. */
function luminance(hex: string): number {
  const n = hex.slice(1);
  const [r, g, b] = [0, 2, 4]
    .map((i) => parseInt(n.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio, 1 (identical) to 21 (black on white). */
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** The nine roles of ESH-FR-YNIV, as the stylesheet names them. */
const ROLES = [
  "--code-comment",
  "--code-keyword",
  "--code-string",
  "--code-number",
  "--code-type",
  "--code-function",
  "--code-variable",
  "--code-meta",
  "--code-punct",
] as const;

/** WCAG AA for body text. A token below this is decoration rather than text. */
const MIN_CONTRAST = 4.5;

/**
 * How far a token's luminance must sit from plain text's. Small — some roles
 * (a variable, an identifier) are *meant* to read close to the prose — but not
 * zero, or the palette would say nothing at all where it claims to.
 */
const MIN_DISTINCTION = 0.02;

describe("syntax token colours are readable in both themes (ESH-FR-YNIV)", () => {
  for (const theme of ["light", "dark"] as const) {
    it(`clears 4.5:1 against the page in ${theme}`, () => {
      const region = REGION[theme];
      // `--doc-page-bg` is declared as `var(--bg-canvas)`, so the page's actual
      // tone is that token's. Pinned, so a page given a background of its own
      // does not leave this measuring the wrong surface.
      expect(region).toMatch(/--doc-page-bg:\s*var\(--bg-canvas\)/);
      const page = token(region, "--bg-canvas");
      for (const role of ROLES) {
        const colour = token(region, role);
        expect(
          contrast(colour, page),
          `${role} (${colour}) against the page (${page}) in ${theme}`,
        ).toBeGreaterThanOrEqual(MIN_CONTRAST);
      }
    });

    it(`keeps every role distinguishable from plain text in ${theme}`, () => {
      const region = REGION[theme];
      const plain = luminance(token(region, "--fg-1"));
      for (const role of ROLES) {
        const colour = token(region, role);
        expect(
          Math.abs(luminance(colour) - plain),
          `${role} (${colour}) against plain text in ${theme}`,
        ).toBeGreaterThanOrEqual(MIN_DISTINCTION);
      }
    });
  }

  /**
   * ESH-FR-VNPW / ESH-FR-LXQR: the layer takes no part in editing and is out of the
   * accessibility tree.
   *
   * Its inertness is a *stylesheet* fact — `pointer-events: none` — which
   * `css: false` makes invisible to every rendering test in the suite. Drop that
   * declaration and the layer starts swallowing the clicks meant for the
   * textarea beneath it: the caret stops landing where the author points, and
   * nothing else here would say so.
   */
  it("keeps the token layer inert and out of the way", () => {
    const kit = readStylesheet("kit.css");
    const at = kit.indexOf(".editor__source-hl {");
    expect(at, ".editor__source-hl is missing from kit.css").toBeGreaterThan(-1);
    const rule = kit.slice(at, kit.indexOf("}", at));
    expect(rule).toContain("pointer-events: none");
    // It sits behind the editable text rather than over it, and does not scroll
    // on its own — the textarea drives it.
    expect(rule).toContain("z-index: 0");
    expect(rule).toContain("overflow: hidden");

    // The two boxes have to break lines in the same places, or the layer and the
    // text it mirrors drift apart into doubled glyphs a line off.
    const sourceAt = kit.indexOf(".editor__source {");
    const sourceRule = kit.slice(sourceAt, kit.indexOf("}", sourceAt));
    for (const declaration of [
      "white-space: pre-wrap",
      "word-break: break-word",
      "overflow-wrap: anywhere",
      "font-family: var(--font-source-family)",
      "font-size: var(--fs-src-md)",
      "line-height: var(--font-source-line-height)",
      "tab-size: 2",
      // A `pre` inherits `optimizeLegibility` where a `textarea` does not, which
      // shapes text differently and drifts the layer off the glyphs it decorates.
      "text-rendering: auto",
    ]) {
      expect(sourceRule, `the textarea is missing ${declaration}`).toContain(
        declaration,
      );
      expect(rule, `the layer is missing ${declaration}`).toContain(declaration);
    }
  });

  it("declares every role in both themes rather than inheriting one", () => {
    // A role defined in the light palette alone would fall through to the light
    // colour on a dark page, which is exactly the failure ESH-FR-YNIV's "a token
    // colour is never carried across from the other theme" rules out.
    for (const role of ROLES) {
      expect(REGION.light).toContain(`${role}:`);
      expect(REGION.dark).toContain(`${role}:`);
    }
  });
});
