import { describe, expect, it } from "vitest";

import {
  MAX_SPLIT,
  MIN_HEIGHT,
  MIN_SPLIT,
  MIN_WIDTH,
  clampRect,
  clampSplit,
  moveRect,
  openRect,
  resizeRect,
} from "./geometry";

const WINDOW = { width: 1000, height: 800 };

describe("the geometry of the diagram editor overlay", () => {
  it("EDT-FR-NKGX: it opens centred at 90% of the window width and 95% of its height", () => {
    expect(openRect(WINDOW)).toEqual({ left: 50, top: 20, width: 900, height: 760 });
  });

  it("EDT-FR-NKGX: a move keeps the overlay inside the window", () => {
    const start = openRect(WINDOW);
    expect(moveRect(start, -30, 10, WINDOW)).toEqual({ ...start, left: 20, top: 30 });
    expect(moveRect(start, -500, -500, WINDOW)).toEqual({ ...start, left: 0, top: 0 });
    expect(moveRect(start, 500, 500, WINDOW)).toEqual({ ...start, left: 100, top: 40 });
  });

  it("EDT-FR-NKGX: a resize from an edge or a corner keeps the opposite edge and the window bounds", () => {
    const start = openRect(WINDOW);
    expect(resizeRect(start, "e", -100, 0, WINDOW)).toEqual({ ...start, width: 800 });
    expect(resizeRect(start, "w", 30, 0, WINDOW)).toEqual({ ...start, left: 80, width: 870 });
    expect(resizeRect(start, "se", 200, 200, WINDOW)).toEqual({
      ...start,
      width: 950,
      height: 780,
    });
    const nw = resizeRect(start, "nw", 2000, 2000, WINDOW);
    expect(nw.width).toBe(MIN_WIDTH);
    expect(nw.height).toBe(MIN_HEIGHT);
    expect(nw.left + nw.width).toBe(start.left + start.width);
    expect(nw.top + nw.height).toBe(start.top + start.height);
  });

  it("EDT-FR-NKGX: a window that shrinks pulls the overlay back inside it", () => {
    const start = openRect(WINDOW);
    expect(clampRect(start, { width: 600, height: 500 })).toEqual({
      left: 0,
      top: 0,
      width: 600,
      height: 500,
    });
  });

  it("EDT-FR-ULPU: the split stays between its limits", () => {
    expect(clampSplit(0)).toBe(MIN_SPLIT);
    expect(clampSplit(1)).toBe(MAX_SPLIT);
    expect(clampSplit(0.4)).toBe(0.4);
  });
});
