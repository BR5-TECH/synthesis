/**
 * The place of the selection panel (FLO layout notes, FLO-FR-44): attached
 * below the item, bound to the room left in the canvas, and opened above the
 * item when the room below is short.
 */
import { describe, expect, it } from "vitest";

import { PANEL_W, panelPlacement } from "./geometry";

const SURFACE = { width: 1200, height: 800 };
/** The canvas keeps its bottom line (52) free; the panel stops above it. */
const FLOOR = SURFACE.height - 52;

describe("panelPlacement (FLO layout notes, FLO-FR-44)", () => {
  it("FLO-FR-44: with room below, the panel opens below the item and its height is bound to the room left", () => {
    const at = panelPlacement({ left: 100, top: 100, bottom: 200 }, SURFACE);
    expect(at).toEqual({ left: 100, top: 210, maxHeight: FLOOR - 210, placement: "below" });
  });

  it("FLO-FR-44: near the bottom of the canvas, the panel opens above the item and stays inside the canvas", () => {
    const at = panelPlacement({ left: 100, top: 560, bottom: 680 }, SURFACE);
    expect(at.placement).toBe("above");
    expect(at.top).toBeUndefined();
    // The panel's foot is 10 above the item, and its height fits above that.
    expect(at.bottom).toBe(SURFACE.height - 550);
    expect(at.maxHeight).toBe(550 - 8);
    expect(SURFACE.height - at.bottom! - at.maxHeight!).toBeGreaterThanOrEqual(8);
  });

  it("FLO-FR-44: with short room on both sides, the larger side wins, and the panel never runs past the canvas", () => {
    const small = { width: 1200, height: 420 };
    const below = panelPlacement({ left: 0, top: 40, bottom: 120 }, small);
    expect(below.placement).toBe("below");
    expect(below.top! + below.maxHeight!).toBeLessThanOrEqual(small.height - 52);
    const above = panelPlacement({ left: 0, top: 300, bottom: 360 }, small);
    expect(above.placement).toBe("above");
    expect(small.height - above.bottom! - above.maxHeight!).toBeGreaterThanOrEqual(8);
  });

  it("FLO-FR-44: an item outside the view still gets a panel inside the canvas", () => {
    const under = panelPlacement({ left: 0, top: 900, bottom: 1000 }, SURFACE);
    expect(under.placement).toBe("above");
    expect(SURFACE.height - under.bottom!).toBeLessThanOrEqual(FLOOR);
    expect(under.bottom).toBeGreaterThanOrEqual(52);
    const over = panelPlacement({ left: 0, top: -400, bottom: -300 }, SURFACE);
    expect(over.placement).toBe("below");
    expect(over.top).toBe(8);
    expect(over.maxHeight).toBe(FLOOR - 8);
  });

  it("FLO-FR-44: the measured height of the panel decides whether the room below is enough", () => {
    // 238 below the item (748 - 510), 382 above it (390 - 8).
    const item = { left: 100, top: 400, bottom: 500 };
    expect(panelPlacement(item, SURFACE, 200).placement).toBe("below");
    expect(panelPlacement(item, SURFACE, 200).maxHeight).toBe(FLOOR - 510);
    expect(panelPlacement(item, SURFACE, 300).placement).toBe("above");
  });

  it("FLO-FR-44: the panel keeps inside the canvas width", () => {
    expect(panelPlacement({ left: 1100, top: 0, bottom: 50 }, SURFACE).left).toBe(
      SURFACE.width - PANEL_W - 16,
    );
    expect(panelPlacement({ left: -50, top: 0, bottom: 50 }, SURFACE).left).toBe(8);
  });

  it("FLO-FR-44: before the canvas has a size, the panel sits below the item with no bound", () => {
    expect(panelPlacement({ left: 40, top: 10, bottom: 60 }, { width: 0, height: 0 })).toEqual({
      left: 40,
      top: 70,
      placement: "below",
    });
  });
});
