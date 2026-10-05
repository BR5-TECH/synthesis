import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../../test/cssRules";

/**
 * The Map tab's visual requirements that live in the stylesheet alone. Vitest
 * applies no CSS, so a component test cannot see any of these.
 */
const css = sheet("components.css");

function only(selector: string): string {
  const blocks = blocksFor(css, selector);
  expect(blocks, `expected one rule for ${selector}`).toHaveLength(1);
  return blocks[0];
}

describe("the tab frame (SMP)", () => {
  it("SMP-FR-CVIB: the toolbar is 38px high and scrolls sideways rather than wrapping", () => {
    const toolbar = only(".smap-toolbar");
    expect(decl(toolbar, "height")).toBe("38px");
    expect(decl(toolbar, "overflow-x")).toBe("auto");
    expect(decl(toolbar, "white-space")).toBe("nowrap");
  });

  it("SMP-FR-YDKM, SMP-FR-GJEW: the project bar is 170 × 8px, and 92px wide when narrow", () => {
    const bar = only('.smap-bar[data-variant="project"]');
    expect([decl(bar, "width"), decl(bar, "height")]).toEqual(["170px", "8px"]);
    expect(decl(only('.smap-bar[data-variant="project"][data-narrow]'), "width")).toBe("92px");
  });
});

describe("the canvas (SMZ)", () => {
  it("SMZ-FR-DPWC: a 16px dot grid of --border-2 on --bg-canvas, and one stage transformed from its origin", () => {
    const canvas = only(".smap-canvas");
    expect(decl(canvas, "background-color")).toBe("var(--bg-canvas)");
    expect(decl(canvas, "background-image")).toContain("var(--border-2) 1px");
    expect(decl(canvas, "background-size")).toBe("16px 16px");
    expect(decl(only(".smap-stage"), "transform-origin")).toBe("0 0");
  });

  it("SMZ-FR-AILK: the legend is a 26px pill", () => {
    expect(decl(only(".smap-legend"), "height")).toBe("26px");
  });
});

describe("nodes (SMN)", () => {
  it("SMN-FR-CMLN: bars and dots use the four state tokens", () => {
    const tokens = { verified: "--ok", built: "--accent", drafted: "--fg-4", gap: "--danger" };
    for (const [state, token] of Object.entries(tokens)) {
      expect(decl(only(`.smap-bar__segment[data-state="${state}"]`), "background")).toBe(`var(${token})`);
      expect(decl(only(`.smap-dot[data-state="${state}"]`), "background")).toBe(`var(${token})`);
      expect(decl(only(`.smap-count[data-state="${state}"]`), "color")).toBe(`var(${token})`);
    }
  });

  it("SMN-FR-WAGD: each of the six folders has a badge colour pair in both themes", () => {
    for (const folder of ["ui", "core", "ai", "tools", "infra", "server"]) {
      for (const selector of [
        `.smap-code[data-folder="${folder}"]`,
        `[data-theme="dark"] .smap-code[data-folder="${folder}"]`,
      ]) {
        const block = only(selector);
        expect(decl(block, "background"), selector).toMatch(/^#[0-9A-F]{6}$/i);
        expect(decl(block, "color"), selector).toMatch(/^#[0-9A-F]{6}$/i);
      }
    }
  });

  it("SMN-FR-EYRV: a selected root card has an accent border and a 3px ring", () => {
    const root = only(".smap-root[data-selected]");
    expect(decl(root, "border-color")).toBe("var(--accent)");
    expect(decl(root, "box-shadow")).toBe("0 0 0 3px var(--ring)");
    expect(decl(only(".smap-box[data-selected]"), "border-color")).toBe("var(--accent)");
    expect(decl(only(".smap-chip[data-selected]"), "background")).toBe("var(--accent-soft)");
  });

  it("SMN-FR-KTZB: a root or branch that contains the active node has a --border-3 border", () => {
    const blocks = blocksFor(css, ".smap-root[data-context]");
    expect(blocks).toHaveLength(1);
    expect(decl(blocks[0], "border-color")).toBe("var(--border-3)");
    expect(blocksFor(css, ".smap-box[data-context]")).toEqual(blocks);
  });

  it("SMN-FR-HLDQ, SME-FR-WRPX, SMO-FR-DKTM: each kind of dimming has its own opacity", () => {
    expect(decl(only('.smap-node[data-dim="gaps"]'), "opacity")).toBe("0.3");
    expect(decl(only('.smap-chip[data-dim="gaps"]'), "opacity")).toBe("0.25");
    expect(decl(only('.smap-node[data-dim="related"]'), "opacity")).toBe("0.5");
    expect(decl(only('.smap-node[data-dim="drag"]'), "opacity")).toBe("0.4");
  });

  it("SMN-FR-MVWA, SMN-FR-YSKQ: the hover card is 280px wide and ignores the pointer", () => {
    const card = only(".smap-hover");
    expect(decl(card, "width")).toBe("280px");
    expect(decl(card, "pointer-events")).toBe("none");
  });
});

describe("edges (SME)", () => {
  it("SME-FR-SGUC: light, medium and heavy draw at 1.4, 2.2 and 3.2 at opacity 0.7", () => {
    const light = only('.smap-edge[data-weight="light"]');
    const medium = only('.smap-edge[data-weight="medium"]');
    const heavy = only('.smap-edge[data-weight="heavy"]');
    expect([decl(light, "stroke"), decl(light, "stroke-width"), decl(light, "opacity")]).toEqual([
      "var(--border-2)",
      "1.4",
      "0.7",
    ]);
    expect(decl(medium, "stroke")).toContain("var(--border-2)");
    expect(decl(medium, "stroke")).toContain("var(--border-3)");
    expect(decl(medium, "stroke-width")).toBe("2.2");
    expect([decl(heavy, "stroke"), decl(heavy, "stroke-width")]).toEqual(["var(--border-3)", "3.2"]);
  });

  it("SME-FR-TJMY: an active edge draws 2.4px in the accent at 0.95", () => {
    const active = only('.smap-edge[data-weight="active"]');
    expect([decl(active, "stroke"), decl(active, "stroke-width"), decl(active, "opacity")]).toEqual([
      "var(--accent)",
      "2.4",
      "0.95",
    ]);
    expect(decl(only('.smap-marker[data-marker="hot"]'), "fill")).toBe("var(--accent)");
  });

  it("SME-FR-VAEC: an unresolved citation is a 1.6px 6/5 dashed line in the danger colour", () => {
    const bad = only('.smap-edge[data-weight="unresolved"]');
    expect([decl(bad, "stroke"), decl(bad, "stroke-width"), decl(bad, "stroke-dasharray")]).toEqual([
      "var(--danger)",
      "1.6",
      "6 5",
    ]);
    expect(decl(only('.smap-marker[data-marker="bad"]'), "fill")).toBe("var(--danger)");
  });
});

describe("the inspector (SMI)", () => {
  it("SMI-FR-QWOP: docked it is 318px wide; as an overlay it is 300px, over the canvas, with shadow 4", () => {
    const docked = only(".smap-inspector");
    expect(decl(docked, "width")).toBe("318px");
    expect(decl(docked, "background")).toBe("var(--bg-panel)");
    const overlay = only(".smap-inspector[data-overlay]");
    expect(decl(overlay, "position")).toBe("absolute");
    expect(decl(overlay, "width")).toBe("300px");
    expect(decl(overlay, "box-shadow")).toBe("var(--shadow-4)");
  });

  it("SMI-FR-TRUZ: the header is 28px high", () => {
    expect(decl(only(".smap-inspector__head"), "height")).toBe("28px");
  });
});

describe("planned chips (SMD)", () => {
  it("SMD-FR-CGNW: a planned chip has a dashed border and a live-coloured draft badge", () => {
    expect(decl(only(".smap-chip--planned"), "border-style")).toBe("dashed");
    const badge = only(".smap-code--draft");
    expect([decl(badge, "background"), decl(badge, "color")]).toEqual(["var(--live-soft)", "var(--live)"]);
  });
});

describe("state styles of controls", () => {
  it("SMZ-FR-VYOS: the current level uses the soft accent, the others are transparent in --fg-3", () => {
    const option = only(".smap-levels__option");
    expect([decl(option, "background"), decl(option, "color")]).toEqual(["transparent", "var(--fg-3)"]);
    const active = only(".smap-levels__option[data-active]");
    expect([decl(active, "background"), decl(active, "color")]).toEqual(["var(--accent-soft)", "var(--accent)"]);
  });

  it("SMZ-FR-KAHU: the canvas shows the grab cursor", () => {
    expect(decl(only(".smap-canvas"), "cursor")).toBe("grab");
  });

  it("SME-FR-OKUH, SMN-FR-HLDQ: each toggle uses its on tokens when pressed and the sunken tokens when not", () => {
    const off = only(".smap-filter");
    expect([decl(off, "background"), decl(off, "color")]).toEqual(["var(--bg-sunken)", "var(--fg-3)"]);
    const deps = only('.smap-filter[data-filter="deps"][aria-pressed="true"]');
    expect([decl(deps, "background"), decl(deps, "color")]).toEqual(["var(--bg-active)", "var(--accent)"]);
    const gaps = only('.smap-filter[data-filter="gaps"][aria-pressed="true"]');
    expect([decl(gaps, "background"), decl(gaps, "color")]).toEqual(["var(--danger-soft)", "var(--danger)"]);
  });

  it("SMD-FR-CGNW: a planned chip's border is a 1px dashed --border-2 line", () => {
    expect(decl(only(".smap-chip"), "border")).toBe("1px solid var(--border-1)");
    const planned = only(".smap-chip--planned");
    expect([decl(planned, "border-style"), decl(planned, "border-color")]).toEqual(["dashed", "var(--border-2)"]);
  });

  it("SMI-FR-QWOP: the docked inspector has a 1px --border-1 left border", () => {
    expect(decl(only(".smap-inspector"), "border-left")).toBe("1px solid var(--border-1)");
  });

  it("SMI-FR-ZLOA, SMI-FR-YHNT: the attention card is --danger-soft, and an unresolved row reads in --danger", () => {
    expect(decl(only(".smap-attention"), "background")).toBe("var(--danger-soft)");
    expect(decl(only(".smap-row[data-danger] .smap-row__label"), "color")).toBe("var(--danger)");
  });

  it("SMI-FR-QWOP: an overlay inspector moves the zoom controls clear of its 300px", () => {
    expect(decl(only(".smap-canvas[data-inspector-overlay] .smap-zoom"), "right")).toBe("312px");
    expect(decl(only(".smap-inspector"), "box-sizing")).toBe("border-box");
  });

  it("SMI-FR-RPCO: a secondary action sits on --bg-canvas with a 1px --border-2 line", () => {
    const action = only(".smap-action");
    expect([decl(action, "background"), decl(action, "border")]).toEqual([
      "var(--bg-canvas)",
      "1px solid var(--border-2)",
    ]);
  });
});
