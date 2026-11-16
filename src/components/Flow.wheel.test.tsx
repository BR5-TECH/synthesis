/**
 * The wheel and the touchpad over a Flow canvas (`FLO-flow.md` FLO-FR-MGJH,
 * FLO-FR-MFPM, FLO-FR-TFHG). The Delivery lens of the Map hosts the same
 * canvas (`MDL-map-delivery-lens.md` MDL-FR-VLQJ, MDL-FR-XJBV).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import { wheelFactor } from "./DiagramEditor/viewerGeometry";
import { clearFlowStores, endFakeTimers, flowBackend, renderFlow } from "../test/flowCanvasHarness";
import { canvas, panel, selectItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { sampleLayout, samplePlan } from "../test/flowPlanFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(async () => {
  await endFakeTimers();
  cleanup();
  clearFlowStores();
});

const viewport = () => document.querySelector(".flow-viewport") as HTMLElement;

/** The translation and the scale of the view, read from the transform. */
function viewOf(): { x: number; y: number; scale: number } {
  const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(viewport().style.transform);
  if (!m) throw new Error(`no view in ${viewport().style.transform}`);
  return { x: Number(m[1]), y: Number(m[2]), scale: Number(m[3]) };
}

async function open() {
  await renderFlow(flowBackend(samplePlan(), sampleLayout()));
  return viewOf();
}

describe("a pinch over the canvas (FLO-FR-MGJH, FLO-FR-TFHG)", () => {
  it("FLO-FR-MGJH: a pinch zooms in proportion to the gesture, and the point under the pointer stays in place", async () => {
    const before = await open();
    const e = fireEvent.wheel(canvas(), { deltaY: -5, ctrlKey: true, clientX: 200, clientY: 120 });
    expect(e).toBe(false);
    const after = viewOf();
    const factor = wheelFactor(-5, 0, true);
    expect(after.scale).toBeCloseTo(before.scale * factor);
    // jsdom places the canvas at the origin, so the pointer is a canvas point.
    expect((200 - after.x) / after.scale).toBeCloseTo((200 - before.x) / before.scale);
    expect((120 - after.y) / after.scale).toBeCloseTo((120 - before.y) / before.scale);
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent(`${Math.round(after.scale * 100)}%`);
  });

  it("FLO-FR-MGJH: the scale stays in the range of the zoom controls", async () => {
    await open();
    for (let i = 0; i < 40; i += 1) fireEvent.wheel(canvas(), { deltaY: -50, metaKey: true });
    expect(viewOf().scale).toBe(2);
    for (let i = 0; i < 80; i += 1) fireEvent.wheel(canvas(), { deltaY: 50, ctrlKey: true });
    expect(viewOf().scale).toBe(0.25);
  });

  it("FLO-FR-MGJH: a WebKit pinch gesture zooms by the change of its scale", async () => {
    const before = await open();
    const start = new Event("gesturestart", { bubbles: true, cancelable: true });
    Object.assign(start, { scale: 1, clientX: 0, clientY: 0 });
    act(() => {
      canvas().dispatchEvent(start);
    });
    const change = new Event("gesturechange", { bubbles: true, cancelable: true });
    Object.assign(change, { scale: 1.5, clientX: 0, clientY: 0 });
    act(() => {
      canvas().dispatchEvent(change);
    });
    expect(change.defaultPrevented).toBe(true);
    expect(viewOf().scale).toBeCloseTo(before.scale * 1.5);
  });

  it("FLO-FR-TFHG: a gesture over the canvas keeps the selection", async () => {
    await open();
    selectItem("t1");
    fireEvent.wheel(canvas(), { deltaY: -5, ctrlKey: true });
    fireEvent.wheel(canvas(), { deltaY: 30 });
    expect(document.querySelector('[data-testid="flow-item-t1"]')).toHaveAttribute("data-selected", "true");
  });
});

describe("a scroll over the canvas (FLO-FR-MFPM, FLO-FR-TFHG)", () => {
  it("FLO-FR-MFPM: a two-finger scroll pans the view by its distance, and scrolls no other surface", async () => {
    const before = await open();
    const e = fireEvent.wheel(canvas(), { deltaX: 15, deltaY: 40 });
    expect(e).toBe(false);
    expect(viewOf()).toEqual({ x: before.x - 15, y: before.y - 40, scale: before.scale });
  });

  it("FLO-FR-MFPM: a wheel with Shift pans the view horizontally", async () => {
    const before = await open();
    fireEvent.wheel(canvas(), { deltaY: 25, shiftKey: true });
    expect(viewOf()).toEqual({ x: before.x - 25, y: before.y, scale: before.scale });
  });

  it("FLO-FR-MFPM: over the panel of a selected item the wheel scrolls the panel, and the view stays", async () => {
    const before = await open();
    selectItem("t1");
    const e = fireEvent.wheel(panel(), { deltaY: 40 });
    expect(e).toBe(true);
    expect(viewOf()).toEqual(before);
  });
});
