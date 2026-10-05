import { describe, expect, it } from "vitest";

import {
  clampPathsFraction,
  DEFAULT_PATHS_FRACTION,
  MAX_PATHS_FRACTION,
  MIN_PATHS_FRACTION,
  pathsFractionForDrag,
  pathsFractionForKey,
  pathsPercent,
} from "./pathsWidth";

describe("the paths column's width", () => {
  it("GRU-FR-KWRB: 0.30 where the author has set none, held between 0.15 and 0.70", () => {
    expect(DEFAULT_PATHS_FRACTION).toBe(0.3);
    expect(MIN_PATHS_FRACTION).toBe(0.15);
    expect(MAX_PATHS_FRACTION).toBe(0.7);
    expect(clampPathsFraction(undefined)).toBe(0.3);
    expect(clampPathsFraction(null)).toBe(0.3);
    expect(clampPathsFraction(Number.NaN)).toBe(0.3);
    expect(clampPathsFraction(0.05)).toBe(0.15);
    expect(clampPathsFraction(0.95)).toBe(0.7);
    expect(clampPathsFraction(0.42)).toBe(0.42);
  });

  it("GRU-FR-ZPTE: the divider announces a whole percentage", () => {
    expect(pathsPercent(0.304)).toBe(30);
    expect(pathsPercent(0.9)).toBe(70);
  });

  it("GRU-FR-ZPTE: a drag sets the share of the columns the pointer stands at, inside the bounds", () => {
    expect(pathsFractionForDrag(400, 1000)).toBe(0.4);
    expect(pathsFractionForDrag(10, 1000)).toBe(0.15);
    expect(pathsFractionForDrag(990, 1000)).toBe(0.7);
    expect(pathsFractionForDrag(100, 0)).toBe(0.3);
  });

  it("GRU-FR-ZPTE: Left and Right move one point, Home and End go to the bounds", () => {
    expect(pathsFractionForKey("ArrowRight", 0.3)).toBeCloseTo(0.31, 5);
    expect(pathsFractionForKey("ArrowLeft", 0.3)).toBeCloseTo(0.29, 5);
    // A dragged width steps from the percentage it announces.
    expect(pathsFractionForKey("ArrowRight", 0.304)).toBeCloseTo(0.31, 5);
    expect(pathsFractionForKey("ArrowLeft", 0.15)).toBe(0.15);
    expect(pathsFractionForKey("ArrowRight", 0.7)).toBe(0.7);
    expect(pathsFractionForKey("Home", 0.4)).toBe(0.15);
    expect(pathsFractionForKey("End", 0.4)).toBe(0.7);
    expect(pathsFractionForKey("ArrowUp", 0.4)).toBeNull();
  });

  it("GRU-FR-KWRB: the bounds hold exactly, and a zero is a bound rather than unset", () => {
    expect(clampPathsFraction(0)).toBe(0.15);
    expect(clampPathsFraction(0.15)).toBe(0.15);
    expect(clampPathsFraction(0.7)).toBe(0.7);
    expect(clampPathsFraction(Number.POSITIVE_INFINITY)).toBe(0.3);
    expect(clampPathsFraction(Number.NEGATIVE_INFINITY)).toBe(0.3);
  });

  it("GRU-FR-ZPTE: a pointer outside the columns holds the width at a bound", () => {
    expect(pathsFractionForDrag(-50, 1000)).toBe(0.15);
    expect(pathsFractionForDrag(1200, 1000)).toBe(0.7);
    expect(pathsFractionForDrag(150, 1000)).toBe(0.15);
    expect(pathsFractionForDrag(700, 1000)).toBe(0.7);
    expect(pathsFractionForDrag(100, Number.NaN)).toBe(0.3);
    expect(pathsFractionForDrag(100, Number.POSITIVE_INFINITY)).toBe(0.3);
  });

  it("GRU-FR-ZPTE: a step starts from the width inside the bounds", () => {
    expect(pathsFractionForKey("ArrowLeft", 0.95)).toBeCloseTo(0.69, 5);
    expect(pathsFractionForKey("ArrowRight", 0.05)).toBeCloseTo(0.16, 5);
  });
});
