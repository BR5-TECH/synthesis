import { describe, expect, it } from "vitest";

import {
  FIT_MARGIN,
  FIT_THRESHOLD,
  MAX_ZOOM,
  MIN_ZOOM,
  ZOOM_STEP,
  fitTransform,
  naturalSize,
  panBy,
  wheelFactor,
  wheelPan,
  zoomAt,
} from "./viewerGeometry";

describe("the zoom and the pan of the diagram viewer", () => {
  it("EDT-FR-LJAP: a fit keeps the aspect ratio, shows all of the diagram, and centres it", () => {
    const t = fitTransform({ width: 800, height: 600 }, { width: 400, height: 100 });
    const scale = (800 - 2 * FIT_MARGIN) / 400;
    expect(t.scale).toBeCloseTo(scale);
    expect(t.x).toBeCloseTo((800 - 400 * scale) / 2);
    expect(t.y).toBeCloseTo((600 - 100 * scale) / 2);
    expect(400 * t.scale).toBeLessThanOrEqual(800);
    expect(100 * t.scale).toBeLessThanOrEqual(600);
  });

  it("EDT-FR-LJAP, EDT-FR-MZIF: a fit stays inside the zoom limits, and an area with no size fits nothing", () => {
    expect(fitTransform({ width: 800, height: 600 }, { width: 1, height: 1 }).scale).toBe(MAX_ZOOM);
    // Too large to fit in either direction: the zoom limit holds.
    expect(fitTransform({ width: 100, height: 100 }, { width: 100000, height: 50000 })).toEqual({
      scale: MIN_ZOOM,
      x: FIT_MARGIN,
      y: FIT_MARGIN,
    });
    expect(fitTransform({ width: 0, height: 0 }, { width: 10, height: 10 })).toEqual({
      scale: 1,
      x: 0,
      y: 0,
    });
  });

  it("EDT-FR-LJAP: a fit of all of the diagram at the threshold stays, and below it the fit is in one direction", () => {
    // Area 800 x 600, room 768 x 568. A diagram 960 wide fits whole at 0.8.
    expect(768 / 960).toBe(FIT_THRESHOLD);
    const at = fitTransform({ width: 800, height: 600 }, { width: 960, height: 100 });
    expect(at.scale).toBe(FIT_THRESHOLD);
    expect(at.x).toBeCloseTo(FIT_MARGIN);
    expect(at.y).toBeCloseTo((600 - 80) / 2);
    // A little wider: a whole fit is below 0.8, so the height fills the area,
    // capped at 100%, and the diagram starts at the left margin.
    const below = fitTransform({ width: 800, height: 600 }, { width: 768 / 0.79, height: 100 });
    expect(below.scale).toBe(1);
    expect(below.x).toBe(FIT_MARGIN);
    expect(below.y).toBeCloseTo((600 - 100) / 2);
  });

  it("EDT-FR-LJAP: a tall diagram fills the width, starts at the top, and is centred across", () => {
    // Whole fit min(768 / 400, 568 / 3000) = 0.19; across 768 / 400 = 1.92, capped at 1.
    const capped = fitTransform({ width: 800, height: 600 }, { width: 400, height: 3000 });
    expect(capped).toEqual({ scale: 1, x: (800 - 400) / 2, y: FIT_MARGIN });
    // A tall diagram wider than the area: the width fills the room.
    const filled = fitTransform({ width: 800, height: 600 }, { width: 1536, height: 6000 });
    expect(filled.scale).toBeCloseTo(0.5);
    expect(filled.x).toBeCloseTo(FIT_MARGIN);
    expect(filled.y).toBe(FIT_MARGIN);
    expect(6000 * filled.scale).toBeGreaterThan(600);
  });

  it("EDT-FR-LJAP: a wide diagram fills the height, starts at the left, and is centred from top to bottom", () => {
    // Whole fit 768 / 3000 = 0.256 and 568 / 2000 = 0.284: the height fills the room.
    const t = fitTransform({ width: 800, height: 600 }, { width: 3000, height: 2000 });
    expect(t.scale).toBeCloseTo(568 / 2000);
    expect(t.x).toBe(FIT_MARGIN);
    expect(t.y).toBeCloseTo(FIT_MARGIN);
    expect(3000 * t.scale).toBeGreaterThan(800);
    // A thin wide diagram shows at its natural size.
    expect(fitTransform({ width: 800, height: 600 }, { width: 5000, height: 120 })).toEqual({
      scale: 1,
      x: FIT_MARGIN,
      y: (600 - 120) / 2,
    });
  });

  it("EDT-FR-LJAP: a fitted diagram is centred against the area, not the room, when it fits in the area", () => {
    // Scale 568 / 2200: the width 774.5 is more than the room but less than the area.
    const t = fitTransform({ width: 800, height: 600 }, { width: 3000, height: 2200 });
    expect(t.scale).toBeCloseTo(568 / 2200);
    expect(t.x).toBeCloseTo((800 - 3000 * t.scale) / 2);
    expect(t.x).toBeLessThan(FIT_MARGIN);
    expect(t.y).toBeCloseTo(FIT_MARGIN);
  });

  it("EDT-FR-LJAP: a small diagram still fits whole and enlarges", () => {
    const t = fitTransform({ width: 800, height: 600 }, { width: 100, height: 100 });
    expect(t.scale).toBeCloseTo(568 / 100);
    expect(t.x).toBeCloseTo((800 - 100 * t.scale) / 2);
    expect(t.y).toBeCloseTo(FIT_MARGIN);
  });

  it("EDT-FR-MZIF: a zoom keeps the point under it, and stays between 10% and 1000%", () => {
    const start = { scale: 2, x: 10, y: 20 };
    const next = zoomAt(start, 1.5, 110, 220);
    expect(next.scale).toBe(3);
    expect((110 - next.x) / next.scale).toBeCloseTo((110 - start.x) / start.scale);
    expect((220 - next.y) / next.scale).toBeCloseTo((220 - start.y) / start.scale);
    expect(zoomAt(start, 100, 0, 0).scale).toBe(MAX_ZOOM);
    expect(zoomAt(start, 0.001, 0, 0).scale).toBe(MIN_ZOOM);
  });

  it("EDT-FR-MZIF: the wheel up zooms in and the wheel down zooms out", () => {
    expect(wheelFactor(-100, 0)).toBeGreaterThan(1);
    expect(wheelFactor(100, 0)).toBeLessThan(1);
    expect(wheelFactor(-3, 1)).toBeCloseTo(wheelFactor(-48, 0));
  });

  it("EDT-FR-MZIF: a pinch zooms more for the same delta, and one event zooms not more than one step", () => {
    expect(wheelFactor(-5, 0, true)).toBeGreaterThan(wheelFactor(-5, 0));
    expect(wheelFactor(-5, 0, true)).toBeCloseTo(Math.exp(0.1));
    expect(wheelFactor(-1000, 0)).toBe(ZOOM_STEP);
    expect(wheelFactor(1000, 0, true)).toBe(1 / ZOOM_STEP);
    expect(wheelFactor(-1, 2)).toBe(ZOOM_STEP);
  });

  it("EDT-FR-PDBC: the wheel pans as a page scrolls, and Shift turns a vertical wheel horizontal", () => {
    expect(wheelPan(0, 100, 0, false)).toEqual({ dx: 0, dy: -100 });
    expect(wheelPan(-30, 40, 0, false)).toEqual({ dx: 30, dy: -40 });
    expect(wheelPan(0, 3, 1, false)).toEqual({ dx: 0, dy: -48 });
    expect(wheelPan(0, 1, 2, false)).toEqual({ dx: 0, dy: -800 });
    expect(wheelPan(0, 100, 0, true)).toEqual({ dx: -100, dy: 0 });
    // A platform that already gives a horizontal delta for Shift keeps it.
    expect(wheelPan(100, 0, 0, true)).toEqual({ dx: -100, dy: 0 });
    expect(wheelPan(100, 5, 0, true)).toEqual({ dx: -100, dy: -5 });
    expect(wheelPan(0, 0, 0, true)).toEqual({ dx: 0, dy: 0 });
  });

  it("EDT-FR-PDBC: a pan moves the diagram and keeps the zoom", () => {
    expect(panBy({ scale: 2, x: 1, y: 2 }, 10, -5)).toEqual({ scale: 2, x: 11, y: -3 });
  });

  it("EDT-FR-LJAP: the natural size of a drawing comes from its view box, else from its size", () => {
    const svg = (attrs: Record<string, string>) => {
      const el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      for (const [key, value] of Object.entries(attrs)) el.setAttribute(key, value);
      return el;
    };
    expect(naturalSize(svg({ viewBox: "-8 -8 316.5 120", width: "100%" }))).toEqual({
      width: 316.5,
      height: 120,
    });
    expect(naturalSize(svg({ width: "40", height: "30" }))).toEqual({ width: 40, height: 30 });
    expect(naturalSize(svg({ width: "100%", height: "30" }))).toBeNull();
    expect(naturalSize(svg({}))).toBeNull();
  });
});
