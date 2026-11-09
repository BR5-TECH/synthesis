/**
 * CAD-FR-VBDT: the width of a text of the print drawing, and the ellipsis of a
 * text that is too wide for its place.
 *
 * The window measures with the faces of `printPalette.ts`, which have the
 * widths of the faces that the backend draws with. Where the window has no
 * canvas to measure with (a test), an estimate for each character applies.
 */
import { PRINT_MONO, PRINT_SANS } from "./printPalette";

export interface PrintFont {
  size: number;
  mono?: boolean;
  bold?: boolean;
  italic?: boolean;
}

/** The width of `text` in `font`, in CSS pixels. */
export type Measure = (text: string, font: PrintFont) => number;

type Context = { font: string; measureText(text: string): { width: number } };
let context: Context | null | undefined;

function canvasContext(): Context | null {
  if (context !== undefined) return context;
  try {
    context =
      typeof OffscreenCanvas === "undefined"
        ? null
        : (new OffscreenCanvas(1, 1).getContext("2d") as Context | null);
  } catch {
    context = null;
  }
  return context;
}

/** The estimate of a width where no canvas measures. */
export function estimateWidth(text: string, font: PrintFont): number {
  // Liberation Mono advances 0.6 em; the sans faces are near 0.5 em on average.
  const per = font.mono ? 0.6 : font.bold ? 0.56 : 0.5;
  return [...text].length * font.size * per;
}

/** The default measure: the canvas of the window, else the estimate. */
export const measureText: Measure = (text, font) => {
  const ctx = canvasContext();
  if (!ctx) return estimateWidth(text, font);
  ctx.font = `${font.italic ? "italic " : ""}${font.bold ? "bold " : ""}${font.size}px ${
    font.mono ? PRINT_MONO : PRINT_SANS
  }`;
  return ctx.measureText(text).width;
};

const ELLIPSIS = "…";

/** CAD-FR-VBDT: `text`, or its start with an ellipsis, so it fits in `width`. */
export function fitText(text: string, width: number, font: PrintFont, measure: Measure): string {
  if (measure(text, font) <= width) return text;
  const chars = [...text];
  let low = 0;
  let high = chars.length;
  // The longest start that fits with the ellipsis after it.
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (measure(chars.slice(0, mid).join("") + ELLIPSIS, font) <= width) low = mid;
    else high = mid - 1;
  }
  return low === 0 ? ELLIPSIS : chars.slice(0, low).join("").trimEnd() + ELLIPSIS;
}
