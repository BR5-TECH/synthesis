import { describe, expect, it } from "vitest";
import { blocksFor, sheet } from "./cssRules";

/**
 * Stylesheet invariants for the surfaces that float over a tab's content — the
 * action control, its menu entries, its composer, its discussion panel, and
 * every other surface drawn the same way.
 *
 * One part of the group `./cssRules.ts` heads, which carries the whole rule
 * these are written under.
 */

type Theme = "light" | "dark";
type Tokens = Map<string, string>;

/** Every custom property a declaration block defines. */
function tokensIn(block: string): [string, string][] {
  return [...block.matchAll(/(--[A-Za-z0-9-]+)\s*:\s*([^;]+)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ]);
}

/**
 * The tokens each theme resolves. The light theme is every `:root` block, in
 * cascade order. The dark theme is the light theme with the dark block on top,
 * because `:root` applies in both and the dark block only overrides it.
 */
function themeTokens(): Record<Theme, Tokens> {
  const css = sheet("colors_and_type.css");
  const light: Tokens = new Map(blocksFor(css, ":root").flatMap(tokensIn));
  const dark: Tokens = new Map([
    ...light,
    ...blocksFor(css, '[data-theme="dark"]').flatMap(tokensIn),
  ]);
  return { light, dark };
}

/** `value` split on the commas and spaces that are not inside parentheses. */
function splitTop(value: string, separator: "," | " "): string[] {
  const out: string[] = [];
  let depth = 0;
  let part = "";
  for (const ch of value) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    const splits = separator === "," ? ch === "," : /\s/.test(ch);
    if (splits && depth === 0) {
      if (part.trim() !== "") out.push(part.trim());
      part = "";
    } else {
      part += ch;
    }
  }
  if (part.trim() !== "") out.push(part.trim());
  return out;
}

/** True when a colour literal has no transparency. */
function isOpaqueColour(colour: string): boolean {
  const hex = colour.match(/^#([0-9a-f]+)$/i);
  if (hex) {
    const digits = hex[1].toLowerCase();
    if (digits.length === 3 || digits.length === 6) return true;
    if (digits.length === 4) return digits.endsWith("f");
    if (digits.length === 8) return digits.endsWith("ff");
    return false;
  }
  const fn = colour.match(/^(rgba?|hsla?)\((.*)\)$/i);
  if (fn) {
    const args = fn[2].includes("/")
      ? [fn[2].split("/")[0], fn[2].split("/")[1]]
      : fn[2].split(",");
    const alpha = args.length === 4 ? args[3] : args.length === 2 ? args[1] : null;
    if (alpha === null) return true;
    const a = alpha.trim();
    return a.endsWith("%") ? Number.parseFloat(a) >= 100 : Number.parseFloat(a) >= 1;
  }
  return false;
}

/**
 * True when a `background` (or `background-color`) value paints an opaque
 * colour under all its other layers in the given theme.
 *
 * Strict by design: a value it cannot resolve — an undefined token, a keyword
 * such as `inherit` or `currentColor`, a `color-mix()` — is not opaque. A guard
 * that is not sure must fail, because a guard that passes when it is not sure
 * is worse than no guard.
 */
function isOpaqueFill(value: string, tokens: Tokens, depth = 0): boolean {
  if (depth > 10) return false;
  const layers = splitTop(value.replace(/!important/i, "").trim(), ",");
  if (layers.length === 0) return false;
  // Only the bottom layer can carry a colour, and it is painted under the rest.
  const bottom = splitTop(layers[layers.length - 1], " ");
  for (let i = bottom.length - 1; i >= 0; i--) {
    const part = bottom[i];
    const ref = part.match(/^var\(\s*(--[A-Za-z0-9-]+)\s*(?:,\s*(.*))?\)$/);
    if (ref) {
      const resolved = tokens.get(ref[1]) ?? ref[2];
      return resolved !== undefined && isOpaqueFill(resolved, tokens, depth + 1);
    }
    if (/^(#|rgba?\(|hsla?\()/i.test(part)) return isOpaqueColour(part);
    // An image, a position, a size, or a repeat keyword is not the colour.
    if (/^(linear|radial|conic|repeating-[a-z]+)-gradient\(|^url\(/i.test(part)) continue;
    if (/^(none|no-repeat|repeat(-x|-y)?|center|top|bottom|left|right|cover|contain)$/i.test(part)) continue;
    // `transparent`, `inherit`, `currentColor`, `color-mix()`, and any other
    // keyword: not something this guard can prove opaque.
    return false;
  }
  // A bottom layer with no colour paints nothing under its image.
  return false;
}

/** The fill a block sets, `background` or `background-color`, whichever is last. */
function fillOf(block: string): string | null {
  const found = [
    ...block.matchAll(/(?:^|;)\s*background(?:-color)?\s*:\s*([^;]+)/gi),
  ];
  return found.length === 0 ? null : found[found.length - 1][1].trim();
}

function shadowOf(block: string): string | null {
  const found = [...block.matchAll(/(?:^|;)\s*box-shadow\s*:\s*([^;]+)/gi)];
  return found.length === 0 ? null : found[found.length - 1][1].trim();
}

interface Rule {
  selector: string;
  block: string;
}

/** The two sheets the surfaces are drawn by, as the browser reads them. */
function componentCss(): string {
  return [sheet("components.css"), sheet("kit.css")].join("\n");
}

/** Every rule of `css`, one entry per selector in a list. */
function rules(css: string): Rule[] {
  const out: Rule[] = [];
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    for (const s of m[1].split(",")) {
      out.push({ selector: s.trim().replace(/\s+/g, " "), block: m[2] });
    }
  }
  return out;
}

/**
 * A surface that floats over content: a single-class rule that paints an
 * opaque fill and casts a shadow. The shadow is what says the surface lies on
 * top of something rather than in the flow beside it.
 */
function floatingSurfaces(all: Rule[], tokens: Tokens): Set<string> {
  const out = new Set<string>();
  for (const { selector, block } of all) {
    if (!/^\.[A-Za-z0-9_-]+$/.test(selector)) continue;
    const fill = fillOf(block);
    const shadow = shadowOf(block);
    if (fill === null || shadow === null || shadow === "none") continue;
    if (isOpaqueFill(fill, tokens)) out.add(selector);
  }
  return out;
}

/**
 * The class a selector names when the selector is that one element in a state:
 * the class followed only by pseudo-classes and attribute selectors. A
 * combinator selects another element and a pseudo-element paints another box,
 * so neither is a state of the surface.
 */
function stateOf(selector: string): string | null {
  const m = selector.match(
    /^(\.[A-Za-z0-9_-]+)((?::[A-Za-z-]+(?:\([^()]*(?:\([^()]*\))?[^()]*\))?|\[[^\]]*\])+)$/,
  );
  return m ? m[1] : null;
}

describe("the opacity guard's own reader", () => {
  // The guard below is only as good as this. If it starts to call a
  // translucent fill opaque, every assertion that uses it passes in silence.
  const themes = themeTokens();

  it("ACT-FR-KPWD: reads both themes' fills", () => {
    for (const theme of ["light", "dark"] as const) {
      expect(themes[theme].get("--bg-elevated"), theme).toMatch(/^#/);
      expect(themes[theme].get("--bg-hover"), theme).toMatch(/^rgba\(/);
    }
    expect(themes.dark.get("--bg-elevated")).not.toBe(themes.light.get("--bg-elevated"));
  });

  it("ACT-FR-KPWD: calls a translucent or unknown fill not opaque", () => {
    for (const theme of ["light", "dark"] as const) {
      const t = themes[theme];
      for (const value of [
        "var(--bg-hover)",
        "var(--bg-active)",
        "transparent",
        "none",
        "inherit",
        "currentColor",
        "var(--no-such-token)",
        "rgba(0,0,0,0.5)",
        "rgb(0 0 0 / 50%)",
        "#0000",
        "#00000080",
        "linear-gradient(var(--bg-hover), var(--bg-hover))",
        "var(--bg-elevated), var(--bg-hover)",
        "color-mix(in srgb, var(--bg-elevated), transparent)",
        "initial",
        "unset",
        "#fff0",
        "rgb(0 0 0 / .5)",
        "hsla(0, 0%, 100%, 0.5)",
      ]) {
        expect(isOpaqueFill(value, t), `${theme}: ${value}`).toBe(false);
      }
    }
  });

  it("ACT-FR-KPWD: calls an opaque fill opaque, also under a tint layer", () => {
    for (const theme of ["light", "dark"] as const) {
      const t = themes[theme];
      for (const value of [
        "var(--bg-elevated)",
        "var(--accent-soft)",
        "var(--no-such-token, var(--bg-elevated))",
        "#fff",
        "#ffffffff",
        "rgb(1, 2, 3)",
        "rgba(1, 2, 3, 1)",
        "rgb(1 2 3 / 100%)",
        "hsl(0, 0%, 100%)",
        "#ffff",
        "var(--bg-elevated) !important",
        "linear-gradient(var(--bg-hover), var(--bg-hover)), var(--bg-elevated)",
      ]) {
        expect(isOpaqueFill(value, t), `${theme}: ${value}`).toBe(true);
      }
    }
  });

  it("ACT-FR-KPWD: ends a token cycle as not opaque", () => {
    const cycle: Tokens = new Map([
      ["--a", "var(--b)"],
      ["--b", "var(--a)"],
    ]);
    expect(isOpaqueFill("var(--a)", cycle)).toBe(false);
  });

  it("ACT-FR-KPWD: takes a state of one element and nothing else", () => {
    expect(stateOf(".draft-actions__item:hover:not(:disabled)")).toBe(".draft-actions__item");
    expect(stateOf('.draft-actions__toggle[data-open="true"]')).toBe(".draft-actions__toggle");
    expect(stateOf(".draft-actions__item:focus-visible")).toBe(".draft-actions__item");
    expect(stateOf(".draft-actions__item")).toBeNull();
    expect(stateOf(".draft-actions__item::before")).toBeNull();
    expect(stateOf(".draft-actions__item:hover .icon")).toBeNull();
    expect(stateOf(".draft-actions__item:hover > span")).toBeNull();
  });
});

/** The surfaces ACT-FR-KPWD names, which must be found and kept opaque in every context. */
const ACTION_CONTROL_SURFACES = [
  ".draft-actions__item",
  ".draft-actions__toggle",
  ".draft-composer",
  ".action-control__panel",
  ".action-control__discussions",
];

/**
 * The boxes that hold the action control's surfaces. They paint no fill of
 * their own, but an `opacity` below 1 on one of them fades every surface in it.
 */
const ACTION_CONTROL_HOLDERS = [".action-control", ".action-control__dock", ".draft-actions__menu"];

/**
 * True when a compound selector selects `name`'s element in some form: the
 * class itself, a modifier of it (`name--danger`), or the class with a tag, a
 * second class, or a state on it (`button.name.is-busy:hover`).
 */
function selectsClass(compound: string, name: string): boolean {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`${escaped}(--[A-Za-z0-9_-]+)?(?![A-Za-z0-9_-])`).test(compound);
}

/** The last compound of a selector: the element the rule actually styles. */
function subjectOf(selector: string): string {
  const parts = selector.split(/\s*[\s>+~]\s*/);
  return parts[parts.length - 1];
}

interface Sweep {
  surfaces: Set<string>;
  checked: string[];
  failures: string[];
}

/**
 * Every rule that paints a floating surface, checked for a fill that is not
 * opaque and for an `opacity` below 1 in each theme.
 *
 * A rule paints a surface when it is the surface at rest or in a state. For
 * the action control's own surfaces it also does when a context selects it —
 * the same element under a host or a theme — because an override in a context
 * is the easiest place for a translucent fill to come back.
 */
function sweep(css: string, themes: Record<Theme, Tokens>): Sweep {
  const all = rules(css);
  const surfaces = floatingSurfaces(all, themes.light);
  const checked: string[] = [];
  const failures: string[] = [];
  for (const { selector, block } of all) {
    const subject = subjectOf(selector);
    const surface = selector === subject ? (stateOf(selector) ?? selector) : null;
    const contextual = ACTION_CONTROL_SURFACES.some((name) => selectsClass(subject, name));
    const holder = ACTION_CONTROL_HOLDERS.some((name) => selectsClass(subject, name));
    if (!(surface !== null && surfaces.has(surface)) && !contextual && !holder) continue;
    // A holder paints no fill, so only its `opacity` is its business.
    const fill = contextual || (surface !== null && surfaces.has(surface)) ? fillOf(block) : null;
    const opacity = block.match(/(?:^|;)\s*opacity\s*:\s*([^;]+)/i)?.[1].trim();
    if (fill === null && opacity === undefined) continue;
    checked.push(selector);
    const level = opacity?.endsWith("%")
      ? Number.parseFloat(opacity) / 100
      : Number.parseFloat(opacity ?? "1");
    if (!(level >= 1)) {
      failures.push(`${selector}: opacity ${opacity}`);
    }
    if (fill === null) continue;
    for (const theme of ["light", "dark"] as const) {
      if (!isOpaqueFill(fill, themes[theme])) {
        failures.push(`${selector} (${theme}): background ${fill}`);
      }
    }
  }
  return { surfaces, checked, failures };
}

describe("a floating surface stays opaque in every state (ACT-FR-KPWD)", () => {
  // The action control's menu lies over the discussion column. Its entries had
  // an opaque fill at rest and a translucent one on hover, so the text under
  // the menu showed through the entry the pointer was on. Vitest loads no
  // stylesheet, so no rendering test can see that. A guard on the rest state
  // alone did not see it either: the defect was in a state rule.
  const themes = themeTokens();
  const css = componentCss();

  it("ACT-FR-KPWD: splits every selector list where the browser does", () => {
    // A comma inside `:is()` or `:not()` would split one selector in two, and
    // the halves would match nothing. Fail here rather than check less.
    const broken = rules(css)
      .map((r) => r.selector)
      .filter((s) => (s.match(/\(/g)?.length ?? 0) !== (s.match(/\)/g)?.length ?? 0));
    expect(broken).toEqual([]);
  });

  it("ACT-FR-KPWD: finds the action control's surfaces as floating", () => {
    // The anchor against a guard that discovers nothing and so checks nothing.
    const { surfaces } = sweep(css, themes);
    for (const name of ACTION_CONTROL_SURFACES) {
      expect(surfaces.has(name), `${name} is not found as a floating surface`).toBe(true);
    }
  });

  it("ACT-FR-KPWD: paints each surface and each of its states opaque in each theme", () => {
    const { checked, failures } = sweep(css, themes);
    // The states that carried the defect and its neighbours are reached, so
    // the walk cannot stop matching in silence.
    expect(checked).toEqual(
      expect.arrayContaining([
        ...ACTION_CONTROL_SURFACES,
        ".draft-actions__item:hover:not(:disabled)",
        '.draft-actions__toggle[data-open="true"]',
        '.action-control__discussions[data-open="true"]',
      ]),
    );
    expect(failures).toEqual([]);
  });

  it("ACT-FR-KPWD: keeps the hover tint on a menu entry, on top of the opaque fill", () => {
    // The fix is a tint on the opaque fill. Dropping the tint would also make
    // the entry opaque, and would remove the hover feedback with it.
    const hover = blocksFor(sheet("components.css"), ".draft-actions__item:hover:not(:disabled)");
    expect(hover, "no hover rule for .draft-actions__item").toHaveLength(1);
    const layers = splitTop(fillOf(hover[0])!, ",");
    expect(layers.length).toBeGreaterThan(1);
    expect(layers[0]).toContain("var(--bg-hover)");
    expect(layers[layers.length - 1]).toBe("var(--bg-elevated)");
  });

  describe("catches the defect when it comes back", () => {
    // Each case changes a copy of the real stylesheet text, never the file.
    const hoverRule = /(\.draft-actions__item:hover:not\(:disabled\)\s*\{)[^}]*\}/;

    it("ACT-FR-KPWD: the hover entry with the translucent tint alone", () => {
      // The rule as it was when the defect shipped.
      expect(css).toMatch(hoverRule);
      const mutated = css.replace(
        hoverRule,
        "$1 background: var(--bg-hover); border-color: var(--border-3); }",
      );
      const { failures } = sweep(mutated, themes);
      expect(failures).toEqual(
        expect.arrayContaining([
          ".draft-actions__item:hover:not(:disabled) (light): background var(--bg-hover)",
          ".draft-actions__item:hover:not(:disabled) (dark): background var(--bg-hover)",
        ]),
      );
    });

    it("ACT-FR-KPWD: a transparent fill appended to a state, where the cascade takes it", () => {
      const mutated = css.replace(
        /(\.draft-actions__toggle\[data-open="true"\]\s*\{[^}]*)\}/,
        "$1; background-color: transparent; }",
      );
      expect(mutated).not.toBe(css);
      const { failures } = sweep(mutated, themes);
      expect(failures.some((f) => f.startsWith('.draft-actions__toggle[data-open="true"]'))).toBe(true);
    });

    it("ACT-FR-KPWD: a new focus state with a translucent fill", () => {
      const mutated = `${css}\n.draft-actions__item:focus-visible { background: var(--bg-active); }`;
      const { failures } = sweep(mutated, themes);
      expect(failures.some((f) => f.startsWith(".draft-actions__item:focus-visible"))).toBe(true);
    });

    it("ACT-FR-KPWD: a disabled state that fades the entry", () => {
      for (const value of ["0.5", "50%"]) {
        const mutated = `${css}\n.draft-actions__item:disabled { opacity: ${value}; }`;
        const { failures } = sweep(mutated, themes);
        expect(failures).toContain(`.draft-actions__item:disabled: opacity ${value}`);
      }
    });

    it("ACT-FR-KPWD: a pressed state with a translucent fill", () => {
      const mutated = `${css}\n.draft-actions__toggle:active { background: var(--bg-hover); }`;
      const { failures } = sweep(mutated, themes);
      expect(failures.some((f) => f.startsWith(".draft-actions__toggle:active"))).toBe(true);
    });

    it("ACT-FR-KPWD: a state written as a tag, a second class, or a modifier", () => {
      for (const selector of [
        "button.draft-actions__item:hover",
        ".draft-actions__item.is-busy",
        ".draft-actions__item--danger:hover",
      ]) {
        const mutated = `${css}\n${selector} { background: transparent; }`;
        const { failures } = sweep(mutated, themes);
        expect(failures.some((f) => f.startsWith(selector)), selector).toBe(true);
      }
    });

    it("ACT-FR-KPWD: a holder that fades every surface in it", () => {
      for (const selector of [".draft-actions__menu", ".action-control"]) {
        const mutated = `${css}\n${selector} { opacity: 0.9; }`;
        const { failures } = sweep(mutated, themes);
        expect(failures, selector).toContain(`${selector}: opacity 0.9`);
      }
    });

    it("ACT-FR-KPWD: a translucent fill in a host's or a theme's context", () => {
      const mutated = `${css}\n[data-theme="dark"] .draft-editor .draft-actions__item:hover { background: var(--bg-hover); }`;
      const { failures } = sweep(mutated, themes);
      expect(
        failures.some((f) => f.startsWith('[data-theme="dark"] .draft-editor .draft-actions__item:hover')),
      ).toBe(true);
    });
  });
});
