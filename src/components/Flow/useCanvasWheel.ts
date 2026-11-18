/**
 * FLO-FR-MGJH / FLO-FR-MFPM / FLO-FR-TFHG: the wheel and the touchpad over the
 * canvas. A pinch, or a wheel with Ctrl or ⌘ held, zooms about the pointer. A
 * two-finger scroll, or a plain wheel, pans. Over the panel of a selected item
 * a scroll is the panel's own, so the panel scrolls and the view stays.
 */
import { useEffect, useRef, type RefObject } from "react";

import { bindCanvasGestures } from "../canvasGestures";

/** The surfaces over the canvas that scroll on their own. */
const SCROLLS_ON_ITS_OWN = ".flow-panel, .miv-sheet, [role='menu'], [role='listbox']";

export function useCanvasWheel(
  surfaceRef: RefObject<HTMLDivElement | null>,
  mounted: boolean,
  actions: {
    zoomAt: (factor: number, clientX: number, clientY: number) => void;
    panBy: (dx: number, dy: number) => void;
  },
): void {
  const latest = useRef(actions);
  latest.current = actions;
  useEffect(() => {
    const el = surfaceRef.current;
    if (!mounted || !el) return;
    return bindCanvasGestures(el, {
      onPinch: (factor, clientX, clientY) => latest.current.zoomAt(factor, clientX, clientY),
      onScroll: (e, dx, dy) => {
        const target = e.target instanceof Element ? e.target : null;
        if (target?.closest(SCROLLS_ON_ITS_OWN)) return;
        e.preventDefault();
        latest.current.panBy(dx, dy);
      },
    });
  }, [surfaceRef, mounted]);
}
