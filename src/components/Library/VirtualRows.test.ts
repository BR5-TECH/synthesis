import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ROW_HEIGHT } from "./VirtualRows";

/**
 * LIB-FR-BXMZ: the windowing places each row at its index times
 * `ROW_HEIGHT`, so that value must be the height a `.tree-row` takes on
 * screen. jsdom lays nothing out, so the rule is read from the stylesheet.
 */
describe("the tree row height (LIB-FR-BXMZ)", () => {
  it("LIB-FR-BXMZ: ROW_HEIGHT is the height plus the vertical padding of .tree-row", () => {
    const css = readFileSync(join(process.cwd(), "src/styles/components/header.css"), "utf8");
    const rule = /\.tree-row\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(rule).not.toBe("");
    const height = Number(/(?:^|[;\s])height:\s*(\d+)px/.exec(rule)?.[1]);
    const padding = /padding:\s*([^;]+);/.exec(rule)?.[1].trim().split(/\s+/) ?? [];
    const top = parseFloat(padding[0] ?? "0");
    const bottom = parseFloat(padding[2] ?? padding[0] ?? "0");
    expect(/box-sizing:\s*border-box/.test(rule)).toBe(false);
    expect(ROW_HEIGHT).toBe(height + top + bottom);
  });
});
