import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { readStylesheet, stylesheetParts } from "./readStylesheet";

/**
 * The stylesheets are split into parts, and every test that reads one must read
 * it through `readStylesheet`.
 *
 * `kit.css` and `components.css` are lists of `@import` rather than rules, so a
 * test that opens one directly finds no declarations at all. It does not throw:
 * a guard written as "this rule must say X" reads nothing, matches nothing, and
 * fails; a guard written as "nothing may say Y" reads nothing and **passes**,
 * silently, for ever. Nine such readers existed when the sheets were split, and
 * the ones that would have gone quiet are the reason this test exists.
 */
const SRC = "src";

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) out.push(...sourceFiles(path));
    else if (/\.(ts|tsx)$/.test(entry.name)) out.push(path);
  }
  return out;
}

describe("every stylesheet read goes through the helper", () => {
  it("finds no test opening a split stylesheet by path", () => {
    // The helper itself is where the path belongs.
    const offenders = sourceFiles(SRC)
      .filter((path) => path !== "src/test/readStylesheet.ts")
      .filter((path) => {
        const source = readFileSync(path, "utf8");
        // A bare mention in a comment is fine; a read is not.
        return /readFileSync\([^)]*styles\/(kit|components|colors_and_type)\.css/s.test(
          source,
        );
      });
    expect(
      offenders,
      "these read a stylesheet directly and would see only its @import list",
    ).toEqual([]);
  });

  it("puts every part of a split sheet in place, in cascade order", () => {
    for (const name of ["kit.css", "components.css"]) {
      const parts = stylesheetParts(name);
      expect(parts.length, `${name} names no parts`).toBeGreaterThan(1);
      const whole = readStylesheet(name);
      // The barrel contributes nothing but imports, so the whole is the parts.
      const measured = parts
        .slice(1)
        .reduce((n, part) => n + readFileSync(part, "utf8").length, 0);
      expect(whole.length, `${name} lost bytes on the way in`).toBe(measured);
    }
  });
});
