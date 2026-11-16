import { useEffect, useRef } from "react";

import type { Drag, View } from "./geometry";

/** DBR-FR-IQOM: how near the edge of the canvas the pointer starts the pan. */
export const AUTO_PAN_EDGE = 56;
/** DBR-FR-IQOM: the pan of one frame, in screen pixels, at the very edge. */
export const AUTO_PAN_MAX = 18;

/**
 * DBR-FR-IQOM: the pan of one frame for a pointer at `client` in the canvas
 * `rect`: toward the near edge, faster as the pointer comes nearer it. Zero
 * away from the edges.
 */
export function autoPanStep(
  client: { x: number; y: number },
  rect: { left: number; top: number; width: number; height: number },
): { dx: number; dy: number } {
  const speed = (distance: number) =>
    distance < AUTO_PAN_EDGE ? Math.ceil(((AUTO_PAN_EDGE - Math.max(distance, 0)) / AUTO_PAN_EDGE) * AUTO_PAN_MAX) : 0;
  const left = client.x - rect.left;
  const right = rect.left + rect.width - client.x;
  const top = client.y - rect.top;
  const bottom = rect.top + rect.height - client.y;
  return { dx: speed(left) - speed(right), dy: speed(top) - speed(bottom) };
}

/**
 * DBR-FR-BBDZ / DBR-FR-IQOM: while a drag of the board runs, **Escape** puts
 * the item back, and a pointer near an edge of the canvas pans the view. The
 * pan moves the dragged item with the pointer, so `onPanned` takes the new
 * position of the drag in canvas units.
 */
/** DBR-FR-IQOM: the room the pan keeps between the board and the far edge of the canvas. */
export const AUTO_PAN_MARGIN = 80;

/**
 * DBR-FR-IQOM: the pan of one frame, limited so that the board, `extent` in
 * canvas units, never leaves the canvas: the pan stops once the far edge of
 * the board is `AUTO_PAN_MARGIN` inside the canvas.
 */
export function limitPan(
  step: { dx: number; dy: number },
  view: View,
  extent: { width: number; height: number },
  rect: { width: number; height: number },
): { dx: number; dy: number } {
  const clamp = (d: number, at: number, size: number, room: number) => {
    if (d < 0) return Math.max(d, Math.min(0, room - AUTO_PAN_MARGIN - (at + size)));
    if (d > 0) return Math.min(d, Math.max(0, AUTO_PAN_MARGIN - at));
    return 0;
  };
  return {
    dx: clamp(step.dx, view.x, extent.width * view.scale, rect.width),
    dy: clamp(step.dy, view.y, extent.height * view.scale, rect.height),
  };
}

export function useBoardDrag(ctx: {
  active: boolean;
  /** DBR-FR-IQOM: the size of the board, in canvas units, from the origin. */
  extent: () => { width: number; height: number };
  dragRef: { current: Drag | null };
  surfaceRef: { current: HTMLElement | null };
  view: View;
  panBy: (dx: number, dy: number) => void;
  onPanned: (next: Drag) => void;
  onCancel: () => void;
}) {
  const latest = useRef(ctx);
  latest.current = ctx;
  const { active } = ctx;

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      latest.current.onCancel();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [active]);

  useEffect(() => {
    if (!active || typeof requestAnimationFrame === "undefined") return;
    let frame = 0;
    const tick = () => {
      const { dragRef, surfaceRef, view, panBy, onPanned } = latest.current;
      const drag = dragRef.current;
      const el = surfaceRef.current;
      if (drag?.moved && drag.client && el) {
        const r = el.getBoundingClientRect();
        // A canvas with no measured size has no edge to pan toward.
        const { dx, dy } =
          r.width > 0 && r.height > 0
            ? limitPan(autoPanStep(drag.client, r), view, latest.current.extent(), r)
            : { dx: 0, dy: 0 };
        if (dx !== 0 || dy !== 0) {
          panBy(dx, dy);
          // The view moved under the pointer, so the point under it moved too.
          const sx = -dx / view.scale;
          const sy = -dy / view.scale;
          onPanned({
            ...drag,
            x: drag.x + sx,
            y: drag.y + sy,
            pointer: { x: drag.pointer.x + sx, y: drag.pointer.y + sy },
          });
        }
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [active]);
}
