/**
 * The wheel and the touchpad over a canvas (`FLO-flow.md` FLO-FR-MGJH,
 * FLO-FR-MFPM, FLO-FR-TFHG; `SMZ-specification-map-zoom.md` SMZ-FR-DLES).
 *
 * A touchpad pinch reaches the page in one of two forms. Chromium, and WebKit
 * where the page listens for wheels, send a `wheel` with `ctrlKey` set. WebKit
 * also sends its own `gesturestart` / `gesturechange` / `gestureend` events,
 * with the running scale of the pinch. Both are taken, and a wheel that holds
 * Ctrl during a WebKit gesture is the same pinch, so it is not applied twice.
 * A wheel with Ctrl or ⌘ held from a mouse is a pinch too.
 *
 * The scale of a pinch wheel and the pan of a plain wheel are those of the
 * diagram viewer (`EDT-editor.md` EDT-FR-MZIF, EDT-FR-PDBC), so every canvas of
 * the app answers a gesture alike.
 */

import { wheelFactor, wheelPan } from "./DiagramEditor/viewerGeometry";

/** A pinch of `factor` at a point of the screen. */
export type PinchHandler = (factor: number, clientX: number, clientY: number) => void;
/**
 * A wheel or two-finger scroll with no zoom modifier. `dx` and `dy` are the
 * pan of the view in pixels: the content moves as a page does when it scrolls,
 * and with Shift held a vertical wheel pans horizontally.
 */
export type ScrollHandler = (e: WheelEvent, dx: number, dy: number) => void;

/** WebKit's gesture event, which the DOM typings do not declare. */
interface GestureLike extends UIEvent {
  scale: number;
  clientX: number;
  clientY: number;
}

/** The handlers and the options of `bindCanvasGestures`. */
export interface CanvasGestureHandlers {
  onPinch: PinchHandler;
  /** A plain wheel. Without it, a plain wheel is left to the page. */
  onScroll?: ScrollHandler;
  /**
   * Whether a pinch that starts over `target` belongs to this element. A pinch
   * that does not is left alone: no handler runs and nothing is prevented, so a
   * surface inside the element keeps its own zoom (EZM-FR-BNFJ, EZM-FR-COUH).
   */
  accepts?: (target: EventTarget | null) => boolean;
  /** A pinch of a WebKit gesture or of two touches starts. */
  onPinchStart?: () => void;
  /** That pinch ends. A Ctrl or Cmd wheel has no start and no end. */
  onPinchEnd?: () => void;
  /** Also take a two-finger touchscreen pinch, by pointer events (EZM-FR-VQHI). */
  touch?: boolean;
}

/**
 * Bind the wheel and the touchpad of `el`. Each listener is not passive, so a
 * gesture over the canvas scrolls no other surface and zooms no page
 * (FLO-FR-TFHG). A scroll handler that returns without calling
 * `preventDefault` leaves the event to the surface under the pointer. Returns
 * the function that removes the listeners.
 *
 * One pinch has one source. When a platform reports a touchscreen pinch both
 * as touches and as a WebKit gesture, the source that starts first owns the
 * pinch and the other is ignored until it ends (EZM-FR-VQHI).
 */
export function bindCanvasGestures(el: HTMLElement, handlers: CanvasGestureHandlers): () => void {
  const accepts = (target: EventTarget | null) => !handlers.accepts || handlers.accepts(target);
  let gestureScale: number | null = null;
  /** The source of the pinch in progress, or `rejected` for a pinch left alone. */
  let owner: "gesture" | "touch" | "rejected" | null = null;
  const touches = new Map<number, { x: number; y: number }>();
  let touchDistance = 0;

  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      if (owner === "rejected" || !accepts(e.target)) return;
      e.preventDefault();
      // A WebKit gesture or a touch pinch already carries this pinch.
      if (gestureScale !== null || owner !== null) return;
      if (e.deltaY === 0) return;
      handlers.onPinch(wheelFactor(e.deltaY, e.deltaMode, e.ctrlKey), e.clientX, e.clientY);
      return;
    }
    if (!handlers.onScroll) return;
    const { dx, dy } = wheelPan(e.deltaX, e.deltaY, e.deltaMode, e.shiftKey);
    handlers.onScroll(e, dx, dy);
  };
  const onGestureStart = (e: Event) => {
    // A new gesture starts a new decision, also when the end of the one before
    // never came. A touch pinch in progress keeps its pinch (EZM-FR-VQHI).
    if (owner === "rejected" || owner === "gesture") owner = null;
    if (owner === null && !accepts(e.target)) {
      owner = "rejected";
      return;
    }
    e.preventDefault();
    gestureScale = (e as GestureLike).scale || 1;
    if (owner === null) {
      owner = "gesture";
      handlers.onPinchStart?.();
    }
  };
  const onGestureChange = (e: Event) => {
    if (owner === "rejected") return;
    e.preventDefault();
    const g = e as GestureLike;
    const previous = gestureScale ?? 1;
    gestureScale = g.scale || previous;
    if (owner !== "gesture") return;
    if (previous <= 0 || gestureScale === previous) return;
    handlers.onPinch(gestureScale / previous, g.clientX, g.clientY);
  };
  const onGestureEnd = (e: Event) => {
    gestureScale = null;
    if (owner === "rejected") {
      owner = null;
      return;
    }
    e.preventDefault();
    if (owner === "gesture") {
      owner = null;
      handlers.onPinchEnd?.();
    }
  };

  const distance = () => {
    const [a, b] = [...touches.values()];
    return Math.hypot(a.x - b.x, a.y - b.y);
  };
  const onPointerDown = (e: PointerEvent) => {
    if (e.pointerType !== "touch") return;
    touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (touches.size !== 2 || owner !== null) return;
    if (!accepts(e.target)) {
      owner = "rejected";
      return;
    }
    owner = "touch";
    touchDistance = distance();
    handlers.onPinchStart?.();
  };
  const onPointerMove = (e: PointerEvent) => {
    const known = touches.get(e.pointerId);
    if (!known) return;
    known.x = e.clientX;
    known.y = e.clientY;
    if (owner !== "touch" || touches.size !== 2) return;
    e.preventDefault();
    const next = distance();
    if (touchDistance > 0 && next > 0 && next !== touchDistance) {
      const [a, b] = [...touches.values()];
      handlers.onPinch(next / touchDistance, (a.x + b.x) / 2, (a.y + b.y) / 2);
    }
    touchDistance = next;
  };
  const onPointerEnd = (e: PointerEvent) => {
    if (!touches.delete(e.pointerId)) return;
    if (touches.size >= 2) {
      // A third finger went up: measure the pair that stays from now on, so
      // the next move does not jump.
      touchDistance = distance();
      return;
    }
    if (owner === "touch") {
      owner = null;
      handlers.onPinchEnd?.();
    } else if (owner === "rejected" && gestureScale === null) {
      owner = null;
    }
  };

  el.addEventListener("wheel", onWheel, { passive: false });
  el.addEventListener("gesturestart", onGestureStart, { passive: false });
  el.addEventListener("gesturechange", onGestureChange, { passive: false });
  el.addEventListener("gestureend", onGestureEnd, { passive: false });
  if (handlers.touch) {
    el.addEventListener("pointerdown", onPointerDown);
    el.addEventListener("pointermove", onPointerMove, { passive: false });
    el.addEventListener("pointerup", onPointerEnd);
    el.addEventListener("pointercancel", onPointerEnd);
  }
  return () => {
    // A pinch that is still in progress ends with the listeners, so its owner
    // does not wait for an end that never comes.
    if (owner === "gesture" || owner === "touch") handlers.onPinchEnd?.();
    owner = null;
    el.removeEventListener("wheel", onWheel);
    el.removeEventListener("gesturestart", onGestureStart);
    el.removeEventListener("gesturechange", onGestureChange);
    el.removeEventListener("gestureend", onGestureEnd);
    el.removeEventListener("pointerdown", onPointerDown);
    el.removeEventListener("pointermove", onPointerMove);
    el.removeEventListener("pointerup", onPointerEnd);
    el.removeEventListener("pointercancel", onPointerEnd);
  };
}
