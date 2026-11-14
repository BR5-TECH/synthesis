import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../test/cssRules";

/**
 * The Delivery widget's visual requirements that live in the stylesheet alone.
 * Vitest applies no CSS, so a component test cannot see these.
 */
const css = sheet("kit.css");

function only(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks, `expected one rule for ${selector}`).toHaveLength(1);
  return blocks[0];
}

describe("the milestone card (DSH-FR-NNWM, DSH-FR-BBRC)", () => {
  it("DSH-FR-BBRC: the in progress · blocked line wraps inside the card and does not run past its border", () => {
    const line = only(".dash-delivery__current-line");
    expect(decl(line, "white-space")).toBe("normal");
    expect(decl(line, "overflow-wrap")).toBe("anywhere");
  });

  it("DSH-FR-NNWM: a card of the axis has the fixed width of its slot, so its content must wrap to fit", () => {
    const card = only(".dash-delivery__milestone");
    expect(decl(card, "box-sizing")).toBe("border-box");
    expect(decl(card, "white-space")).toBeNull();
  });
});
