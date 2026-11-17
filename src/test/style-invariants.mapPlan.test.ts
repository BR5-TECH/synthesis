import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";
import { readStylesheet } from "./readStylesheet";

/**
 * Stylesheet invariants of the Map tab and the review of a plan proposal
 * (`../../specifications/ui/SMP-specification-map.md` SMP-FR-AOZF,
 * `../../specifications/ui/MPV-map-proposal-review.md` MPV-FR-CTTA,
 * MPV-FR-KCOB, `../../specifications/ui/DBR-delivery-board.md` DBR-FR-WKNI).
 *
 * jsdom applies no stylesheet, so a component test passes the same with each
 * of these rules removed. In the app, a hidden live region that escaped the
 * transcript gave the tab a scroll range, and the Map tab scrolled up to show
 * an empty area under it.
 */

describe("the Map tab (SMP-FR-AOZF)", () => {
  it("SMP-FR-AOZF: the Map tab clips at its bounds and makes no scroll box", () => {
    const [smap] = blocksFor(sheet("components.css"), ".smap");
    expect(smap, "no rule for .smap").toBeDefined();
    expect(decl(smap, "overflow")).toBe("clip");
    expect(decl(smap, "height")).toBe("100%");
  });

  it("SMP-FR-AOZF: the text for assistive technology sits in a positioned box of the summary row", () => {
    const css = sheet("kit.css");
    const [hidden] = blocksFor(css, ".pp-sr-only");
    expect(decl(hidden, "position")).toBe("absolute");
    const positioned = blocksFor(css, ".pp-entry").some((b) => decl(b, "position") === "relative");
    expect(positioned).toBe(true);
  });
});

describe("the review sheet (MPV-FR-CTTA, MPV-FR-KCOB)", () => {
  it("MPV-FR-CTTA: the sheet is centred over the canvas area at 90% and has the overlay shadow", () => {
    const [block] = blocksFor(sheet("kit.css"), ".pp-sheet");
    expect(decl(block, "position")).toBe("absolute");
    expect(decl(block, "width")).toBe("90%");
    expect(decl(block, "height")).toBe("90%");
    expect(decl(block, "box-shadow")).toBe("var(--shadow-4)");
  });

  it("MPV-FR-CTTA: the canvas area that hosts the sheet is positioned", () => {
    const css = sheet("components.css");
    const positioned = blocksFor(css, ".dlens-main").some((b) => decl(b, "position") === "relative");
    expect(positioned).toBe(true);
  });

  it("MPV-FR-KCOB: the summary ends in an ellipsis and the row never overflows", () => {
    const css = sheet("kit.css");
    const [sum] = blocksFor(css, ".pp-entry__sum");
    expect(decl(sum, "text-overflow")).toBe("ellipsis");
    expect(decl(sum, "min-width")).toBe("0");
    const [entry] = blocksFor(css, ".pp-entry");
    expect(decl(entry, "overflow")).toBe("hidden");
  });

  it("MPV-FR-FDDK: every colour of the review comes from a token", () => {
    const own = readStylesheet("kit/proposals.css").replace(/\/\*[\s\S]*?\*\//g, "");
    expect(own).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(own).not.toMatch(/\brgba?\(/);
  });
});

describe("the ghost card (DBR-FR-WKNI)", () => {
  it("DBR-FR-WKNI: a ghost card on the board has a dashed border in the ok tone", () => {
    const [ghost] = blocksFor(sheet("kit.css"), '.flow[data-board="true"] .dm-card--ghost');
    expect(ghost, "no board rule for the ghost card").toBeDefined();
    expect(decl(ghost, "border-style")).toBe("dashed");
    expect(decl(ghost, "border-color")).toBe("var(--ok)");
  });
});
