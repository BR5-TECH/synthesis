/** What the terminal row menu styles must hold, which jsdom cannot see (`TRM-terminals.md`). */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../styles/components/terminals.css"), "utf8");
const shared = readFileSync(resolve(__dirname, "../styles/components/header.css"), "utf8");

function block(selector: string): string {
  const at = css.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return css.slice(at, css.indexOf("}", at));
}

describe("the terminal row menu styles", () => {
  it("TRM-FR-EKGK: an entry of the row menu shows no native border, fill, or font", () => {
    const entry = block(".terminal-menu .menu-item");
    expect(entry).toMatch(/border: 0;/);
    expect(entry).toMatch(/background: transparent;/);
    expect(entry).toMatch(/font-family: inherit;/);
    expect(entry).toMatch(/appearance: none;/);
    // The shorthand would put back the font size of `.menu-item`, and a color
    // would hide the color of a danger entry.
    expect(entry).not.toMatch(/\bfont:/);
    expect(entry).not.toMatch(/\bcolor:/);
  });

  it("TRM-FR-EKGK: an entry keeps the row height of the div entries and fills the row without a width", () => {
    expect(block(".terminal-menu .menu-item")).toMatch(/box-sizing: content-box;/);
    expect(block(".terminal-menu .menu-item")).not.toMatch(/\bwidth:/);
    expect(block('.terminal-menu > [role="none"]')).toMatch(/flex-direction: column;/);
  });

  it("TRM-FR-EKGK: an entry shows the full-row highlight on hover and on keyboard focus, a disabled entry not on hover", () => {
    expect(block(".terminal-menu .menu-item:not(.menu-item--disabled):hover")).toMatch(/background: var\(--bg-active\)/);
    expect(css).toMatch(/\.terminal-menu \.menu-item:focus \{ outline: none; background: var\(--bg-active\); \}/);
  });

  it("TRM-FR-EKGK: the button reset stays in the row menu and does not change the other menus", () => {
    expect(shared).not.toMatch(/button\.menu-item/);
  });
});
