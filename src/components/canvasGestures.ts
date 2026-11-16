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

/**
 * Bind the wheel and the touchpad of `el`. Each listener is not passive, so a
 * gesture over the canvas scrolls no other surface and zooms no page
 * (FLO-FR-TFHG). A scroll handler that returns without calling
 * `preventDefault` leaves the event to the surface under the pointer. Returns
 * the function that removes the listeners.
 */
export function bindCanvasGestures(
  el: HTMLElement,
  handlers: { onPinch: PinchHandler; onScroll: ScrollHandler },
): () => void {
  let gestureScale: number | null = null;

  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      // The WebKit gesture already carries this pinch.
      if (gestureScale !== null) return;
      if (e.deltaY === 0) return;
      handlers.onPinch(wheelFactor(e.deltaY, e.deltaMode, e.ctrlKey), e.clientX, e.clientY);
      return;
    }
    const { dx, dy } = wheelPan(e.deltaX, e.deltaY, e.deltaMode, e.shiftKey);
    handlers.onScroll(e, dx, dy);
  };
  const onGestureStart = (e: Event) => {
    e.preventDefault();
    gestureScale = (e as GestureLike).scale || 1;
  };
  const onGestureChange = (e: Event) => {
    e.preventDefault();
    const g = e as GestureLike;
    const previous = gestureScale ?? 1;
    gestureScale = g.scale || previous;
    if (previous <= 0 || gestureScale === previous) return;
    handlers.onPinch(gestureScale / previous, g.clientX, g.clientY);
  };
  const onGestureEnd = (e: Event) => {
    e.preventDefault();
    gestureScale = null;
  };

  el.addEventListener("wheel", onWheel, { passive: false });
  el.addEventListener("gesturestart", onGestureStart, { passive: false });
  el.addEventListener("gesturechange", onGestureChange, { passive: false });
  el.addEventListener("gestureend", onGestureEnd, { passive: false });
  return () => {
    el.removeEventListener("wheel", onWheel);
    el.removeEventListener("gesturestart", onGestureStart);
    el.removeEventListener("gesturechange", onGestureChange);
    el.removeEventListener("gestureend", onGestureEnd);
  };
}
