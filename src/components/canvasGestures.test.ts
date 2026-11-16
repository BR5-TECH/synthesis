/**
 * The wheel and the touchpad over a canvas (`FLO-flow.md` FLO-FR-MGJH,
 * FLO-FR-MFPM, FLO-FR-TFHG; `SMZ-specification-map-zoom.md` SMZ-FR-DLES).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { bindCanvasGestures } from "./canvasGestures";
import { wheelFactor } from "./DiagramEditor/viewerGeometry";

let el: HTMLDivElement;
let unbind: () => void;
const onPinch = vi.fn();
const onScroll = vi.fn((e: WheelEvent, _dx: number, _dy: number) => e.preventDefault());

function wheel(init: WheelEventInit): WheelEvent {
  const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  el.dispatchEvent(e);
  return e;
}

/** WebKit's gesture event, with the running scale of the pinch. */
function gesture(type: string, scale: number, at = { x: 30, y: 40 }): Event {
  const e = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(e, { scale, clientX: at.x, clientY: at.y });
  el.dispatchEvent(e);
  return e;
}

beforeEach(() => {
  onPinch.mockClear();
  onScroll.mockClear();
  el = document.createElement("div");
  document.body.appendChild(el);
  unbind = bindCanvasGestures(el, { onPinch, onScroll });
});

afterEach(() => {
  unbind();
  el.remove();
});

describe("a pinch (FLO-FR-MGJH, SMZ-FR-DLES, FLO-FR-TFHG)", () => {
  it("FLO-FR-MGJH: a pinch wheel zooms in proportion to its distance, at the pointer, and zooms no page", () => {
    const e = wheel({ deltaY: -4, ctrlKey: true, clientX: 12, clientY: 34 });
    expect(e.defaultPrevented).toBe(true);
    expect(onPinch).toHaveBeenCalledWith(wheelFactor(-4, 0, true), 12, 34);
    expect(onPinch.mock.calls[0][0]).toBeGreaterThan(1);
    wheel({ deltaY: -8, ctrlKey: true });
    // Twice the distance is a larger step.
    expect(onPinch.mock.calls[1][0]).toBeGreaterThan(onPinch.mock.calls[0][0]);
    wheel({ deltaY: 4, ctrlKey: true });
    expect(onPinch.mock.calls[2][0]).toBeLessThan(1);
    expect(onScroll).not.toHaveBeenCalled();
  });

  it("FLO-FR-MGJH: a wheel with ⌘ held is a pinch, and a pinch wheel with no vertical distance does nothing", () => {
    wheel({ deltaY: 100, metaKey: true });
    expect(onPinch).toHaveBeenCalledTimes(1);
    // A ⌘ wheel is a mouse wheel, so it takes the rate of a wheel, not of a pinch.
    expect(onPinch.mock.calls[0][0]).toBe(wheelFactor(100, 0, false));
    const e = wheel({ deltaX: 20, ctrlKey: true });
    expect(e.defaultPrevented).toBe(true);
    expect(onPinch).toHaveBeenCalledTimes(1);
  });

  it("SMZ-FR-DLES: a WebKit gesture zooms by the change of its scale, at the pointer, and its own wheels do not zoom twice", () => {
    expect(gesture("gesturestart", 1).defaultPrevented).toBe(true);
    expect(gesture("gesturechange", 1.5).defaultPrevented).toBe(true);
    expect(onPinch).toHaveBeenLastCalledWith(1.5, 30, 40);
    gesture("gesturechange", 1.2);
    expect(onPinch.mock.calls[1][0]).toBeCloseTo(0.8);
    wheel({ deltaY: -10, ctrlKey: true });
    expect(onPinch).toHaveBeenCalledTimes(2);
    gesture("gestureend", 1.2);
    wheel({ deltaY: -10, ctrlKey: true });
    expect(onPinch).toHaveBeenCalledTimes(3);
  });

  it("FLO-FR-TFHG: the listeners go when the canvas unbinds them", () => {
    unbind();
    wheel({ deltaY: -4, ctrlKey: true });
    gesture("gesturechange", 2);
    wheel({ deltaY: 10 });
    expect(onPinch).not.toHaveBeenCalled();
    expect(onScroll).not.toHaveBeenCalled();
    unbind = () => {};
  });
});

describe("a scroll (FLO-FR-MFPM)", () => {
  it("FLO-FR-MFPM: a two-finger scroll pans by its distance, the content moving as a page does", () => {
    wheel({ deltaX: 15, deltaY: 40 });
    expect(onScroll).toHaveBeenCalledTimes(1);
    expect(onScroll.mock.calls[0].slice(1)).toEqual([-15, -40]);
    expect(onPinch).not.toHaveBeenCalled();
  });

  it("FLO-FR-MFPM: with Shift a vertical wheel pans horizontally, and lines count as pixels", () => {
    wheel({ deltaY: 25, shiftKey: true });
    expect(onScroll.mock.calls[0].slice(1)).toEqual([-25, 0]);
    wheel({ deltaY: 2, deltaMode: 1 });
    expect(onScroll.mock.calls[1][2]).toBeLessThan(-2);
  });
});
