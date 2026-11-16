import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../../test/cssRules";

/**
 * The board's requirements that live in the stylesheet alone
 * (`DBR-delivery-board.md`). Vitest applies no CSS, so a component test
 * cannot see any of these.
 */
const css = sheet("kit.css");

/** The first rule for `selector`: the one outside the reduced-motion block. */
function first(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks.length, `expected a rule for ${selector}`).toBeGreaterThan(0);
  return blocks[0];
}

describe("the board stylesheet (DBR)", () => {
  it("DBR-FR-EEKD: where no stack takes the drop, the pointer shows that a drop is not allowed", () => {
    expect(decl(first('.flow[data-dragging="true"][data-refuse="true"]'), "cursor")).toBe("not-allowed");
  });

  it("DBR-FR-DQRX: the cards, the groups, and the placeholder slide to their slots; the dragged item does not", () => {
    for (const s of ['.flow[data-board="true"] .flow-bcard', '.flow[data-board="true"] .flow-ctr', '.flow[data-board="true"] .flow-bslot']) {
      const transition = decl(first(s), "transition") ?? "";
      expect(transition, s).toMatch(/left \d+ms/);
      expect(transition, s).toMatch(/top \d+ms/);
    }
    expect(decl(first('.flow[data-board="true"] [data-lifted="true"]'), "transition")).toBe("none");
  });

  it("DBR-FR-DQRX: with reduced motion the members take their slots with no animation", () => {
    for (const s of ['.flow[data-board="true"] .flow-bcard', '.flow[data-board="true"] .flow-ctr', '.flow[data-board="true"] .flow-bslot']) {
      const blocks = [...css.matchAll(/@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*?)\n\}/g)].flatMap(
        (m) => blocksFor(m[1], s),
      );
      expect(blocks, s).toHaveLength(1);
      expect(decl(blocks[0], "transition")).toBe("none");
    }
  });

  it("DBR-FR-XOOU: the address truncates at its start, so the issue number always shows", () => {
    const addr = first(".flow-baddr");
    expect(decl(addr, "direction")).toBe("rtl");
    expect(decl(addr, "text-overflow")).toBe("ellipsis");
    expect(decl(addr, "white-space")).toBe("nowrap");
    expect(decl(first(".flow-baddr__text"), "unicode-bidi")).toBe("isolate");
  });

  it("DBR-FR-LVBY: the name takes one line and ends in an ellipsis", () => {
    const name = first(".flow-bcard .flow-bcard__name");
    expect(decl(name, "white-space")).toBe("nowrap");
    expect(decl(name, "text-overflow")).toBe("ellipsis");
  });

  it("DBR-FR-LNZI, DBR-FR-QZPA: a card has three rows and a 3px stripe in the colour of its state", () => {
    expect(decl(first(".flow-bcard"), "grid-template-rows")?.split(/\s+/)).toHaveLength(3);
    expect(decl(first(".flow-bcard::before"), "width")).toBe("3px");
    expect(decl(first('.flow-bcard[data-state="done"]::before'), "background")).toBe("var(--ok)");
  });
});
