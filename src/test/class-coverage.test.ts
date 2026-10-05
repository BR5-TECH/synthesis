import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "./readStylesheet";

/**
 * Every class a component names has a rule behind it.
 *
 * This is the guard the Graduate window went without: it asked for a
 * `modal-scrim`, no stylesheet declared one, and the window rendered with no
 * backdrop and no centring — in a suite that stayed green, because Vitest
 * applies no CSS. The same slip had left an `icon-button` where the kit calls
 * it `icon-btn`, and a `t-caption` where the role is `t-ui-xs`.
 *
 * A class passes when a **selector** names it. Not when a comment mentions it,
 * and not because the block it belongs to is styled: `.modal__scrim` under a
 * styled `.modal` is the same defect as `modal-scrim` and must fail the same
 * way. What is left over is the list below — every name in the tree today that
 * carries no rule of its own — so a new one is a decision somebody takes rather
 * than a blanket it slips under.
 */

const STYLES = resolve(process.cwd(), "src/styles");

/**
 * The selector text of every rule in the stylesheets.
 *
 * Comments are stripped first, and declarations are dropped, so a class named
 * in prose or inside a `url()` cannot answer for a class a component uses.
 */
function selectorText(): string {
  const sheets = ["colors_and_type.css", "components.css", "kit.css"]
    .map((name) => readStylesheet(name))
    .join("\n");
  const withoutComments = sheets.replace(/\/\*[\s\S]*?\*\//g, " ");
  return [...withoutComments.matchAll(/(?:^|\})([^{}]*)\{/g)]
    .map((match) => match[1])
    .join(" ");
}

/**
 * Names that carry no rule of their own, each for a reason.
 *
 * Three kinds: a hook something queries the DOM by, a block whose children
 * carry the whole treatment, and a modifier that changes nothing visually and
 * exists for a test or for a data attribute to read. A name joins this list by
 * being looked at, which is the point of the list.
 */
const UNSTYLED = new Set([
  // Queried with `closest()` to decide what a click landed in.
  "drafts-overlay",
  // Blocks whose own box needs no rule: their children carry the treatment.
  "docker-images",
  "rollback-confirm",
  "search-results",
  "draft-info__body",
  // Bases whose modifiers carry the whole treatment.
  "hl",
  // Modifiers that change nothing visually, kept as handles.
  "activity-bar__cluster--bottom",
  "changes-actions__split",
  "changes-state__error",
  "comment-card__detach",
  "comment-card__maximize",
  "comment-composer__control",
  "conversation-overlay__attach",
  "conversation-overlay__state",
  "diff-line__gutter--new",
  "diff-line__gutter--old",
  "search-state--empty",
  "search-state--idle",
  "search-state--running",
  "tab--home",
]);

/** Every `className` a component sets, with the files that set it. */
function namedClasses(): Map<string, string[]> {
  const found = new Map<string, string[]>();
  const walk = (dir: string): string[] => {
    const out: string[] = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) out.push(...walk(path));
      else if (entry.name.endsWith(".tsx") && !entry.name.includes(".test."))
        out.push(path);
    }
    return out;
  };
  for (const path of walk(resolve(process.cwd(), "src"))) {
    const source = readFileSync(path, "utf8");
    // A plain literal or a template, which is how all but a handful of these
    // are written. A class a condition or a constant computes is outside what
    // a read of the source can settle, and is left to the reader.
    for (const match of source.matchAll(
      /className=(?:"([^"]*)"|\{`([^`]*)`\})/g,
    )) {
      // The static halves of a template only: what an interpolation computes
      // is not a name this test can read.
      const text = (match[1] ?? match[2] ?? "").replace(/\$\{[^}]*\}/g, " ");
      for (const token of text.split(/\s+/)) {
        // A fragment ending in a separator is half a name the interpolation
        // completes.
        if (!/^[A-Za-z][A-Za-z0-9_-]*$/.test(token)) continue;
        if (token.endsWith("-") || token.endsWith("_")) continue;
        found.set(token, [...(found.get(token) ?? []), path]);
      }
    }
  }
  return found;
}

describe("every class a component names is one the stylesheets declare", () => {
  const declared = new Set(
    [...selectorText().matchAll(/\.([A-Za-z][A-Za-z0-9_-]*)/g)].map((m) => m[1]),
  );

  it("no component asks for a class no selector declares", () => {
    const unanswered: string[] = [];
    for (const [token, files] of namedClasses()) {
      if (declared.has(token) || UNSTYLED.has(token)) continue;
      unanswered.push(`${token} (${files[0].replace(`${process.cwd()}/`, "")})`);
    }
    expect(unanswered).toEqual([]);
  });

  it("the guard reads selectors rather than the prose around them", () => {
    // A class named only in a comment must not answer for one a component
    // uses: `.draft-composer__actions` appears in kit.css in prose alone.
    expect(declared.has("draft-composer__actions")).toBe(false);
    // And a real rule is still found.
    expect(declared.has("scrim")).toBe(true);
    expect(declared.has("modal__actions")).toBe(true);
  });

  it("the list of unstyled names holds nothing that has since been styled", () => {
    // A name that gained a rule leaves the list rather than sitting on it,
    // where it would hide the next slip of the same kind.
    expect([...UNSTYLED].filter((name) => declared.has(name))).toEqual([]);
  });
});
