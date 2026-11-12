import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../test/cssRules";

/**
 * The layout of the failure notice that lives in the stylesheet alone
 * (`../../specifications/ui/OFN-openspec-failure-notices.md`). Vitest applies
 * no CSS, so a component test cannot see these.
 */
const css = sheet("components.css");

function only(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks, `expected one rule for ${selector}`).toHaveLength(1);
  return blocks[0];
}

describe("the width of the notice (OFN layout notes)", () => {
  it("OFN-FR-IHSQ: the notice takes the full width of its host, so opening Details does not move the Details button", () => {
    const notice = only(".ofn");
    expect(decl(notice, "align-self")).toBe("stretch");
    expect(decl(notice, "width")).toBe("100%");
    expect(decl(notice, "min-width")).toBe("0");
    expect(decl(notice, "box-sizing")).toBe("border-box");
    // Details stands at the end of the action line, against the full width.
    expect(decl(only(".ofn__toggle"), "margin-left")).toBe("auto");
  });

  it("OFN-FR-IHSQ, SMP-FR-KBXW: on the Spec Map the notice keeps one width and stands in the centre", () => {
    const notice = only(".smap-state__notice");
    expect(decl(notice, "max-width")).toBe("36rem");
    expect(decl(notice, "margin-inline")).toBe("auto");
  });

  it("OFN-FR-IHSQ: each line of Details keeps its spaces, and the newline between two lines adds no empty line", () => {
    expect(decl(only(".ofn__line"), "white-space")).toBe("pre-wrap");
    expect(decl(only(".ofn__diagnostic"), "white-space")).toBe("normal");
    expect(decl(only(".ofn__diagnostics"), "white-space")).toBeNull();
  });
});

describe("the note of a stage (OFN-FR-QZEC)", () => {
  it("OFN-FR-QZEC: the short form under a stage row is one line, cut with an ellipsis", () => {
    const note = only(".run-progress__note");
    expect(decl(note, "white-space")).toBe("nowrap");
    expect(decl(note, "overflow")).toBe("hidden");
    expect(decl(note, "text-overflow")).toBe("ellipsis");
  });
});
