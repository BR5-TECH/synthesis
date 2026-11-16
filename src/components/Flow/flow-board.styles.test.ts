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
    for (const s of ['.flow[data-board="true"] .dm-card', '.flow[data-board="true"] .flow-ctr', '.flow[data-board="true"] .flow-bslot']) {
      const transition = decl(first(s), "transition") ?? "";
      expect(transition, s).toMatch(/left \d+ms/);
      expect(transition, s).toMatch(/top \d+ms/);
    }
    expect(decl(first('.flow[data-board="true"] [data-lifted="true"]'), "transition")).toBe("none");
  });

  it("DBR-FR-DQRX: with reduced motion the members take their slots with no animation", () => {
    for (const s of ['.flow[data-board="true"] .dm-card', '.flow[data-board="true"] .flow-ctr', '.flow[data-board="true"] .flow-bslot']) {
      const blocks = [...css.matchAll(/@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*?)\n\}/g)].flatMap(
        (m) => blocksFor(m[1], s),
      );
      expect(blocks, s).toHaveLength(1);
      expect(decl(blocks[0], "transition")).toBe("none");
    }
  });

  it("DBR-FR-LVBY: the name takes at most two lines and then ends in an ellipsis", () => {
    const name = first('.flow[data-board="true"] .dm-card__title');
    expect(decl(name, "-webkit-line-clamp")).toBe("2");
    expect(decl(name, "line-clamp")).toBe("2");
    expect(decl(name, "overflow")).toBe("hidden");
    expect(decl(name, "white-space")).toBe("normal");
  });

  it("DBR-FR-XOOU, DBR-FR-DZSU, DBR-FR-VNSJ: the address, the kind chip, and the counts use the monospace face", () => {
    for (const s of [".dm-card__ref", ".dm-card__side", ".dm-lane__count"]) {
      expect(decl(first(s), "font"), s).toMatch(/var\(--font-mono\)/);
    }
    expect(decl(first(".dm-kind"), "font")).toMatch(/var\(--font-mono\)/);
  });

  it("DBR-FR-DZSU: each kind chip takes the chip colours of its kind", () => {
    const chips = { epic: "scenario", feature: "skill", task: "harness", defect: "spec" };
    for (const [kind, chip] of Object.entries(chips)) {
      const block = first(`.dm-kind[data-kind="${kind}"]`);
      expect(decl(block, "background"), kind).toBe(`var(--chip-${chip}-bg)`);
      expect(decl(block, "color"), kind).toBe(`var(--chip-${chip}-fg)`);
    }
  });

  it("DBR-FR-QZPA: the status dot is 6px, with one colour for each of the five statuses and a pulse for in progress", () => {
    const dot = first(".dm-dot");
    expect(decl(dot, "width")).toBe("6px");
    expect(decl(dot, "height")).toBe("6px");
    expect(decl(dot, "background")).toBe("var(--fg-4)");
    expect(decl(first('.dm-dot[data-s="ready"]'), "background")).toBe("var(--accent)");
    expect(decl(first('.dm-dot[data-s="in progress"]'), "background")).toBe("var(--live)");
    expect(decl(first('.dm-dot[data-s="in progress"]'), "animation")).toMatch(/dm-pulse 1\.6s/);
    expect(decl(first('.dm-dot[data-s="in review"]'), "background")).toBe("var(--warn)");
    expect(decl(first('.dm-dot[data-s="done"]'), "background")).toBe("var(--ok)");
  });

  it("DBR-FR-CJSG: an error gives the card the danger border, and its strip is one line on the danger-soft ground", () => {
    expect(decl(first('.flow[data-board="true"] .dm-card[data-error="true"]'), "border-color")).toBe("var(--danger)");
    expect(decl(first('.flow[data-board="true"] .dm-group[data-error="true"]'), "border-color")).toBe("var(--danger)");
    expect(decl(first(".dm-card__error"), "background")).toBe("var(--danger-soft)");
    const text = first(".dm-card__error-text");
    expect(decl(text, "white-space")).toBe("nowrap");
    expect(decl(text, "text-overflow")).toBe("ellipsis");
  });

  it("DBR layout notes: a card has a 1px border and no shadow at rest; the selection is the accent and the focus ring; the dragged item has the raised shadow", () => {
    const card = blocksFor(css, '.flow[data-board="true"] .dm-card').find((b) => decl(b, "border") !== null)!;
    expect(decl(card, "border")).toBe("1px solid var(--border-1)");
    expect(decl(card, "box-shadow")).toBeNull();
    const selected = first('.flow[data-board="true"] .dm-card[data-selected="true"]');
    expect(decl(selected, "border-color")).toBe("var(--accent)");
    expect(decl(selected, "box-shadow")).toBe("var(--shadow-focus)");
    expect(decl(first('.flow[data-board="true"] .dm-card:hover'), "border-color")).toBe("var(--border-3)");
    expect(decl(first('.flow[data-board="true"] .dm-card[data-lifted="true"]'), "box-shadow")).toBe("var(--shadow-3)");
  });

  it("DBR-FR-SBLK, DBR-FR-BYQX: a group stack sits on the panel ground below a divider; a column is on the panel ground with a 1px border", () => {
    const body = first(".dm-group__body");
    expect(decl(body, "background")).toBe("var(--bg-panel)");
    expect(decl(body, "border-top")).toBe("1px solid var(--border-1)");
    const lane = first('.flow[data-board="true"] .dm-lane');
    expect(decl(lane, "background")).toBe("var(--bg-panel)");
    expect(decl(lane, "border")).toBe("1px solid var(--border-1)");
    expect(decl(first(".dm-lane__desc"), "-webkit-line-clamp")).toBe("2");
  });

  it("MDL-FR-XKCH: the popover is 340px on the elevated ground with a 1px border, and its sections take the uppercase label", () => {
    const pop = first(".flow-panel.dm-pop");
    expect(decl(pop, "width")).toBe("340px");
    expect(decl(pop, "background")).toBe("var(--bg-elevated)");
    expect(decl(pop, "border")).toBe("1px solid var(--border-2)");
    expect(decl(first(".dm-pop .flow-panel__label"), "text-transform")).toBe("uppercase");
    expect(decl(first(".dm-pop__foot"), "background")).toBe("var(--bg-panel)");
    expect(decl(first(".flow-panel__section"), "display")).toBe("contents");
  });

  it("DBR layout notes: the board styles take every colour from a token", () => {
    const own = sheet("kit/delivery-map.css") + sheet("kit/flow-board.css");
    expect(own).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(own).not.toMatch(/rgba?\(/);
    for (const m of own.matchAll(/border(?:-top|-bottom|-left|-right)?\s*:\s*([\d.]+)px/g)) expect(m[1]).toBe("1");
  });
});
