import { describe, expect, it } from "vitest";
import {
  BUTTON_STEP,
  SCALE_MAX,
  SCALE_MIN,
  WHEEL_IN,
  WHEEL_OUT,
  centerScale,
  centeredOn,
  firstPaintView,
  placeHoverCard,
  scaleLabel,
  screenToWorld,
  worldToCanvas,
  zoomStep,
} from "./zoom";
import type { MapView } from "../../state/specMap/session";

const view = (patch: Partial<MapView>): MapView => ({
  level: 1,
  scale: 1,
  tx: 100,
  ty: -50,
  touched: true,
  ...patch,
});

describe("semantic zoom (SMZ-FR-GMEB, SMZ-FR-ZQLP)", () => {
  it("SMZ-FR-ZQLP, SMZ-FR-HCTF: a wheel step multiplies the scale and the translation by the same ratio", () => {
    const next = zoomStep(view({}), WHEEL_IN, 3);
    expect(next.level).toBe(1);
    expect(next.scale).toBeCloseTo(1.06);
    expect(next.tx).toBeCloseTo(106);
    expect(next.ty).toBeCloseTo(-53);
    expect(zoomStep(view({}), WHEEL_OUT, 3).scale).toBeCloseTo(0.94);
    expect(WHEEL_IN).toBe(1.06);
    expect(WHEEL_OUT).toBe(0.94);
  });

  it("SMZ-FR-ZQLP, SMZ-FR-HCTF: crossing 1.30 steps one level deeper at 0.66", () => {
    const next = zoomStep(view({ scale: 1.25 }), WHEEL_IN, 3);
    expect(next.level).toBe(2);
    expect(next.scale).toBe(SCALE_MIN);
    expect(next.tx).toBeCloseTo(100 * (0.66 / 1.25));
  });

  it("SMZ-FR-ZQLP: crossing 0.66 steps one level shallower at 1.30", () => {
    const next = zoomStep(view({ scale: 0.68 }), WHEEL_OUT, 3);
    expect([next.level, next.scale]).toEqual([0, SCALE_MAX]);
  });

  it("SMZ-FR-GMEB, SMZ-FR-ZQLP: the scale clamps at the last level and at level 0", () => {
    expect(zoomStep(view({ level: 3, scale: 1.29 }), WHEEL_IN, 3)).toMatchObject({ level: 3, scale: SCALE_MAX });
    expect(zoomStep(view({ level: 0, scale: 0.67 }), WHEEL_OUT, 3)).toMatchObject({ level: 0, scale: SCALE_MIN });
    for (let i = 0, v = view({ level: 0, scale: 0.8 }); i < 60; i++) {
      v = zoomStep(v, i < 30 ? BUTTON_STEP : 1 / BUTTON_STEP, 3);
      expect(v.scale).toBeGreaterThanOrEqual(SCALE_MIN);
      expect(v.scale).toBeLessThanOrEqual(SCALE_MAX);
    }
  });

  it("SMZ-FR-JWXA: the buttons zoom by 1.18 and the label shows a whole percent", () => {
    expect(BUTTON_STEP).toBe(1.18);
    expect(zoomStep(view({ scale: 1 }), 1 / BUTTON_STEP, 3).scale).toBeCloseTo(0.8475);
    expect(scaleLabel(0.8475)).toBe("85%");
  });
});

describe("framing (SMZ-FR-NUOB, SMZ-FR-EPTR)", () => {
  it("SMZ-FR-NUOB: first paint is level 0 at 0.8, top-aligned from the measured height", () => {
    expect(firstPaintView(1000, 600)).toEqual({ level: 0, scale: 0.8, tx: 0, ty: 126, touched: false });
    // Content that fits keeps a translation of zero rather than a negative one.
    expect(firstPaintView(200, 800).ty).toBe(0);
  });

  it("SMZ-FR-EPTR: a chosen level opens at 0.8, or 0.85 at the last level, centred on the node", () => {
    expect(centerScale(1, 3)).toBe(0.8);
    expect(centerScale(3, 3)).toBe(0.85);
    expect(centeredOn({ x: 100, y: 0, w: 200, h: 100 }, 2, 0.8)).toEqual({
      level: 2,
      scale: 0.8,
      tx: -160,
      ty: -40,
      touched: true,
    });
    // A tall node is centred on a point at most 240px below its top.
    expect(centeredOn({ x: 0, y: 0, w: 0, h: 2000 }, 1, 1).ty).toBe(-240);
  });

  it("converts between the screen and the world in both directions", () => {
    const v = view({ scale: 0.75, tx: 30, ty: -20 });
    const rect = { left: 50, top: 80, width: 800, height: 600 };
    const world = screenToWorld(700, 300, rect, v);
    const back = worldToCanvas(world.x, world.y, v, rect.width, rect.height);
    expect(back.x + rect.left).toBeCloseTo(700);
    expect(back.y + rect.top).toBeCloseTo(300);
  });
});

describe("the hover card (SMN-FR-MVWA)", () => {
  it("SMN-FR-MVWA: sits 18px right of and 14px below the node, clamped 8px inside the canvas", () => {
    const v = view({ scale: 1, tx: 0, ty: 0 });
    expect(placeHoverCard(-100, -100, v, 1000, 800)).toEqual({ x: 418, y: 314 });
    expect(placeHoverCard(480, 380, v, 1000, 800)).toEqual({ x: 712, y: 622 });
    expect(placeHoverCard(-600, -500, v, 1000, 800)).toEqual({ x: 8, y: 8 });
  });
});
