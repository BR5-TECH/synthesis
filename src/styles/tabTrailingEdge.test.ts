import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

/**
 * The close control keeps a tab's trailing edge and the dirty indicator keeps
 * its place after the label (`specifications/ui/TAB-tabs.md` TAB-FR-29, and the
 * layout note under TAB-FR-35), whatever the label is.
 *
 * This reads the stylesheet as text rather than rendering it, for the reason
 * `shellGrid.test.ts` gives: Vitest runs with `css: false`, so jsdom applies no
 * stylesheet and no rendering test in this repo can measure a laid-out box.
 *
 * The failure mode is silent to every other test. A tab holds a floor width, so
 * a short label leaves free space in it; when nothing took that space, it fell
 * after the close control and put the control in the middle of the tab, out of
 * line with the close control of every fuller tab beside it. The DOM is the
 * same either way — `TabStrip.test.tsx` puts the close control last and stays
 * green — so these rules are the only place the requirement lives.
 *
 * **This guard pins one implementation, not the behaviour.** A text-level read
 * cannot tell two correct layouts apart, so a different but equally correct fix
 * fails it: flex longhands in place of the shorthand, or `.tab > *:not(...)` in
 * place of the pair of rules. That is the same bargain `shellGrid.test.ts`
 * makes. If you are here because you changed the CSS on purpose, measure the
 * new rules in a browser and then change these assertions to match; do not
 * assume the test is right and the CSS is wrong.
 *
 * Giving the free space to the label instead of to the margin is the one
 * alternative that is **not** equally correct: it moves the close control the
 * same way but carries the dirty indicator along with it, which the layout note
 * does not allow. Measured in Chromium on a floored tab labelled `Te`, the dot
 * stood 16.98px from its text rather than 8px.
 */

/**
 * The sheet with every comment taken out.
 *
 * A comment is not a rule, and this file's own house style names selectors in
 * backticks inside the comment above a rule (`header.css` does it several
 * times). A scan that reads the text ahead of a `{` would take that comment for
 * part of the selector.
 */
const CSS = readStylesheet("components.css").replace(/\/\*[\s\S]*?\*\//g, "");

interface Rule {
  /** One selector. A rule written as a list becomes one `Rule` per selector. */
  selector: string;
  body: string;
  /** True when the rule stands inside an at-rule, `@media` above all. */
  nested: boolean;
}

/**
 * Every rule in the sheet, in cascade order, at-rules included.
 *
 * This walks the braces rather than matching selectors with a regex, because
 * the three things a regex missed here are the three a reader of this file
 * would most want caught: a rule inside a `@media` block, a selector written in
 * a list, and a rule that a comment stands in front of.
 */
function parse(css: string, nested = false): Rule[] {
  const out: Rule[] = [];
  let at = 0;
  let from = 0;
  while (at < css.length) {
    if (css[at] !== "{") {
      at += 1;
      continue;
    }
    const prelude = css.slice(from, at).trim();
    let depth = 1;
    let end = at + 1;
    while (end < css.length && depth > 0) {
      if (css[end] === "{") depth += 1;
      else if (css[end] === "}") depth -= 1;
      end += 1;
    }
    const body = css.slice(at + 1, end - 1);
    if (prelude.startsWith("@")) out.push(...parse(body, true));
    else
      for (const selector of prelude.split(","))
        out.push({ selector: selector.trim().replace(/\s+/g, " "), body, nested });
    at = end;
    from = end;
  }
  return out;
}

const RULES = parse(CSS);

/**
 * The winning value of one property for a selector.
 *
 * The last declaration wins, as it does in CSS — and it is resolved per
 * property rather than per rule, because a later rule with the same selector
 * overrides only what it declares. `.tab__attention` is re-declared inside a
 * `prefers-reduced-motion` block to drop its transition; reading the last whole
 * *rule* would see that block alone and report the mark as unpositioned.
 */
function value(selector: string, property: string): string | undefined {
  const all = RULES.filter((r) => r.selector === selector);
  expect(all.length, `no rule for \`${selector}\``).toBeGreaterThan(0);
  let found: string | undefined;
  for (const r of all) {
    const here = decl(r.body, property);
    if (here !== undefined) found = here;
  }
  return found;
}

/** A shorthand or longhand declaration in one rule body, as written. */
function decl(body: string, property: string): string | undefined {
  const found = new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]+)`).exec(body);
  return found?.[1].trim();
}

/** The properties that can move a tab's parts about. */
const LAYOUT =
  /(?:^|;)\s*(flex|flex-flow|flex-grow|flex-shrink|flex-basis|order|align-self|justify-self|display|position|float|gap|inset[a-z-]*|(?:margin|padding)(?:-(?:inline|block))?(?:-(?:start|end))?|margin-left|margin-right|padding-left|padding-right|width|min-width|max-width)\s*:/;

/**
 * Every rule that reaches `.<part>` other than the part's own, and lays it out.
 *
 * A rule that only recolours is fine and several exist (`.tab__close--disabled`
 * carries TAB-FR-16); what may not happen is a second rule placing the part
 * again, from a modifier, a descendant selector, or inside a `@media` block.
 */
function layoutOffenders(part: string, allowed: string[] = []): string[] {
  const reaches = new RegExp(`\\.${part}(?![\\w-])`);
  return RULES.filter(
    (r) =>
      reaches.test(r.selector) &&
      r.selector !== `.${part}` &&
      !allowed.includes(r.selector) &&
      LAYOUT.test(r.body),
  ).map((r) => (r.nested ? `${r.selector} (in an at-rule)` : r.selector));
}

/** A length in px, from a declaration that is a bare px value. */
function px(value: string | undefined, what: string): number {
  expect(value, `${what} must be a px length`).toMatch(/^-?\d+(\.\d+)?px$/);
  return parseFloat(value!);
}

describe("the close control keeps the tab's trailing edge (TAB-FR-29)", () => {
  it("gives the tab's free space to the margin ahead of the close control", () => {
    // This is what puts the control on the trailing edge. An auto margin takes
    // only free space, so it resolves to zero on a tab that is already full.
    expect(value(".tab__close", "margin-inline-start")).toBe("auto");
  });

  it("leaves the dirty indicator against the label it marks", () => {
    // The layout note under TAB-FR-35 asks for both at once: the indicator
    // keeps its place after the label AND the close control keeps the trailing
    // edge. A growing label would satisfy the second clause and break the
    // first, so the label must not take the free space.
    const flex = value(".tab__label", "flex");
    expect(flex, ".tab__label must declare `flex`").toBeDefined();
    expect(flex!.split(/\s+/)[0], "the label must not grow").toBe("0");
    expect(value(".tab__dirty", "margin-inline-start")).toBeUndefined();
  });

  it("lets the label shrink below its text, and truncate on one line", () => {
    // Without `min-width: 0` the label's automatic minimum size is its whole
    // text, and a long label pushes the close control past the tab's trailing
    // edge instead of truncating. Without `nowrap` the ellipsis is inert and
    // the label wraps to a second line, which TAB-FR-27 does not allow either.
        expect(value(".tab__label", "flex")!.split(/\s+/)[1], "the label must give").toBe(
      "1",
    );
    expect(value(".tab__label", "min-width")).toBe("0");
    expect(value(".tab__label", "overflow")).toBe("hidden");
    expect(value(".tab__label", "text-overflow")).toBe("ellipsis");
    expect(value(".tab__label", "white-space")).toBe("nowrap");
  });

  it("keeps every other part of a tab whole", () => {
    // TAB-FR-29: the close control, the dirty indicator, and the type glyph
    // each keep their reserved place; the label is the one part that gives.
    expect(value(".tab > *", "flex")).toBe("0 0 auto");
  });

  it("declares the label's own flex after the rule it narrows", () => {
    // Both selectors carry the same specificity, so the later one wins. The
    // wrong order takes the label's shrink away and pushes the close control
    // out of the tab on a long label.
    expect(CSS.search(/^\.tab > \*\s*\{/m)).toBeLessThan(
      CSS.search(/^\.tab__label\s*\{/m),
    );
  });

  it("leaves no other rule to move any of this about", () => {
    // A modifier, a descendant selector, a selector list, or a `@media` block
    // could each place these again and win. `value()` resolves one selector and
    // would not see any of them.
    for (const part of ["tab__label", "tab__close", "tab__dirty"]) {
      expect(layoutOffenders(part), `a second rule lays .${part} out`).toEqual(
        [],
      );
    }
  });

  it("leaves the tab itself laid out in one place", () => {
    // The same hole one level up: every assertion here is about flex items, so
    // a `.tabstrip .tab { display: block }` anywhere — a `@media` block
    // included — would outrank `.tab` and collapse the layout silently.
    expect(
      layoutOffenders("tab", [
        // TAB-FR-27's floor, deliberately scoped to the scrolling region.
        ".tabstrip__scroll > .tab",
        // The rule this file pins for the parts themselves.
        ".tab > *",
      ]),
      "a second rule lays .tab out",
    ).toEqual([]);
  });

  it("leaves no narrower child rule to beat `.tab > *`", () => {
    // `.tab > span` carries more specificity than `.tab > *` and would take a
    // part's reserved width back.
    expect(
      RULES.filter(
        (r) =>
          /^\.tab > /.test(r.selector) &&
          r.selector !== ".tab > *" &&
          LAYOUT.test(r.body),
      ).map((r) => r.selector),
    ).toEqual([]);
  });
});

describe("the tab is the flex container all of that depends on (TAB-FR-29)", () => {
  

  it("lays its parts out in a row", () => {
    // Every assertion above is about flex items. Make the tab a block and they
    // all still pass while the layout collapses.
    expect(value(".tab", "display")).toContain("flex");
  });

  it("packs them against the leading edge", () => {
    // `center` or `space-between` would move the parts about on its own and
    // undo the trailing edge as thoroughly as the defect did.
    const justify = value(".tab", "justify-content");
    expect(justify === undefined || justify === "flex-start").toBe(true);
  });

  it("holds a floor width for a tab in the scrolling region (TAB-FR-27)", () => {
    // The floor is why the free space exists at all. It stays: the fix
    // distributes that space, it does not remove it. The number is not the
    // contract — a legitimate retune is fine — so this asserts the floor is
    // wide enough to hold what TAB-FR-27 lists rather than one exact value.
    const floor = value(".tabstrip__scroll > .tab", "min-width");
    expect(floor, "the floor must stay on the scrolling region").toBeDefined();
    expect(parseInt(floor!, 10)).toBeGreaterThanOrEqual(88);
  });

  it("keeps the floor off the Home affordance", () => {
    // SNV-FR-09: Home is a control of the strip rather than a tab. It carries
    // `.tab` but sits outside the scrolling region, so a floor moved onto
    // `.tab` itself would stretch an icon-only control into a wide empty box.
    expect(value(".tab", "min-width")).toBeUndefined();
  });

  it("caps a tab's width so a long label must truncate (TAB-FR-29)", () => {
    // The other half of "laid out against that tab's own width". Without the
    // cap a long label just grows the tab and never reaches the ellipsis. As
    // with the floor, the number is not the contract — but a cap wide enough
    // that no real label meets it is not a cap, so this holds a ceiling too.
    const cap = px(value(".tab", "max-width"), "the tab's max-width");
    const floor = px(
      value(".tabstrip__scroll > .tab", "min-width"),
      "the tab's min-width",
    );
    expect(cap).toBeGreaterThan(floor);
    expect(cap).toBeLessThanOrEqual(400);
  });
});

describe("the leading gutter (TAB-FR-35, NTF-FR-34)", () => {
  /** The two marks, with the gutter each needs: its inset plus its width. */
  const MARKS = [".tab__pin", ".tab__attention"] as const;

  it("is cleared by the tab's leading padding, so no mark covers the label", () => {
    // TAB-FR-35: the marker "obscures neither the tab's label nor its close
    // control". The marks are out of flow, so the only thing holding the glyph
    // and the label clear of them is the tab's own leading padding — a bare
    // number that nothing otherwise ties to the marks' geometry. Narrow it and
    // the pin sits under the type glyph with every other test still green.
    const padding = value(".tab", "padding");
    expect(padding, "the tab's padding must be the four-value shorthand")
      .toBeDefined();
    const sides = padding!.split(/\s+/);
    expect(sides).toHaveLength(4);
    const lead = px(sides[3], "the tab's leading padding");

    for (const mark of MARKS) {
      const needs =
        px(value(mark, "inset-inline-start"), `${mark} inset`) +
        px(value(mark, "width"), `${mark} width`);
      expect(lead, `${mark} reaches under the tab's contents`)
        .toBeGreaterThanOrEqual(needs);
    }
  });

  // `.tab > *` reaches these too. They are unaffected only because they are
  // absolutely positioned — take that away and the rule reserves the marks'
  // width *in flow*, which costs the label width and moves every tab's
  // contents when a mark appears. That is the one regression the new selector
  // can cause, so it is pinned here.
  it.each([".tab__attention", ".tab__pin", ".sr-only"])(
    "%s is absolutely positioned",
    (selector) => {
      expect(value(selector, "position")).toBe("absolute");
    },
  );
});
