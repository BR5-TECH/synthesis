/**
 * The options of `bindCanvasGestures` that the editor zoom uses
 * (`EZM-editor-zoom.md`): the target filter, the start and the end of a pinch,
 * and the two-finger touchscreen pinch.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { bindCanvasGestures } from "./canvasGestures";

let host: HTMLDivElement;
let inside: HTMLDivElement;
let outside: HTMLDivElement;
let unbind: () => void;
const onPinch = vi.fn();
const onPinchStart = vi.fn();
const onPinchEnd = vi.fn();

function wheel(target: Element, init: WheelEventInit): WheelEvent {
  const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
}

function gesture(target: Element, type: string, scale: number): Event {
  const e = new Event(type, { bubbles: true, cancelable: true });
  Object.assign(e, { scale, clientX: 10, clientY: 20 });
  target.dispatchEvent(e);
  return e;
}

/** A pointer event of a touch. jsdom has no `PointerEvent`, so it is built by hand. */
function touch(target: Element, type: string, id: number, x: number, y: number): Event {
  const e = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y });
  Object.assign(e, { pointerId: id, pointerType: "touch" });
  target.dispatchEvent(e);
  return e;
}

beforeEach(() => {
  onPinch.mockClear();
  onPinchStart.mockClear();
  onPinchEnd.mockClear();
  host = document.createElement("div");
  inside = document.createElement("div");
  inside.className = "accepted";
  outside = document.createElement("div");
  host.append(inside, outside);
  document.body.appendChild(host);
  unbind = bindCanvasGestures(host, {
    onPinch,
    onPinchStart,
    onPinchEnd,
    touch: true,
    accepts: (target) => target instanceof Element && target.closest(".accepted") !== null,
  });
});

afterEach(() => {
  unbind();
  host.remove();
});

describe("the target filter", () => {
  it("EZM-FR-COUH: a Ctrl wheel over a rejected target is not prevented and changes nothing", () => {
    const e = wheel(outside, { deltaY: -10, ctrlKey: true });
    expect(e.defaultPrevented).toBe(false);
    expect(onPinch).not.toHaveBeenCalled();
  });

  it("EZM-FR-GMSX: a Ctrl or Cmd wheel over an accepted target pinches and is prevented", () => {
    const ctrl = wheel(inside, { deltaY: -10, ctrlKey: true });
    const meta = wheel(inside, { deltaY: 10, metaKey: true });
    expect(ctrl.defaultPrevented).toBe(true);
    expect(meta.defaultPrevented).toBe(true);
    expect(onPinch).toHaveBeenCalledTimes(2);
    expect(onPinch.mock.calls[0][0]).toBeGreaterThan(1);
    expect(onPinch.mock.calls[1][0]).toBeLessThan(1);
  });

  it("EZM-FR-GMSX: a plain wheel with no scroll handler is left to the page", () => {
    const e = wheel(inside, { deltaY: 10 });
    expect(e.defaultPrevented).toBe(false);
    expect(onPinch).not.toHaveBeenCalled();
  });

  it("EZM-FR-BNFJ: a WebKit gesture that starts over a rejected target is left alone to its end", () => {
    const start = gesture(outside, "gesturestart", 1);
    const change = gesture(outside, "gesturechange", 1.5);
    gesture(outside, "gestureend", 1.5);
    expect(start.defaultPrevented).toBe(false);
    expect(change.defaultPrevented).toBe(false);
    expect(onPinch).not.toHaveBeenCalled();
    expect(onPinchStart).not.toHaveBeenCalled();
    expect(onPinchEnd).not.toHaveBeenCalled();
  });

  it("EZM-FR-BNFJ: a rejected gesture that moves over an accepted target stays rejected", () => {
    gesture(outside, "gesturestart", 1);
    gesture(inside, "gesturechange", 2);
    expect(onPinch).not.toHaveBeenCalled();
  });
});

describe("the start and the end of a pinch", () => {
  it("EZM-FR-GMSX: a new gesture decides again when the end of the one before never came", () => {
    gesture(outside, "gesturestart", 1);
    gesture(inside, "gesturestart", 1);
    gesture(inside, "gesturechange", 2);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
    expect(onPinch.mock.calls.map((c) => c[0])).toEqual([2]);
  });

  it("EZM-FR-GMSX, EZM-FR-VPYL: a WebKit gesture starts, pinches by its scale, and ends once", () => {
    gesture(inside, "gesturestart", 1);
    gesture(inside, "gesturechange", 1.5);
    gesture(inside, "gesturechange", 3);
    gesture(inside, "gestureend", 3);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
    expect(onPinch.mock.calls.map((c) => c[0])).toEqual([1.5, 2]);
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
  });

  it("EZM-FR-GMSX: a Ctrl wheel during a WebKit gesture is the same pinch and does not count twice", () => {
    gesture(inside, "gesturestart", 1);
    const e = wheel(inside, { deltaY: -10, ctrlKey: true });
    expect(e.defaultPrevented).toBe(true);
    expect(onPinch).not.toHaveBeenCalled();
  });
});

describe("a two-finger touchscreen pinch", () => {
  it("EZM-FR-VQHI: two touches that move apart pinch by the ratio of their distances", () => {
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
    const move = touch(inside, "pointermove", 2, 200, 0);
    expect(move.defaultPrevented).toBe(true);
    expect(onPinch).toHaveBeenCalledWith(2, 100, 0);
    touch(inside, "pointerup", 2, 200, 0);
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
  });

  it("EZM-FR-VQHI: one touch pans and does not pinch", () => {
    touch(inside, "pointerdown", 1, 0, 0);
    const move = touch(inside, "pointermove", 1, 50, 50);
    touch(inside, "pointerup", 1, 50, 50);
    expect(move.defaultPrevented).toBe(false);
    expect(onPinch).not.toHaveBeenCalled();
    expect(onPinchStart).not.toHaveBeenCalled();
  });

  it("EZM-FR-VQHI: a mouse or pen pointer is not a touch", () => {
    const e = new MouseEvent("pointerdown", { bubbles: true });
    Object.assign(e, { pointerId: 1, pointerType: "mouse" });
    inside.dispatchEvent(e);
    touch(inside, "pointerdown", 2, 100, 0);
    expect(onPinchStart).not.toHaveBeenCalled();
  });

  it("EZM-FR-VQHI: a cancelled touch ends the pinch", () => {
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    touch(inside, "pointercancel", 1, 0, 0);
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
  });

  it("EZM-FR-COUH: a touch pinch over a rejected target is left alone", () => {
    touch(outside, "pointerdown", 1, 0, 0);
    touch(outside, "pointerdown", 2, 100, 0);
    const move = touch(outside, "pointermove", 2, 200, 0);
    expect(move.defaultPrevented).toBe(false);
    expect(onPinch).not.toHaveBeenCalled();
    touch(outside, "pointerup", 2, 200, 0);
    touch(outside, "pointerup", 1, 0, 0);
    // The next pinch over an accepted target is taken again.
    touch(inside, "pointerdown", 3, 0, 0);
    touch(inside, "pointerdown", 4, 100, 0);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
  });

  it("EZM-FR-VQHI: a pinch that the platform reports as touches and as a gesture changes the level once", () => {
    // The touches start first, so they own the pinch.
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    gesture(inside, "gesturestart", 1);
    gesture(inside, "gesturechange", 2);
    touch(inside, "pointermove", 2, 200, 0);
    gesture(inside, "gestureend", 2);
    touch(inside, "pointerup", 2, 200, 0);
    expect(onPinch).toHaveBeenCalledTimes(1);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
  });

  it("EZM-FR-VQHI: when the gesture starts first, the touches of the same pinch are ignored", () => {
    gesture(inside, "gesturestart", 1);
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    gesture(inside, "gesturechange", 2);
    touch(inside, "pointermove", 2, 200, 0);
    touch(inside, "pointerup", 2, 200, 0);
    gesture(inside, "gestureend", 2);
    expect(onPinch.mock.calls.map((c) => c[0])).toEqual([2]);
    expect(onPinchStart).toHaveBeenCalledTimes(1);
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
  });
});

describe("edges of a touch pinch", () => {
  it("EZM-FR-VQHI: when one of three fingers goes up, the pair that stays does not jump", () => {
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    touch(inside, "pointerdown", 3, 500, 0);
    // A move with three fingers down pinches nothing, but moves finger 2.
    touch(inside, "pointermove", 2, 150, 0);
    expect(onPinch).not.toHaveBeenCalled();
    touch(inside, "pointerup", 3, 500, 0);
    touch(inside, "pointermove", 2, 165, 0);
    expect(onPinch).toHaveBeenCalledTimes(1);
    expect(onPinch.mock.calls[0][0]).toBeCloseTo(1.1);
  });

  it("EZM-FR-VQHI: when a finger of the pair goes up with a third finger down, the new pair does not jump", () => {
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    touch(inside, "pointerdown", 3, 300, 0);
    touch(inside, "pointerup", 2, 100, 0);
    // The pair is now fingers 1 and 3, 300 px apart.
    touch(inside, "pointermove", 3, 330, 0);
    expect(onPinch).toHaveBeenCalledTimes(1);
    expect(onPinch.mock.calls[0][0]).toBeCloseTo(1.1);
  });

  it("EZM-FR-VPYL: a pinch in progress ends when the listeners go away", () => {
    gesture(inside, "gesturestart", 1);
    unbind();
    expect(onPinchEnd).toHaveBeenCalledTimes(1);
    unbind = bindCanvasGestures(host, { onPinch, onPinchStart, onPinchEnd, touch: true });
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    unbind();
    expect(onPinchEnd).toHaveBeenCalledTimes(2);
    // No end without a pinch.
    unbind = bindCanvasGestures(host, { onPinch, onPinchEnd });
    unbind();
    expect(onPinchEnd).toHaveBeenCalledTimes(2);
    unbind = () => {};
  });
});

describe("binding without the touch option", () => {
  it("EZM-FR-VQHI: Flow and the spec map, which do not ask for touches, take no touch pinch", () => {
    unbind();
    unbind = bindCanvasGestures(host, { onPinch, onScroll: () => {} });
    touch(inside, "pointerdown", 1, 0, 0);
    touch(inside, "pointerdown", 2, 100, 0);
    touch(inside, "pointermove", 2, 200, 0);
    expect(onPinch).not.toHaveBeenCalled();
  });
});
