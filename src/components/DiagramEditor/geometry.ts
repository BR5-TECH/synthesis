/**
 * EDT-FR-NKGX, EDT-FR-ULPU: the size and the position of the diagram editor
 * overlay, and the split of its two panes.
 *
 * Pure functions of the window size, so each opening starts from the same
 * place and nothing is remembered.
 */

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The window the overlay is drawn in. */
export interface Bounds {
  width: number;
  height: number;
}

/** EDT-FR-NKGX: the part of the window width and height the overlay opens at. */
export const OPEN_WIDTH = 0.9;
export const OPEN_HEIGHT = 0.95;

/** The smallest overlay that keeps both panes usable. */
export const MIN_WIDTH = 420;
export const MIN_HEIGHT = 260;

/** EDT-FR-ULPU: the split each opening starts with, and its limits. */
export const OPEN_SPLIT = 0.5;
export const MIN_SPLIT = 0.15;
export const MAX_SPLIT = 0.85;
/** EDT-FR-HVGG: how far one arrow key moves the divider. */
export const SPLIT_STEP = 0.05;

const clamp = (value: number, low: number, high: number) =>
  Math.min(Math.max(value, low), Math.max(low, high));

/** EDT-FR-NKGX: centred, at 90% of the window width and 95% of its height. */
export function openRect(bounds: Bounds): Rect {
  const width = bounds.width * OPEN_WIDTH;
  const height = bounds.height * OPEN_HEIGHT;
  return {
    left: (bounds.width - width) / 2,
    top: (bounds.height - height) / 2,
    width,
    height,
  };
}

/** EDT-FR-NKGX: the overlay stays inside the window. */
export function clampRect(rect: Rect, bounds: Bounds): Rect {
  const width = clamp(rect.width, Math.min(MIN_WIDTH, bounds.width), bounds.width);
  const height = clamp(rect.height, Math.min(MIN_HEIGHT, bounds.height), bounds.height);
  return {
    left: clamp(rect.left, 0, bounds.width - width),
    top: clamp(rect.top, 0, bounds.height - height),
    width,
    height,
  };
}

/** EDT-FR-NKGX: the overlay moved by the title bar. */
export function moveRect(start: Rect, dx: number, dy: number, bounds: Bounds): Rect {
  return clampRect({ ...start, left: start.left + dx, top: start.top + dy }, bounds);
}

/** An edge or a corner that resizes the overlay. */
export type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export const EDGES: readonly Edge[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

/**
 * EDT-FR-NKGX: the overlay resized from `edge`. The opposite edge stays where
 * it is, and the overlay stays inside the window.
 */
export function resizeRect(
  start: Rect,
  edge: Edge,
  dx: number,
  dy: number,
  bounds: Bounds,
): Rect {
  const minWidth = Math.min(MIN_WIDTH, bounds.width);
  const minHeight = Math.min(MIN_HEIGHT, bounds.height);
  let { left, top } = start;
  let right = start.left + start.width;
  let bottom = start.top + start.height;
  if (edge.includes("e")) right = clamp(right + dx, left + minWidth, bounds.width);
  if (edge.includes("w")) left = clamp(left + dx, 0, right - minWidth);
  if (edge.includes("s")) bottom = clamp(bottom + dy, top + minHeight, bounds.height);
  if (edge.includes("n")) top = clamp(top + dy, 0, bottom - minHeight);
  return { left, top, width: right - left, height: bottom - top };
}

/** EDT-FR-ULPU: the split, kept inside its limits. */
export function clampSplit(split: number): number {
  return clamp(split, MIN_SPLIT, MAX_SPLIT);
}
