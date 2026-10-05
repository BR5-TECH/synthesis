import { readStylesheet } from "./readStylesheet";

/**
 * A small, strict reader for asserting against the real stylesheets.
 *
 * Vitest runs with `css: false`, so jsdom never applies a stylesheet: the DOM a
 * component test inspects is identical whether a rule is present, absent, or
 * overridden. Every guard written against this reader was found by driving the
 * real app in a browser, and every one would have shipped again — silently,
 * with a green suite — without an assertion at the source.
 *
 * The parser is deliberately small and strict. It reads declaration blocks by
 * selector; anything it cannot find fails rather than passing vacuously, since
 * a guard that quietly stops matching is worse than no guard.
 */
export function sheet(name: string): string {
  // Comments are stripped so a rule quoted in prose cannot satisfy a guard.
  return readStylesheet(name).replace(/\/\*[\s\S]*?\*\//g, "");
}

/**
 * Every declaration block whose selector list contains `selector` exactly.
 * Returns one entry per rule, so a class defined twice is visible as two.
 */
export function blocksFor(css: string, selector: string): string[] {
  const out: string[] = [];
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selectors = m[1].split(",").map((s) => s.trim().replace(/\s+/g, " "));
    if (selectors.includes(selector)) out.push(m[2]);
  }
  return out;
}

/**
 * The value of `prop` in a block, or null when the block does not set it.
 *
 * The *last* occurrence, because that is the one the cascade takes. Reading the
 * first would un-guard every assertion against the commonest way a rule is
 * undone: appending the old declaration to the end of the block, which is what
 * a merge-conflict resolution and a "just add the line" edit both do.
 */
export function decl(block: string, prop: string): string | null {
  const found = [
    ...block.matchAll(new RegExp(`(?:^|;)\\s*${prop}\\s*:\\s*([^;]+)`, "gi")),
  ];
  return found.length === 0 ? null : found[found.length - 1][1].trim();
}

/** Every `z-index` declared on a rule whose selector starts with `prefix`. */
export function layersOf(css: string, prefix: RegExp): number[] {
  const out: number[] = [];
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selectors = m[1].split(",").map((s) => s.trim());
    if (!selectors.some((s) => prefix.test(s))) continue;
    const z = decl(m[2], "z-index");
    if (z !== null && z !== "auto") out.push(Number(z));
  }
  return out;
}
