import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the Git panel
 * (`../../specifications/ui/GIT-git.md`).
 *
 * jsdom computes no layout, so no rendering test can see these. Each one is a
 * fact about the stylesheet that, reverted, leaves every component assertion
 * green while the panel is wrong on screen.
 */

const style = () => sheet("kit.css");
const last = (selector: string, prop: string) => {
  const found = blocksFor(style(), selector)
    .map((block) => decl(block, prop))
    .filter((value): value is string => value !== null);
  return found.length === 0 ? null : found[found.length - 1];
};

describe("the left rail at its narrowest width (GIT-FR-WQHD)", () => {
  it("GIT-FR-WQHD: the section controls wrap onto further rows", () => {
    expect(last(".git__tabs", "display")).toBe("flex");
    expect(last(".git__tabs", "flex-wrap")).toBe("wrap");
  });

  it("GIT-FR-WQHD: no section control scrolls, clips or truncates", () => {
    for (const selector of [".git__tabs", ".git__tab", ".bottom-tab"]) {
      for (const prop of ["overflow", "overflow-x"]) {
        const value = last(selector, prop);
        expect(value === null || value === "visible", `${selector} ${prop}`).toBe(true);
      }
      expect(last(selector, "text-overflow"), `${selector} text-overflow`).toBeNull();
    }
    // A control keeps its full label on one line.
    expect(last(".bottom-tab", "white-space")).toBe("nowrap");
  });

  it("GIT-FR-WQHD: the rail is 260px wide and never scrolls sideways", () => {
    expect(last(".git", "grid-template-columns")).toBe("260px 1fr");
    const x = last(".git__left", "overflow-x");
    expect(x === "hidden" || x === null || x === "visible").toBe(true);
    expect(x === "auto" || x === "scroll").toBe(false);
  });
});

describe("the large views scroll inside the panel (GIT-FR-06, GIT-FR-FNQA)", () => {
  it("GIT-FR-FNQA: the pull request view scrolls vertically within the panel", () => {
    expect(last(".git-pr__scroll", "overflow-y")).toBe("auto");
    expect(last(".git-pr__scroll", "min-height")).toBe("0");
    expect(last(".git__right", "overflow")).toBe("hidden");
  });

  it("GIT-FR-JRYS: the commit diff and the file list scroll inside their regions", () => {
    expect(last(".git-log__diff", "overflow")).toBe("auto");
    expect(last(".git-log__files", "overflow-y")).toBe("auto");
    expect(last(".git-log__view", "min-height")).toBe("0");
  });
});

describe("the panel's overlays and menu (GIT-FR-ZEKI, GIT-FR-VCDG)", () => {
  it("GIT-FR-VCDG: the overlay body scrolls inside the window instead of growing past the screen", () => {
    expect(last(".git-overlay", "max-height")).toContain("100vh");
    expect(last(".git-overlay__body", "overflow-y")).toBe("auto");
  });

  it("GIT-FR-ZEKI: the menu keeps the shared menu frame and shows a focus ring on its entries", () => {
    expect(blocksFor(sheet("components.css"), ".menu").length).toBeGreaterThan(0);
    expect(last(".git-menu__item:focus-visible", "background")).toBe("var(--bg-active)");
  });
});
