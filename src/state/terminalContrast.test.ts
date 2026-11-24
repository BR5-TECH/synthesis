// The minimum contrast of drawn terminal text (`TRM-terminals.md` TRM-FR-DLJP, TRM-FR-LNOZ).
import { describe, expect, it, vi } from "vitest";
import { TERMINAL_THEMES } from "./terminalThemes";
import {
  CELL_FAINT,
  CELL_INVERSE,
  contrastRatio,
  enforceMinimumContrast,
  newCellContrast,
  parseColor,
  readableColor,
  type DrawnCell,
  type Rgb,
} from "./terminalContrast";
import { logWarn } from "../logging";

vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

/** The link colour that Claude Code picks for a dark background. */
const LIGHT_BLUE: Rgb = [177, 185, 249];

const rgb = (css: string) => parseColor(css)!;

function cell(fg: Rgb, bg: Rgb = [0, 0, 0], flags = 0): DrawnCell {
  return { fg_r: fg[0], fg_g: fg[1], fg_b: fg[2], bg_r: bg[0], bg_g: bg[1], bg_b: bg[2], flags };
}

describe("parseColor", () => {
  it("TRM-FR-DLJP: reads the colour forms of the themes and of the CSS variables", () => {
    expect(parseColor("#fbfaf7")).toEqual([251, 250, 247]);
    expect(parseColor("#FFF")).toEqual([255, 255, 255]);
    expect(parseColor("  #0a0b0c ")).toEqual([10, 11, 12]);
    expect(parseColor("rgb(16, 20, 28)")).toEqual([16, 20, 28]);
    expect(parseColor("rgb(16 20 28)")).toEqual([16, 20, 28]);
    expect(parseColor("rgba(1, 2, 3, 0.5)")).toEqual([1, 2, 3]);
    expect(parseColor("rgba(1 2 3 / 50%)")).toEqual([1, 2, 3]);
    expect(parseColor("#AABBCC")).toEqual([170, 187, 204]);
  });

  it("TRM-FR-DLJP: gives null for text that is not one of those forms", () => {
    for (const bad of [undefined, null, "", "red", "#12345", "#gggggg", "rgb(300, 0, 0)", "rgb(-1, 0, 0)", "#abcd", "#aabbccdd", "hsl(0, 0%, 0%)"])
      expect(parseColor(bad), String(bad)).toBeNull();
  });
});

describe("readableColor", () => {
  it("TRM-FR-DLJP: a light blue gets 4.5:1 on each light theme, stays blue, and moves by the smallest step", () => {
    for (const theme of TERMINAL_THEMES.filter((t) => !t.dark)) {
      const bg = rgb(theme.palette.background);
      const out = readableColor(LIGHT_BLUE, bg);
      expect(contrastRatio(out, bg), theme.id).toBeGreaterThanOrEqual(4.5);
      // It moved toward black: each channel is darker, and blue stays the largest.
      expect(out.every((c, i) => c <= LIGHT_BLUE[i]), theme.id).toBe(true);
      expect(out[2], theme.id).toBeGreaterThan(out[0]);
      // The smallest step: one step less toward black does not reach 4.5:1.
      const lighter = out.map((c, i) => Math.min(LIGHT_BLUE[i], c + 2)) as unknown as Rgb;
      expect(contrastRatio(lighter, bg), theme.id).toBeLessThan(4.5);
    }
  });

  it("TRM-FR-DLJP: a dark colour gets 4.5:1 on each dark theme by a move toward white", () => {
    for (const theme of TERMINAL_THEMES.filter((t) => t.dark)) {
      const bg = rgb(theme.palette.background);
      const darkBlue: Rgb = [20, 40, 120];
      const out = readableColor(darkBlue, bg);
      expect(contrastRatio(out, bg), theme.id).toBeGreaterThanOrEqual(4.5);
      expect(out.every((c, i) => c >= darkBlue[i]), theme.id).toBe(true);
      // The smallest step: one step less toward white does not reach 4.5:1.
      const darker = out.map((c, i) => Math.max(darkBlue[i], c - 2)) as unknown as Rgb;
      expect(contrastRatio(darker, bg), theme.id).toBeLessThan(4.5);
    }
  });

  it("TRM-FR-DLJP: a colour that has 4.5:1 stays as it is, also the theme's own colours", () => {
    const paper = rgb("#fbfaf7");
    const blue = rgb("#2156b8");
    expect(readableColor(blue, paper)).toBe(blue);
    for (const theme of TERMINAL_THEMES) {
      const fg = rgb(theme.palette.foreground);
      expect(readableColor(fg, rgb(theme.palette.background))).toBe(fg);
    }
  });

  it("TRM-FR-DLJP: where no colour reaches the minimum, it gives the end with the most contrast", () => {
    const grey: Rgb = [119, 119, 119];
    // Black has more contrast on this grey than white has.
    expect(contrastRatio([0, 0, 0], grey)).toBeGreaterThan(contrastRatio([255, 255, 255], grey));
    expect(readableColor([120, 120, 120], grey, 21)).toEqual([0, 0, 0]);
  });
});

describe("contrastRatio", () => {
  it("TRM-FR-DLJP: black on white is 21, a colour on itself is 1, and the order does not matter", () => {
    expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrastRatio([90, 120, 30], [90, 120, 30])).toBe(1);
    expect(contrastRatio([10, 20, 30], [200, 210, 220])).toBe(contrastRatio([200, 210, 220], [10, 20, 30]));
  });

  it("TRM-FR-DLJP: the result of readableColor has the minimum over a grid of colours, after rounding", () => {
    const steps = [0, 51, 102, 153, 204, 255];
    const backgrounds: Rgb[] = [[251, 250, 247], [245, 238, 223], [237, 241, 244], [16, 20, 28], [27, 21, 18], [128, 128, 128]];
    for (const bg of backgrounds)
      for (const r of steps)
        for (const g of steps)
          for (const b of steps) {
            const out = readableColor([r, g, b], bg);
            const best = Math.max(contrastRatio([0, 0, 0], bg), contrastRatio([255, 255, 255], bg));
            expect(contrastRatio(out, bg), `${r},${g},${b} on ${bg}`).toBeGreaterThanOrEqual(Math.min(4.5, best));
          }
  });
});

describe("the contrast of one cell", () => {
  it("TRM-FR-DLJP: the cache gives the right colours also after it starts again", () => {
    const contrast = newCellContrast();
    const first = contrast(cell(LIGHT_BLUE), "#fbfaf7");
    for (let i = 0; i < 5000; i++) contrast(cell([200, i % 256, (i >> 8) % 256]), "#fbfaf7");
    expect(contrast(cell(LIGHT_BLUE), "#fbfaf7")).toEqual(first);
  });

  const paper = "#fbfaf7";

  it("TRM-FR-DLJP: a cell on the default background is measured on the theme background", () => {
    const contrast = newCellContrast();
    const out = contrast(cell(LIGHT_BLUE), paper)!;
    expect(out).not.toBeNull();
    expect(contrastRatio(out, rgb(paper))).toBeGreaterThanOrEqual(4.5);
  });

  it("TRM-FR-DLJP: a cell with its own background is measured on that background", () => {
    const contrast = newCellContrast();
    // Light blue on a dark cell background is readable, also on a light theme.
    expect(contrast(cell(LIGHT_BLUE, [20, 20, 40]), paper)).toBeNull();
    // Dark text on a dark cell background moves toward white.
    const out = contrast(cell([30, 30, 60], [20, 20, 40]), paper)!;
    expect(contrastRatio(out, [20, 20, 40])).toBeGreaterThanOrEqual(4.5);
  });

  it("TRM-FR-DLJP: inverse and faint text keep their colours", () => {
    const contrast = newCellContrast();
    expect(contrast(cell(LIGHT_BLUE, [0, 0, 0], CELL_INVERSE), paper)).toBeNull();
    expect(contrast(cell(LIGHT_BLUE, [0, 0, 0], CELL_FAINT), paper)).toBeNull();
    expect(contrast(cell(LIGHT_BLUE, [0, 0, 0], CELL_INVERSE | CELL_FAINT), paper)).toBeNull();
    expect(contrast(cell(LIGHT_BLUE, [0, 0, 0], 1 /* bold */), paper)).not.toBeNull();
  });

  it("TRM-FR-DLJP: a theme background that cannot be read changes nothing", () => {
    expect(newCellContrast()(cell(LIGHT_BLUE), "var(--x)")).toBeNull();
    expect(newCellContrast()(cell(LIGHT_BLUE), undefined)).toBeNull();
  });

  it("TRM-FR-DLJP: the answer follows a change of the theme background", () => {
    const contrast = newCellContrast();
    expect(contrast(cell(LIGHT_BLUE), paper)).not.toBeNull();
    expect(contrast(cell(LIGHT_BLUE), "#10141c")).toBeNull();
  });
});

describe("enforceMinimumContrast", () => {
  function renderer(background: string) {
    const drawn: { cell: DrawnCell; col: number; row: number; self: unknown }[] = [];
    const r = {
      theme: { background },
      renderCellText(this: unknown, c: DrawnCell, col: number, row: number) {
        drawn.push({ cell: { ...c }, col, row, self: this });
      },
    };
    return { r, drawn };
  }

  it("TRM-FR-DLJP: the renderer draws a light blue darker on a light theme and keeps other fields", () => {
    const { r, drawn } = renderer("#fbfaf7");
    expect(enforceMinimumContrast({ renderer: r } as never, "t1")).toBe(true);
    const input = { ...cell(LIGHT_BLUE), codepoint: 65, hyperlink_id: 3 };
    r.renderCellText(input, 4, 2);
    expect(drawn[0].col).toBe(4);
    expect(drawn[0].row).toBe(2);
    expect(drawn[0].self).toBe(r);
    const out = drawn[0].cell as DrawnCell & { codepoint: number; hyperlink_id: number };
    expect(out.codepoint).toBe(65);
    expect(out.hyperlink_id).toBe(3);
    expect(contrastRatio([out.fg_r, out.fg_g, out.fg_b], rgb("#fbfaf7"))).toBeGreaterThanOrEqual(4.5);
    // The cell of the library is not changed.
    expect(input.fg_r).toBe(LIGHT_BLUE[0]);
  });

  it("TRM-FR-DLJP: a readable cell goes to the renderer as it is", () => {
    const { r, drawn } = renderer("#fbfaf7");
    enforceMinimumContrast({ renderer: r } as never, "t1");
    const input = cell([33, 86, 184]);
    r.renderCellText(input, 0, 0);
    expect(drawn[0].cell).toEqual(input);
  });

  it("TRM-FR-DLJP: a change of the renderer's theme background applies to the next cell", () => {
    const { r, drawn } = renderer("#fbfaf7");
    enforceMinimumContrast({ renderer: r } as never, "t1");
    r.theme.background = "#10141c";
    r.renderCellText(cell(LIGHT_BLUE), 0, 0);
    expect(drawn[0].cell.fg_r).toBe(LIGHT_BLUE[0]);
  });

  it("TRM-FR-LNOZ: without the hook and with warn off, it logs nothing", () => {
    vi.mocked(logWarn).mockClear();
    expect(enforceMinimumContrast({ renderer: undefined } as never, "t1", false)).toBe(false);
    expect(logWarn).not.toHaveBeenCalled();
  });

  it("TRM-FR-DLJP, TRM-FR-LNOZ: without the renderer hook it logs one WARN and changes nothing", () => {
    vi.mocked(logWarn).mockClear();
    expect(enforceMinimumContrast({ renderer: undefined } as never, "t1")).toBe(false);
    expect(enforceMinimumContrast({ renderer: { theme: {} } } as never, "t2")).toBe(false);
    expect(logWarn).toHaveBeenCalledTimes(2);
    expect(vi.mocked(logWarn).mock.calls[0]).toEqual([
      ["frontend"],
      "terminal contrast not applied",
      { terminalId: "t1", reason: "no_renderer_hook" },
    ]);
  });
});

describe("the ghostty-web 0.4.0 renderer that the contrast reaches into", () => {
  it("TRM-FR-DLJP: CanvasRenderer has renderCellText, which reads the cell's colours, and the flags have the values used", async () => {
    const real = await vi.importActual<typeof import("ghostty-web")>("ghostty-web");
    const draw = (real.CanvasRenderer.prototype as unknown as Record<string, unknown>).renderCellText;
    expect(typeof draw).toBe("function");
    const source = String(draw);
    for (const field of ["fg_r", "fg_g", "fg_b", "flags"]) expect(source).toContain(field);
    // Selected text gets the selection colour, whatever the cell's colour is.
    expect(source).toContain("isInSelection");
    expect(source).toContain("selectionForeground");
    const proto = real.CanvasRenderer.prototype as unknown as Record<string, unknown>;
    // Each line calls the method on the instance, so the wrapper of one terminal applies.
    expect(String(proto.renderLine)).toContain("this.renderCellText(");
    // A cell background of 0,0,0 is not painted, so the theme background shows.
    expect(String(proto.renderCellBackground)).toMatch(/=== 0 && \w+ === 0 && \w+ === 0/);
    expect(real.CellFlags.INVERSE).toBe(CELL_INVERSE);
    expect(real.CellFlags.FAINT).toBe(CELL_FAINT);
  }, 30_000);
});
