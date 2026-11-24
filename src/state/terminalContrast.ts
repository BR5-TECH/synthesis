/**
 * The minimum contrast of drawn terminal text (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-DLJP).
 *
 * Programs such as Claude Code pick 24-bit colours for a dark background. No
 * palette changes them, so on a light theme the terminal moves a colour with
 * too little contrast toward black (toward white on a dark theme) when it
 * draws the cell.
 */
import type { Terminal } from "ghostty-web";
import { logWarn } from "../logging";

export type Rgb = readonly [number, number, number];

/** TRM-FR-DLJP: the lowest contrast ratio of drawn text. */
export const MINIMUM_TEXT_CONTRAST = 4.5;

/** The cell flags of ghostty-web 0.4.0 (`CellFlags`) that keep the colours of a cell. */
export const CELL_INVERSE = 16;
export const CELL_FAINT = 128;

const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];

/** Reads `#rgb`, `#rrggbb`, `rgb(r, g, b)` and `rgba(r, g, b, a)`. Returns null for other text. */
export function parseColor(css: string | null | undefined): Rgb | null {
  if (!css) return null;
  const text = css.trim().toLowerCase();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(text);
  if (short) return [short[1], short[2], short[3]].map((h) => Number.parseInt(h + h, 16)) as unknown as Rgb;
  const long = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/.exec(text);
  if (long) return [long[1], long[2], long[3]].map((h) => Number.parseInt(h, 16)) as unknown as Rgb;
  const fn = /^rgba?\(\s*(\d{1,3})\s*[, ]\s*(\d{1,3})\s*[, ]\s*(\d{1,3})\s*(?:[,/]\s*[\d.]+%?\s*)?\)$/.exec(text);
  if (fn) {
    const channels = [fn[1], fn[2], fn[3]].map(Number);
    if (channels.every((c) => c <= 255)) return channels as unknown as Rgb;
  }
  return null;
}

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** The relative luminance of WCAG 2. */
export function luminance([r, g, b]: Rgb): number {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** The contrast ratio of WCAG 2, from 1 to 21. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function mix(from: Rgb, to: Rgb, amount: number): Rgb {
  return [0, 1, 2].map((i) => Math.round(from[i] + (to[i] - from[i]) * amount)) as unknown as Rgb;
}

/**
 * TRM-FR-DLJP: `fg` where it has `min` on `bg`; else `fg` moved toward black or
 * white, the end with more contrast on `bg`, by the smallest step that gives
 * `min`. Where that end has less than `min`, the end itself.
 */
export function readableColor(fg: Rgb, bg: Rgb, min: number = MINIMUM_TEXT_CONTRAST): Rgb {
  if (contrastRatio(fg, bg) >= min) return fg;
  const target = contrastRatio(BLACK, bg) >= contrastRatio(WHITE, bg) ? BLACK : WHITE;
  if (contrastRatio(target, bg) < min) return target;
  let low = 0;
  let high = 1;
  for (let step = 0; step < 12; step++) {
    const middle = (low + high) / 2;
    if (contrastRatio(mix(fg, target, middle), bg) >= min) high = middle;
    else low = middle;
  }
  // Rounding to whole channels can lose a little contrast; move on until it holds.
  let result = mix(fg, target, high);
  for (let amount = high; contrastRatio(result, bg) < min && amount < 1; ) {
    amount = Math.min(1, amount + 1 / 256);
    result = mix(fg, target, amount);
  }
  return result;
}

/** The fields of a ghostty-web 0.4.0 cell that the contrast reads and writes. */
export interface DrawnCell {
  fg_r: number;
  fg_g: number;
  fg_b: number;
  bg_r: number;
  bg_g: number;
  bg_b: number;
  flags: number;
}

type DrawText = (cell: DrawnCell, col: number, row: number) => void;

/** The private parts of the ghostty-web 0.4.0 renderer that the contrast uses. */
interface RendererInternals {
  theme?: { background?: string };
  renderCellText?: DrawText;
}

/** The most colour pairs that one terminal keeps; above it the cache starts again. */
const CACHE_LIMIT = 4096;

/**
 * TRM-FR-DLJP: the colour that a cell's text gets, from the cell and the
 * terminal's background. Null where the cell keeps its own colour.
 */
export function newCellContrast(min: number = MINIMUM_TEXT_CONTRAST) {
  const cache = new Map<number, Rgb>();
  let backgroundText: string | undefined;
  let background: Rgb | null = null;
  return (cell: DrawnCell, themeBackground: string | undefined): Rgb | null => {
    if (cell.flags & (CELL_INVERSE | CELL_FAINT)) return null;
    let bg: Rgb;
    // ghostty-web 0.4.0 reads a background of 0,0,0 as the default background.
    if (cell.bg_r === 0 && cell.bg_g === 0 && cell.bg_b === 0) {
      if (themeBackground !== backgroundText) {
        backgroundText = themeBackground;
        background = parseColor(themeBackground);
      }
      if (!background) return null;
      bg = background;
    } else {
      bg = [cell.bg_r, cell.bg_g, cell.bg_b];
    }
    // Two 24-bit colours in one safe integer, so the hot path makes no string.
    const key = ((cell.fg_r << 16) | (cell.fg_g << 8) | cell.fg_b) * 0x1000000 + ((bg[0] << 16) | (bg[1] << 8) | bg[2]);
    let color = cache.get(key);
    if (!color) {
      if (cache.size >= CACHE_LIMIT) cache.clear();
      color = readableColor([cell.fg_r, cell.fg_g, cell.fg_b], bg, min);
      cache.set(key, color);
    }
    if (color[0] === cell.fg_r && color[1] === cell.fg_g && color[2] === cell.fg_b) return null;
    return color;
  };
}

/**
 * TRM-FR-DLJP: makes the renderer of `term` draw each cell's text with the
 * minimum contrast. Call it after `open()`, which makes the renderer. Where
 * the library has no such renderer, it changes nothing and, where `warn` is
 * true, logs a `WARN` record (TRM-FR-LNOZ). Returns whether it applied.
 */
export function enforceMinimumContrast(term: Pick<Terminal, "renderer">, terminalId: string, warn = true): boolean {
  const renderer = term.renderer as unknown as RendererInternals | undefined;
  const original = renderer?.renderCellText;
  if (!renderer || typeof original !== "function") {
    if (warn) logWarn(["frontend"], "terminal contrast not applied", { terminalId, reason: "no_renderer_hook" });
    return false;
  }
  const contrast = newCellContrast();
  // One copy of the cell for each terminal: the renderer draws one cell at a time.
  const scratch: DrawnCell = { fg_r: 0, fg_g: 0, fg_b: 0, bg_r: 0, bg_g: 0, bg_b: 0, flags: 0 };
  renderer.renderCellText = function drawReadable(this: RendererInternals, cell, col, row) {
    const color = contrast(cell, this.theme?.background);
    if (!color) return original.call(this, cell, col, row);
    Object.assign(scratch, cell);
    scratch.fg_r = color[0];
    scratch.fg_g = color[1];
    scratch.fg_b = color[2];
    return original.call(this, scratch, col, row);
  };
  return true;
}
