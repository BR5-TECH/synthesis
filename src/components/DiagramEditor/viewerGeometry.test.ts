import { describe, expect, it } from "vitest";

import {
  FIT_MARGIN,
  MAX_ZOOM,
  MIN_ZOOM,
  fitTransform,
  naturalSize,
  panBy,
  wheelFactor,
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
    expect(fitTransform({ width: 100, height: 100 }, { width: 100000, height: 10 }).scale).toBe(
      MIN_ZOOM,
    );
    expect(fitTransform({ width: 0, height: 0 }, { width: 10, height: 10 })).toEqual({
      scale: 1,
      x: 0,
      y: 0,
    });
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
