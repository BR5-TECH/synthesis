/**
 * EDT-FR-LJAP, EDT-FR-MZIF, EDT-FR-PDBC, EDT-FR-JJDP: the fit, the zoom, and
 * the pan of a drawing of known natural size in a diagram area. The Mermaid
 * viewer and every interactive CALM canvas (`CAD-calm-architecture-diagrams.md`
 * CAD-FR-MBML) use it, so both follow the same rules.
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";

import { logDebug } from "../../logging";
import {
  PAN_STEP,
  ZOOM_STEP,
  fitTransform,
  panBy,
  wheelFactor,
  wheelPan,
  zoomAt,
  type Size,
  type ViewTransform,
} from "./viewerGeometry";

const sizeOf = (el: HTMLElement | null): Size =>
  el ? { width: el.clientWidth, height: el.clientHeight } : { width: 0, height: 0 };

export const sameSize = (a: Size | null, b: Size | null) =>
  a === b || (a !== null && b !== null && a.width === b.width && a.height === b.height);

/** A move shorter than this, in pixels, is a click and not a pan. */
const DRAG_THRESHOLD = 3;

/** EDT-FR-LJAP: the fit of the diagram, and a log of its result. Numbers only. */
function fitAndLog(area: Size, natural: Size, reason: string): ViewTransform {
  const t = fitTransform(area, natural);
  logDebug(["frontend"], "the diagram viewer fits the diagram", {
    reason,
    areaWidth: area.width,
    areaHeight: area.height,
    diagramWidth: natural.width,
    diagramHeight: natural.height,
    scale: t.scale,
    whole: natural.width * t.scale <= area.width && natural.height * t.scale <= area.height,
  });
  return t;
}

export interface PanZoom {
  transform: ViewTransform;
  /** EDT-FR-JJDP: true until the author zooms or pans. */
  fitted: boolean;
  dragging: boolean;
  /** EDT-FR-LJAP: the Fit control and the key `0`. */
  fit: () => void;
  onKeyDown: (e: ReactKeyboardEvent<HTMLElement>) => void;
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  /**
   * Pans so that `box`, in the coordinates of the drawing, shows in the
   * middle of the area, when it does not show completely now.
   */
  reveal: (box: { x: number; y: number; width: number; height: number }) => void;
}

/**
 * The view of a drawing of size `natural` in the area `areaRef`. While the view
 * is fitted, a new size of the area and a new `renderKey` fit it again.
 */
export function usePanZoom(
  areaRef: RefObject<HTMLDivElement | null>,
  natural: Size | null,
  renderKey: unknown,
): PanZoom {
  const [area, setArea] = useState<Size>({ width: 0, height: 0 });
  const [transform, setTransform] = useState<ViewTransform>({ scale: 1, x: 0, y: 0 });
  const [fitted, setFitted] = useState(true);
  const [dragging, setDragging] = useState(false);
  /** The transform of the last render, for a change that reads it outside an updater. */
  const transformRef = useRef(transform);
  transformRef.current = transform;

  // EDT-FR-JJDP: the size of the diagram area, again at each change.
  useLayoutEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const measure = () =>
      setArea((last) => {
        const next = sizeOf(el);
        return sameSize(last, next) ? last : next;
      });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [areaRef]);

  // EDT-FR-LJAP, EDT-FR-JJDP: while fitted, a new size or a new render fits
  // the diagram again. A new drawing is logged once; a change of size is not,
  // so that a resize by drag does not fill the log.
  const loggedFit = useRef<Size | null>(null);
  useLayoutEffect(() => {
    if (!fitted || !natural) return;
    if (loggedFit.current !== natural && area.width > 0 && area.height > 0) {
      loggedFit.current = natural;
      setTransform(fitAndLog(area, natural, "render"));
      return;
    }
    setTransform(fitTransform(area, natural));
  }, [fitted, area, natural, renderKey]);

  const fit = useCallback(() => {
    setFitted(true);
    if (natural) setTransform(fitAndLog(sizeOf(areaRef.current), natural, "fit control"));
  }, [natural, areaRef]);

  /** EDT-FR-MZIF, EDT-FR-PDBC: a zoom or a pan ends the fit. */
  const change = useCallback((next: (t: ViewTransform) => ViewTransform) => {
    setFitted(false);
    setTransform(next);
  }, []);

  // EDT-FR-MZIF, EDT-FR-PDBC: a pinch, or the wheel with Ctrl or Cmd held,
  // zooms about the pointer. The wheel alone pans. A native listener, because
  // the listener that React adds for the wheel cannot stop the page scroll.
  useEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        // An event with no vertical delta does not zoom, so it keeps the fit.
        if (e.deltaY === 0) return;
        const box = el.getBoundingClientRect();
        const factor = wheelFactor(e.deltaY, e.deltaMode, e.ctrlKey);
        change((t) => zoomAt(t, factor, e.clientX - box.left, e.clientY - box.top));
        return;
      }
      const { dx, dy } = wheelPan(e.deltaX, e.deltaY, e.deltaMode, e.shiftKey);
      if (dx !== 0 || dy !== 0) change((t) => panBy(t, dx, dy));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [change, areaRef]);

  /** EDT-FR-MZIF, EDT-FR-PDBC, EDT-FR-LJAP: the keys of the diagram area. */
  const onKeyDown = (e: ReactKeyboardEvent<HTMLElement>) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const centre = sizeOf(areaRef.current);
    const zoom = (factor: number) =>
      change((t) => zoomAt(t, factor, centre.width / 2, centre.height / 2));
    switch (e.key) {
      case "+":
      case "=":
        zoom(ZOOM_STEP);
        break;
      case "-":
      case "_":
        zoom(1 / ZOOM_STEP);
        break;
      case "0":
        fit();
        break;
      // The diagram moves in the direction of the key.
      case "ArrowLeft":
        change((t) => panBy(t, -PAN_STEP, 0));
        break;
      case "ArrowRight":
        change((t) => panBy(t, PAN_STEP, 0));
        break;
      case "ArrowUp":
        change((t) => panBy(t, 0, -PAN_STEP));
        break;
      case "ArrowDown":
        change((t) => panBy(t, 0, PAN_STEP));
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  /**
   * EDT-FR-PDBC: a drag with the primary button pans the diagram. The pointer
   * is captured only once it moves, so a click without a move still reaches
   * the item under it (`CAD-calm-architecture-diagrams.md` CAD-FR-KUBU).
   */
  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const target = e.currentTarget;
    target.focus();
    const pointerId = e.pointerId;
    const startX = e.clientX;
    const startY = e.clientY;
    let lastX = startX;
    let lastY = startY;
    let moving = false;
    const onMove = (ev: PointerEvent) => {
      if (!moving) {
        if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD) return;
        moving = true;
        setDragging(true);
        target.setPointerCapture?.(pointerId);
      }
      const dx = ev.clientX - lastX;
      const dy = ev.clientY - lastY;
      lastX = ev.clientX;
      lastY = ev.clientY;
      if (dx !== 0 || dy !== 0) change((t) => panBy(t, dx, dy));
    };
    const onUp = () => {
      setDragging(false);
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
      window.removeEventListener("pointerup", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
    // A release outside the area before the drag began never reaches it.
    window.addEventListener("pointerup", onUp);
  };

  const reveal = useCallback(
    (box: { x: number; y: number; width: number; height: number }) => {
      const size = sizeOf(areaRef.current);
      const t = transformRef.current;
      const left = t.x + box.x * t.scale;
      const top = t.y + box.y * t.scale;
      const inside =
        left >= 0 &&
        top >= 0 &&
        left + box.width * t.scale <= size.width &&
        top + box.height * t.scale <= size.height;
      if (inside) return;
      setFitted(false);
      setTransform({
        scale: t.scale,
        x: size.width / 2 - (box.x + box.width / 2) * t.scale,
        y: size.height / 2 - (box.y + box.height / 2) * t.scale,
      });
    },
    [areaRef],
  );

  return { transform, fitted, dragging, fit, onKeyDown, onPointerDown, reveal };
}
