import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Conformance test for the app's motion policy.
 *
 * An `animation: … infinite` never stops on its own. While the element carrying
 * it is mounted, the browser produces a frame every display refresh, and the
 * compositor re-uploads the window surface to match — a cost paid continuously,
 * on a window nobody is touching. On a high-refresh Retina display that is
 * enough to saturate a core in the OS compositor while the app process itself
 * looks idle, which is exactly how the cost hides.
 *
 * Two rules follow, and both fail silently if broken — the UI looks correct and
 * only a power meter disagrees:
 *
 *   1. Every infinite animation must be reachable only from a transient state
 *      (a run in flight, an operation in progress, a refresh underway). That is
 *      a property of the components, asserted where each is rendered.
 *   2. Every infinite animation must have a `prefers-reduced-motion: reduce`
 *      escape that actually applies to it. That is a property of the
 *      stylesheets, and this file asserts it — for every rule now and every rule
 *      added later.
 *
 * The selector list is derived from the stylesheets, never hardcoded: a literal
 * list would pass forever while the next unguarded animation sailed past it.
 *
 * Known limits, so nobody reads a green run as "nothing animates":
 *   - Only `src/styles/*.css` is scanned. An animation introduced by inline
 *     `style`, by CSS-in-JS, or by an SVG `<animate>` element is invisible here.
 *   - A rule and its guard must live in the same file. That is the convention
 *     today; a guard moved to a shared sheet would fail rather than pass.
 *   - The parser is deliberately strict. Every construct it cannot parse with
 *     confidence throws or fails — it never shrugs and returns green.
 */

// Vitest runs with the project root as cwd, matching src/test/ci-workflow.test.ts
// and src/styles/shellGrid.test.ts.
const STYLES_DIR = resolve(process.cwd(), "src/styles");

/**
 * The one prelude that counts as a guard. Matched structurally rather than by
 * substring, because a substring test would accept `@media not (…reduce)` —
 * which means the exact opposite — and `@media (…reduce) and (min-width: 900px)`,
 * which only guards wide screens. It also accepts the minified spelling, so
 * running the sheets through a minifier does not turn this suite red.
 */
const GUARD_PRELUDE = /^@media\s*\(\s*prefers-reduced-motion\s*:\s*reduce\s*\)$/;

/**
 * An animation that never stops, written any of the ways CSS allows: the
 * `animation` shorthand carrying `infinite`, the `animation-iteration-count`
 * longhand, or a count so large it is infinite in practice. Case-insensitive —
 * CSS keywords are, and `INFINITE` would otherwise be a silent bypass.
 */
const INFINITE =
  /(?:animation:[^;{}]*\binfinite\b|animation-iteration-count:\s*(?:infinite|\d{4,}))/i;
const INFINITE_GLOBAL = new RegExp(INFINITE.source, "gi");

/** Strip `/* … *\/` comments so prose about animations is never parsed as CSS. */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** A span of CSS together with its offset in the original file. */
interface Segment {
  text: string;
  offset: number;
}

/**
 * Split a stylesheet into the reduced-motion guard blocks and everything else,
 * keeping each span's original offset so cascade order stays checkable.
 *
 * Brace matching is done by scanning rather than by regex: a guard block
 * contains nested rules, and a non-greedy `\}` would end it at the first inner
 * closing brace, silently treating the rest of the guard as unguarded CSS.
 *
 * @throws if the braces do not balance. A hand-rolled parser that cannot parse
 * must fail loudly — swallowing it would move the unparsed remainder of the file
 * into whichever bucket it was mid-way through, and an entire sheet's worth of
 * animations would go unchecked with the suite still green.
 */
function splitGuards(
  css: string,
  file: string,
): { base: Segment[]; guards: Segment[] } {
  const base: Segment[] = [];
  const guards: Segment[] = [];
  let i = 0;
  while (i < css.length) {
    const at = css.indexOf("@media", i);
    if (at === -1) {
      base.push({ text: css.slice(i), offset: i });
      break;
    }
    const open = css.indexOf("{", at);
    if (open === -1) {
      throw new Error(`${file}: '@media' at ${at} has no block`);
    }
    let depth = 0;
    let end = open;
    for (; end < css.length; end += 1) {
      if (css[end] === "{") depth += 1;
      else if (css[end] === "}") {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    if (depth !== 0) {
      throw new Error(`${file}: unbalanced braces in '@media' at ${at}`);
    }
    const prelude = css.slice(at, open).trim();
    base.push({ text: css.slice(i, at), offset: i });
    // A non-guard at-rule (a colour-scheme query, say) keeps its inner rules in
    // `base`, so those rules are still checked; only the wrapper is dropped.
    const body = { text: css.slice(open + 1, end), offset: open + 1 };
    if (GUARD_PRELUDE.test(prelude)) guards.push(body);
    else base.push(body);
    i = end + 1;
  }
  return { base, guards };
}

interface Rule {
  /** Normalised full selector text, e.g. `.a, .b`. */
  selector: string;
  /** The individual selectors it applies to. */
  selectors: string[];
  body: string;
  /** Offset in the original file, for cascade-order checks. */
  index: number;
}

/**
 * Every innermost `selector { declarations }` block. Innermost is what the
 * regex finds, because a declaration body is the only thing containing no
 * further braces — so at-rule wrappers are skipped over rather than matched.
 *
 * `@keyframes` steps (`0%, 100% { … }`) are innermost blocks too and are
 * returned here. That is harmless: `animation` is not an animatable property, so
 * a step body can never legally carry one, and a step selector reaching the
 * guard side could only ever cause a failure, never a pass.
 */
function rules(segments: Segment[]): Rule[] {
  const out: Rule[] = [];
  for (const segment of segments) {
    const re = /([^{}]+)\{([^{}]*)\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(segment.text)) !== null) {
      const selector = m[1].trim().replace(/\s+/g, " ");
      out.push({
        selector,
        selectors: selector.split(",").map((s) => s.trim()).filter(Boolean),
        body: m[2],
        index: segment.offset + (m.index ?? 0),
      });
    }
  }
  return out;
}

/**
 * Every stylesheet the app loads, as paths relative to `src/styles`.
 *
 * The sweep recurses: `components.css` and `kit.css` are lists of imports
 * rather than rules, so a walk of the top level alone would read no animation
 * at all and every guard below would pass without seeing anything.
 */
function sheetPaths(dir: string, prefix = ""): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...sheetPaths(resolve(dir, entry.name), rel));
    else if (entry.name.endsWith(".css")) out.push(rel);
  }
  return out;
}

const SHEETS = sheetPaths(STYLES_DIR).sort();

interface Sheet {
  file: string;
  animated: Rule[];
  guards: Rule[];
  /** Infinite-animation declarations found by scanning the raw file. */
  naiveCount: number;
}

const PARSED: Sheet[] = SHEETS.map((file) => {
  const source = stripComments(readFileSync(resolve(STYLES_DIR, file), "utf8"));
  const { base, guards } = splitGuards(source, file);
  return {
    file,
    animated: rules(base).filter((r) => INFINITE.test(r.body)),
    // A guard only counts if it actually turns the animation off. One that sets
    // `animation: none` and then re-declares an animation is not a guard.
    guards: rules(guards).filter(
      (r) => /animation:\s*none/.test(r.body) && !INFINITE.test(r.body),
    ),
    naiveCount: (source.match(INFINITE_GLOBAL) ?? []).length,
  };
});

describe("motion policy over src/styles/*.css", () => {
  it("finds the stylesheets it claims to check", () => {
    // A rename or a move would otherwise turn this whole file into a green
    // no-op that asserts nothing.
    expect(SHEETS.length).toBeGreaterThan(0);
  });

  it("accounts for every infinite animation in the app", () => {
    // The canary for the parser itself, and the reason it is per-file rather
    // than a global total: a bug that erased one sheet's contents would still
    // leave a global count above zero, and the suite would stay green while a
    // whole stylesheet went unchecked. Comparing the structured parse against a
    // naive whole-file scan is self-adjusting — it needs no hardcoded number and
    // grows with the stylesheets.
    for (const sheet of PARSED) {
      expect(
        sheet.animated.length,
        `${sheet.file}: parser found ${sheet.animated.length} infinite ` +
          `animation(s) but the file contains ${sheet.naiveCount}`,
      ).toBe(sheet.naiveCount);
    }
    expect(PARSED.reduce((n, s) => n + s.animated.length, 0)).toBeGreaterThan(0);
  });

  for (const sheet of PARSED) {
    for (const rule of sheet.animated) {
      it(`${sheet.file}: \`${rule.selector}\` animates infinitely and has a reduced-motion escape`, () => {
        // Every selector in the list needs cover, and the cover has to come
        // *after* the rule: an equal-specificity guard written earlier in the
        // file loses the cascade and never applies, which is exactly the shape
        // someone produces by consolidating guards at the top of a sheet.
        for (const selector of rule.selectors) {
          const cover = sheet.guards.filter(
            (g) => g.selectors.includes(selector) && g.index > rule.index,
          );
          expect(
            cover.length,
            `no reduced-motion guard for \`${selector}\` after its rule`,
          ).toBeGreaterThan(0);
        }
      });

      if (/will-change:/.test(rule.body)) {
        it(`${sheet.file}: \`${rule.selector}\` releases its compositor layer under reduced motion`, () => {
          // `will-change` promotes the element to its own layer so the animation
          // costs a composite rather than a repaint. Keeping that promotion once
          // the animation is switched off leaves a permanent layer behind for an
          // element that no longer moves — the cost with none of the benefit.
          for (const selector of rule.selectors) {
            const cover = sheet.guards.filter(
              (g) => g.selectors.includes(selector) && g.index > rule.index,
            );
            expect(cover.some((g) => /will-change:\s*auto/.test(g.body))).toBe(
              true,
            );
          }
        });
      }
    }
  }
});
