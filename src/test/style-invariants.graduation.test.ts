import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the graduation run region's layout: the pinned foot
 * bar, the bounded path lists, the open pass row that stays in view, and the
 * two columns of a wide region.
 *
 * One part of the group `./cssRules.ts` heads. jsdom applies no stylesheet, so
 * a component test passes the same with each of these rules removed.
 */

describe("the graduation run region's layout", () => {
  it("GRU-FR-OZAR: the action row is a foot bar pinned to the region's bottom edge", () => {
    const css = sheet("components.css");
    const [foot] = blocksFor(css, ".graduation__foot");
    expect(foot, "no rule for .graduation__foot").toBeDefined();
    expect(decl(foot, "position")).toBe("sticky");
    // At the region's bottom edge, not at its padding: a sticky box is held
    // inside the scroller's padding, so it sticks at minus that padding.
    expect(decl(foot, "bottom")).toBe("-8px");
    // A scrolling box in a flex column shrinks to a strip without this.
    expect(decl(foot, "flex")).toBe("none");
    // Held at the foot of a region whose content is short.
    expect(decl(foot, "margin")).toMatch(/^auto\b/);
    // Opaque, so the content that scrolls under it does not show through.
    expect(decl(foot, "background")).toBe("var(--bg-panel)");
    // A tall foot scrolls inside itself rather than filling a short region.
    expect(decl(foot, "max-height")).toBe("50%");
    expect(decl(foot, "overflow-y")).toBe("auto");
    // The region it is pinned in is what scrolls, and a control the keyboard
    // moves to is scrolled clear of the foot rather than under it.
    const regions = blocksFor(css, ".graduation__run");
    expect(decl(regions[0], "overflow")).toBe("auto");
    // `margin-top: auto` holds the foot down only in a flex column.
    expect(decl(regions[0], "display")).toBe("flex");
    expect(decl(regions[0], "flex-direction")).toBe("column");
    // Above the open pass row that sticks in the content it covers.
    const [row] = blocksFor(
      css,
      '.graduation__pass[data-open="true"] > .graduation__pass-row',
    );
    expect(Number(decl(foot, "z-index"))).toBeGreaterThan(Number(decl(row, "z-index")));
    expect(regions.map((block) => decl(block, "scroll-padding-bottom"))).toContain("56px");
  });

  it("GRU-FR-TXLW: a path list scrolls inside a bounded box and never sideways", () => {
    const css = sheet("components.css");
    const [box] = blocksFor(css, ".graduation__paths");
    expect(box, "no rule for .graduation__paths").toBeDefined();
    expect(decl(box, "max-height")).toBe("240px");
    expect(decl(box, "overflow-y")).toBe("auto");
    expect(decl(box, "overflow-x")).toBe("hidden");
    // A long name ellipsizes in the row rather than widening the box.
    const [name] = blocksFor(css, ".tree-row__name");
    expect(decl(name, "text-overflow")).toBe("ellipsis");
    expect(decl(name, "white-space")).toBe("nowrap");
  });

  it("GRU-FR-VDSK: the open pass row stays at the top edge of the run region", () => {
    const css = sheet("components.css");
    const [row] = blocksFor(
      css,
      '.graduation__pass[data-open="true"] > .graduation__pass-row',
    );
    expect(row, "no rule for the open pass row").toBeDefined();
    expect(decl(row, "position")).toBe("sticky");
    // The region is the scroller it sticks in, and a sticky box is held inside
    // that scroller's padding: the row sticks at minus the region's top padding
    // to reach its top edge.
    const [region] = blocksFor(css, ".graduation__run");
    const regionTop = decl(region, "padding")?.split(/\s+/)[0];
    expect(regionTop).toBeTruthy();
    expect(decl(row, "top")).toBe(`-${regionTop}`);
    // The tint over the panel's ground: the tone alone is translucent, and the
    // account would show through the row it scrolls under.
    expect(decl(row, "background")).toBe(
      "linear-gradient(var(--bg-active), var(--bg-active)), var(--bg-panel)",
    );
  });

  it("GRU-FR-GXQE, GRU-FR-ZLWI: the pass history and its accounts have no height of their own", () => {
    const css = sheet("components.css");
    // A box that scrolls inside the region is a second scroll position for
    // one text, and one WebKit fails to repaint when an account closes. Every
    // rule on the history, its accounts and the columns that hold them is
    // read, compound selectors and container queries included.
    const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
      .map(([, selector, block]) => ({ selector: selector.trim(), block }))
      .filter(({ selector }) =>
        /graduation__(passes|pass|account|revision|columns)\b/.test(selector),
      );
    expect(rules.length).toBeGreaterThan(10);
    for (const { selector, block } of rules) {
      expect(decl(block, "max-height"), selector).toBeNull();
      // One line that ellipsizes clips its own text; it scrolls nothing.
      if (decl(block, "text-overflow") === "ellipsis") continue;
      for (const prop of ["overflow", "overflow-x", "overflow-y"]) {
        expect(decl(block, prop), `${selector} ${prop}`).toBeNull();
      }
    }
    // The boxes that hold an account take the height of what they hold.
    for (const selector of [
      ".graduation__passes",
      ".graduation__pass-list",
      ".graduation__pass",
      ".graduation__account",
      ".graduation__account-block",
      ".graduation__account-findings",
      ".graduation__revision-text",
    ]) {
      const blocks = blocksFor(css, selector);
      expect(blocks.length, `no rule for ${selector}`).toBeGreaterThan(0);
      for (const block of blocks) expect(decl(block, "height"), selector).toBeNull();
    }
    // No state of the region bounds the history either: a run that waits on
    // the author renders its escalation above the columns.
    expect(css).not.toMatch(/\[data-awaiting="true"\][^{]*\.graduation__pass/);
  });

  it("GRU-FR-MCYF, GRU-FR-KWRB: a region 880px wide or wider sets two columns, a narrower one stacks", () => {
    const css = sheet("components.css");
    const wide = css.match(
      /@container \(min-width: 856px\)\s*\{\s*\.graduation__columns\[data-split="true"\]\s*\{([^}]*)\}/,
    );
    expect(wide, "no two-column rule for a wide region").not.toBeNull();
    expect(decl(wide![1], "display")).toBe("grid");
    // GRU-FR-KWRB: the paths column takes the author's share, 30 % unless
    // they set another; the divider stands between; the passes take the rest.
    expect(decl(wide![1], "grid-template-columns")).toBe(
      "minmax(0, var(--graduation-paths, 30%)) auto minmax(0, 1fr)",
    );
    // The query reads the content box: 880px less the region's side padding.
    const [regionBox] = blocksFor(css, ".graduation__run");
    expect(decl(regionBox, "padding")).toBe("8px 12px");
    // Outside the query the region stacks.
    // Every rule outside the query keeps the stack: none of them sets a grid.
    const outside = blocksFor(css.replace(/@container[^{]*\{[\s\S]*?\n\}/g, ""), ".graduation__columns");
    expect(outside.length).toBeGreaterThan(0);
    expect(outside.map((block) => decl(block, "display")).filter(Boolean)).toEqual(["flex"]);
    expect(decl(outside[0], "flex-direction")).toBe("column");
    // The query reads the region's width, not the window's.
    const [region] = blocksFor(css, ".graduation__run");
    expect(decl(region, "container-type")).toBe("inline-size");
  });

  it("GRU-FR-ZPTE: the column divider stands only where the columns stand side by side", () => {
    const css = sheet("components.css");
    const [hidden] = blocksFor(css, ".graduation__columns-divider");
    expect(decl(hidden, "display")).toBe("none");
    const shown = css.match(
      /@container \(min-width: 856px\)\s*\{[^@]*?\.graduation__columns\[data-split="true"\] > \.graduation__columns-divider\s*\{([^}]*)\}/,
    );
    expect(shown, "no rule shows the divider in a wide region").not.toBeNull();
    expect(decl(shown![1], "display")).toBe("block");
    expect(decl(shown![1], "cursor")).toBe("col-resize");
    // A hit area wider than the rule it draws.
    expect(parseInt(decl(shown![1], "width") ?? "0", 10)).toBeGreaterThanOrEqual(7);
  });
});
