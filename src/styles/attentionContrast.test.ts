import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

/**
 * NTF-FR-34: a tab needing attention must not rely on colour alone.
 *
 * The mark's shape and its position carry most of that, and both are asserted
 * in `components/TabStrip.test.tsx`. What cannot be asserted there is the
 * *tone* the requirement also names — Vitest runs with `css: false`
 * (`vitest.config.ts`), so jsdom applies no stylesheet and no rendering test in
 * this repo can compute a colour.
 *
 * The failure mode this guards is silent and was real: `--accent-soft`, the
 * obvious token for an accent-tinted surface, sits within **0.2 of 255** of
 * `--bg-chrome` in greyscale in the light theme. It looks unmistakably tinted
 * on a colour display and vanishes completely on a greyscale one — and every
 * other test in the suite stays green either way, because the class is applied
 * and the mark is present regardless. A palette change could reintroduce it
 * just as quietly, which is why the separation is pinned as a number here.
 *
 * Written in the same bargain as `shellGrid.test.ts`: a text-level guard is
 * worth having where the alternative is no coverage at all of a requirement
 * whose breakage nothing else can see.
 */

const CSS = readFileSync(
  resolve(process.cwd(), "src/styles/colors_and_type.css"),
  "utf8",
);

/**
 * The light palette is declared before the dark one and the dark one
 * (`[data-theme="dark"]`) redefines the same names, so the file splits cleanly
 * in two at that selector. Slicing on the boundary rather than trying to match
 * a brace-balanced block keeps this readable and immune to the several `:root`
 * rules the file opens with.
 */
// Anchored on the rule at the start of a line, not on the bare selector text:
// the file's own header comment names it too, and splitting there would leave
// the light region empty of every token this reads.
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

/**
 * Rec. 709 luma, which is what a greyscale rendering — and a reader who cannot
 * discriminate the hues — is left with. 0 is black, 255 white.
 */
function luma(hex: string): number {
  const n = hex.slice(1);
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(n.slice(i, i + 2), 16));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * The floor. Well under the 0.2 that prompted this guard and well under what
 * the tokens currently deliver (13.3 light, 21.5 dark, both measured in a real
 * browser), so an ordinary palette adjustment does not trip it and a collapse
 * back to "tinted but identical in greyscale" does.
 */
const MIN_SEPARATION = 8;

describe("a marked tab is distinguishable without colour (NTF-FR-34)", () => {
  for (const theme of ["light", "dark"] as const) {
    it(`separates the marked tab from its neighbours in ${theme}`, () => {
      const block = REGION[theme];
      const marked = luma(token(block, "--attention-surface"));

      // An unmarked tab: the strip's own chrome shows through it.
      const unmarked = luma(token(block, "--bg-chrome"));
      expect(Math.abs(marked - unmarked)).toBeGreaterThanOrEqual(
        MIN_SEPARATION,
      );

      // The active tab, which is never marked (NTF-FR-26) and must therefore
      // not be confusable with one that is.
      const active = luma(token(block, "--bg-canvas"));
      expect(Math.abs(marked - active)).toBeGreaterThanOrEqual(MIN_SEPARATION);

      // And the mark itself against the surface it sits on.
      const mark = luma(token(block, "--accent"));
      expect(Math.abs(mark - marked)).toBeGreaterThanOrEqual(MIN_SEPARATION);
    });
  }

  it("does not tint the marked tab with a token that vanishes in greyscale", () => {
    // The specific regression: `--accent-soft` reads as an accent tint and is
    // the obvious thing to reach for, but it is not separable from the chrome
    // it sits on. If a future edit points the marked tab at it, this says so.
    const light = REGION.light;
    expect(
      Math.abs(luma(token(light, "--accent-soft")) - luma(token(light, "--bg-chrome"))),
    ).toBeLessThan(MIN_SEPARATION);

    const components = readStylesheet("components.css");
    const rule = components.slice(
      components.indexOf(".tab[data-attention] {"),
      components.indexOf("}", components.indexOf(".tab[data-attention] {")),
    );
    expect(rule).toContain("var(--attention-surface)");
    expect(rule).not.toContain("var(--accent-soft)");
  });
});
