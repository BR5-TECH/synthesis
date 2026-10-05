import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { TreeIndex } from "../../state/specMap/tree";
import type { MapRef } from "../../state/specMap/types";
import type { MapView } from "../../state/specMap/session";
import type { MapLayout } from "./layout";
import { hitTestDrop, type DropTarget } from "./dnd";
import { screenToWorld } from "./zoom";

/** SMO-FR-DKTM: a press becomes a drag after this much movement. */
export const DRAG_THRESHOLD = 4;

export interface DragState {
  ref: MapRef;
  world: { x: number; y: number };
  target: DropTarget | null;
}

interface DragOptions {
  canvas: HTMLElement | null;
  tree: TreeIndex | null;
  layout: MapLayout | null;
  view: MapView;
  onDrop: (ref: MapRef, target: DropTarget) => void;
}

interface Press {
  ref: MapRef;
  x: number;
  y: number;
  active: boolean;
  target: DropTarget | null;
}

/**
 * SMO-FR-DKTM / SMO-FR-ARQV / SMO-FR-JYVB: the drag gesture of a map node.
 *
 * A press is only a drag once it has moved; before that it stays a click. The
 * listeners go on the window, so a drag that leaves the canvas still ends.
 */
export function useMapDrag(options: DragOptions) {
  const [drag, setDrag] = useState<DragState | null>(null);
  const latest = useRef(options);
  latest.current = options;
  const press = useRef<Press | null>(null);
  const suppressClick = useRef(false);
  const detach = useRef<(() => void) | null>(null);

  useEffect(() => () => detach.current?.(), []);

  const pointerDown = useCallback(
    (ref: MapRef) => (e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      // A press on a node never pans the canvas (SMZ-FR-KAHU).
      e.stopPropagation();
      suppressClick.current = false;
      detach.current?.();
      press.current = { ref, x: e.clientX, y: e.clientY, active: false, target: null };

      const end = () => {
        detach.current?.();
        press.current = null;
        setDrag(null);
      };
      const move = (ev: PointerEvent) => {
        const p = press.current;
        if (!p) return;
        if (!p.active && Math.hypot(ev.clientX - p.x, ev.clientY - p.y) < DRAG_THRESHOLD) return;
        p.active = true;
        const { canvas, tree, layout, view } = latest.current;
        if (!canvas || !tree || !layout) return;
        const world = screenToWorld(ev.clientX, ev.clientY, canvas.getBoundingClientRect(), view);
        p.target = hitTestDrop(tree, layout, p.ref, world);
        setDrag({ ref: p.ref, world, target: p.target });
      };
      const up = () => {
        const p = press.current;
        if (p?.active) {
          suppressClick.current = true;
          if (p.target) latest.current.onDrop(p.ref, p.target);
        }
        end();
      };
      // SMO-FR-JYVB: Escape and a pointer cancel end the drag and change nothing.
      const cancel = () => {
        if (press.current?.active) suppressClick.current = true;
        end();
      };
      const key = (ev: KeyboardEvent) => {
        if (ev.key === "Escape") cancel();
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", cancel);
      window.addEventListener("keydown", key);
      detach.current = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", cancel);
        window.removeEventListener("keydown", key);
        detach.current = null;
      };
    },
    [],
  );

  /** True once for the click that ends a drag, so the drag does not also select. */
  const consumeClick = useCallback((): boolean => {
    if (!suppressClick.current) return false;
    suppressClick.current = false;
    return true;
  }, []);

  return { drag, pointerDown, consumeClick };
}
