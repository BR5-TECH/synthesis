import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-DWGS,
 * CPR-FR-ODBY).
 *
 * jsdom computes no layout, so no rendering test can see these. Each one is a
 * fact about the stylesheet that, reverted, leaves every component assertion
 * green while the window is wrong on screen.
 */

const style = () => sheet("components.css");
const last = (selector: string, prop: string) => {
  const found = blocksFor(style(), selector)
    .map((block) => decl(block, prop))
    .filter((value): value is string => value !== null);
  return found.length === 0 ? null : found[found.length - 1];
};

describe("the frame of the window (CPR-FR-DWGS)", () => {
  it("CPR-FR-DWGS: the window is 880 px wide, at most the main window less 32 px each way", () => {
    expect(last(".modal.create-pr", "width")).toBe("880px");
    // The border is in the width, or the window is 882 px and 2 px past the cap.
    expect(last(".modal.create-pr", "box-sizing")).toBe("border-box");
    expect(last(".modal.create-pr", "max-width")).toBe("calc(100vw - 32px)");
    expect(last(".modal.create-pr", "max-height")).toBe("calc(100vh - 32px)");
  });

  it("CPR-FR-DWGS, CPR-FR-ODBY: a flex column whose body alone scrolls, so the action row stays visible", () => {
    expect(last(".modal.create-pr", "display")).toBe("flex");
    expect(last(".modal.create-pr", "flex-direction")).toBe("column");
    expect(last(".modal.create-pr > *", "flex")).toBe("none");
    expect(last(".modal.create-pr > .create-pr__body", "flex")).toBe("1 1 auto");
    expect(last(".modal.create-pr > .create-pr__body", "min-height")).toBe("0");
    expect(last(".modal.create-pr > .create-pr__body", "overflow-y")).toBe("auto");
  });
});

describe("the description (CPR-FR-ODBY)", () => {
  it("CPR-FR-ODBY: the description takes the free height of the body", () => {
    expect(last(".modal.create-pr > .create-pr__body", "display")).toBe("flex");
    expect(last(".create-pr__body > .create-pr__description", "flex")).toBe("1 0 auto");
    expect(last(".create-pr__panel", "flex")).toBe("1 0 auto");
    expect(last(".create-pr__textarea", "flex")).toBe("1 0 auto");
  });

  it("CPR-FR-ODBY: the description is a column, so its panel can grow", () => {
    expect(last(".create-pr__body > .create-pr__description", "display")).toBe("flex");
    expect(last(".create-pr__body > .create-pr__description", "flex-direction")).toBe("column");
  });

  it("CPR-FR-ILFN: a hidden tab panel shows nothing, so Write and Preview never show together", () => {
    expect(last(".create-pr__panel", "display")).toBe("flex");
    expect(last(".create-pr__panel[hidden]", "display")).toBe("none");
  });

  it("CPR-FR-ODBY: the field is at least twelve lines high", () => {
    const min = last(".create-pr__textarea", "min-height");
    expect(min).toMatch(/^calc\(12 \* 1\.5 \* var\(--fs-ui-md\)/);
  });
});
