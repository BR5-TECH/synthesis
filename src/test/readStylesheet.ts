import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const STYLES = resolve(process.cwd(), "src/styles");

/**
 * The full text of a stylesheet, with every `@import` it makes put in place.
 *
 * The sheets are split into parts to stay under the file-size ceiling, so
 * `kit.css` and `components.css` are lists of imports rather than the rules
 * themselves. The bundler joins them in import order and the browser sees one
 * sheet; a guard that read only the top file would see no rules at all and pass
 * vacuously, which is worse than no guard.
 *
 * Import order is cascade order, so the parts are put in place exactly where
 * their `@import` stands.
 */
export function readStylesheet(name: string): string {
  const expand = (path: string): string =>
    readFileSync(path, "utf8").replace(
      // The line break after the statement belongs to the statement: leaving it
      // behind would insert a blank line where the part's own text begins.
      /^@import\s+["']([^"']+)["']\s*;[ \t]*\r?\n?/gm,
      (_whole, target: string) => expand(resolve(dirname(path), target)),
    );
  return expand(resolve(STYLES, name));
}

/** Every part `name` is built from, as absolute paths, `name` itself first. */
export function stylesheetParts(name: string): string[] {
  const out: string[] = [];
  const walk = (path: string) => {
    out.push(path);
    for (const match of readFileSync(path, "utf8").matchAll(
      /^@import\s+["']([^"']+)["']\s*;\s*$/gm,
    )) {
      walk(resolve(dirname(path), match[1]));
    }
  };
  walk(resolve(STYLES, name));
  return out;
}
