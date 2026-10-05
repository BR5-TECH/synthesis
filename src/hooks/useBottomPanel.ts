import { useCallback, useEffect, useRef, useState } from "react";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
} from "../state/layoutPreferences";
import {
  DEFAULT_BPANEL_PX,
  bottomPanelPx,
  clampBottomPanelPx,
} from "../state/panelLayout";

/**
 * The bottom panel's height, and the drag that changes it
 * (`SNV-shell-navigation.md` SNV-FR-50 – SNV-FR-53).
 *
 * The sibling of `useVerticalPanel`, and deliberately shaped like it: restore
 * the persisted value on mount, resolve it against the live shell so the clamp
 * is re-applied as the window changes size (SNV-FR-52), track the pointer for
 * the length of a drag, and persist exactly once on release (SNV-FR-53).
 *
 * The one real difference is the unit. The vertical panel stores a *fraction*
 * so it keeps its proportion of the window (SNV-FR-34); this one stores
 * *pixels*, because a bottom panel is read as a number of lines of output and
 * should not grow just because the window did. The 75% ceiling is what makes
 * that safe: a height set on a tall window is clamped down on a short one
 * rather than swallowing the viewport.
 */
export interface BottomPanelState {
  /** The height to render at, in CSS pixels, already clamped. */
  heightPx: number;
  /** True while the user is dragging the splitter — drives the drag styling. */
  dragging: boolean;
  /** Begin a drag from a pointer-down on the splitter. */
  onResizeStart: (event: {
    clientY: number;
    preventDefault?: () => void;
  }) => void;
}

/**
 * Measure the element the panel is sized against. Falls back to
 * `window.innerHeight` before the first layout pass (and under jsdom, where
 * `getBoundingClientRect` reports zeroes), so the panel always resolves to a
 * usable height rather than to the clamp floor.
 */
function measureShellHeight(el: HTMLElement | null): number {
  const measured = el?.getBoundingClientRect().height ?? 0;
  if (measured > 0) return measured;
  return typeof window !== "undefined" ? window.innerHeight : 0;
}

export function useBottomPanel(
  shellRef: React.RefObject<HTMLElement | null>,
  enabled: boolean,
  /**
   * Identity of the open project. Switching projects (OVW-FR-11) opens a
   * different layout slot, so the persisted height is re-read — otherwise B
   * renders at A's height and A's height is written into B's slot.
   */
  projectKey: string,
): BottomPanelState {
  const [stored, setStored] = useState(DEFAULT_BPANEL_PX);
  const [shellHeight, setShellHeight] = useState(() =>
    typeof window !== "undefined" ? window.innerHeight : 0,
  );
  const [dragging, setDragging] = useState(false);

  // The live stored height, readable from the pointer handlers without making
  // them depend on (and re-subscribe to) each render's state value.
  const storedRef = useRef(stored);
  storedRef.current = stored;
  // Whether a drag is in flight. Guards against a second pointerdown stacking a
  // second set of window listeners on top of the first.
  const draggingRef = useRef(false);
  // Tears down the current drag's listeners, so the unmount cleanup can reach a
  // drag still in flight.
  const endDragRef = useRef<(() => void) | null>(null);
  const projectRef = useRef(projectKey);
  projectRef.current = projectKey;

  // SNV-FR-53: restore the persisted height for this project.
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    loadLayoutPreferences(projectKey)
      .then((prefs) => {
        if (cancelled) return;
        setStored(
          typeof prefs.bottomPanelHeight === "number" &&
            prefs.bottomPanelHeight > 0
            ? prefs.bottomPanelHeight
            : DEFAULT_BPANEL_PX,
        );
      })
      .catch(() => {
        // No persisted layout (or no backend): the default height stands.
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, projectKey]);

  // A drag still in flight when the shell unmounts must not keep listening —
  // its `pointerup` would otherwise fire after the project was torn down.
  useEffect(() => {
    return () => endDragRef.current?.();
  }, []);

  // SNV-FR-52: the clamp is re-applied at every shell height, so the rendered
  // height has to be recomputed whenever the window resizes.
  useEffect(() => {
    if (!enabled) return;
    const measure = () => setShellHeight(measureShellHeight(shellRef.current));
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [enabled, shellRef]);

  const heightPx = bottomPanelPx(stored, shellHeight);

  /**
   * SNV-FR-50: track the pointer for the length of the drag, and SNV-FR-53:
   * persist exactly once, on release.
   *
   * Listeners go on `window` rather than on the splitter, so a fast drag whose
   * pointer outruns the handle keeps resizing instead of stalling. The height
   * is derived from the pointer's distance to the shell's bottom edge rather
   * than from accumulated deltas, so the boundary stays pinned to the cursor
   * even if a move event is dropped.
   */
  const onResizeStart = useCallback(
    (event: { clientY: number; preventDefault?: () => void }) => {
      if (draggingRef.current) return;
      event.preventDefault?.();
      const el = shellRef.current;
      const height = measureShellHeight(el);
      const rect = el?.getBoundingClientRect();
      const shellBottom = rect ? rect.bottom : height;

      setShellHeight(height);
      setDragging(true);
      draggingRef.current = true;

      const onMove = (e: PointerEvent | MouseEvent) => {
        // Dragging upward grows the panel: its height is the gap between the
        // pointer and the shell's bottom edge.
        setStored(clampBottomPanelPx(shellBottom - e.clientY, height));
      };
      /** Detach this drag's listeners. Idempotent — unmount may also call it. */
      const detach = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        draggingRef.current = false;
        endDragRef.current = null;
        setDragging(false);
      };
      const onUp = () => {
        if (!draggingRef.current) return;
        detach();
        // SNV-FR-53: one write, on release. Patched rather than replaced — the
        // same record carries the vertical panel's fraction and the window
        // geometry, which other hooks own.
        void patchLayoutPreferences(projectRef.current, {
          bottomPanelHeight: storedRef.current,
        }).catch(() => {
          // A failed write leaves the panel where the user dropped it for this
          // session; the next successful save carries the same value.
        });
      };

      endDragRef.current = detach;
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
    },
    [shellRef],
  );

  return { heightPx, dragging, onResizeStart };
}
