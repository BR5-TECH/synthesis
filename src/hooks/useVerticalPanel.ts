import { useCallback, useEffect, useRef, useState } from "react";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
} from "../state/layoutPreferences";
import {
  DEFAULT_VPANEL_FRACTION,
  fractionToPx,
  pxToFraction,
  shellInnerWidth,
} from "../state/panelLayout";

/**
 * The vertical panel's width, and the drag that changes it
 * (SNV-shell-navigation.md SNV-FR-33 – SNV-FR-37).
 *
 * The panel's width is a **fraction** of the shell's inner width, not a pixel
 * count (SNV-FR-34). This hook owns that fraction, resolves it to pixels against
 * the live shell width so the panel keeps its proportion as the window resizes
 * (SNV-FR-35), and persists it — once, when the drag ends (SNV-FR-36), rather
 * than on every pointer move.
 *
 * The write goes through `state/layoutPreferences`, which merges it into the
 * record as it currently stands. That matters because `useMainWindowState`
 * writes the window's geometry into the *same* record on a different trigger:
 * if each hook wrote a snapshot it took at mount, whichever wrote second would
 * revert the other's field.
 */
export interface VerticalPanelState {
  /** The width to render the panel at, in CSS pixels, already clamped. */
  widthPx: number;
  /** True while the user is dragging the splitter — drives the drag styling. */
  dragging: boolean;
  /**
   * SNV-FR-37: the splitter exists only while the panel renders content at full
   * width. A collapsed or hidden panel has no boundary to drag.
   */
  resizable: boolean;
  /**
   * SNV-FR-05(c) / SNV-FR-45: the panel renders nothing and the main viewport
   * takes its space; the activity bar stays put.
   */
  hidden: boolean;
  /**
   * Show or hide the panel, persisting the choice (SNV-FR-08).
   *
   * Deliberately does not touch the width fraction: hiding leaves the persisted
   * value alone so reopening returns the panel to exactly the width it had
   * (SNV-FR-37, SNV-FR-45).
   */
  setHidden: (hidden: boolean) => void;
  /** Begin a drag from a pointer-down on the splitter. */
  onResizeStart: (event: {
    clientX: number;
    preventDefault?: () => void;
  }) => void;
}

/**
 * Measure the element the panel is sized against. Falls back to
 * `window.innerWidth` before the first layout pass (and under jsdom, where
 * `getBoundingClientRect` reports zeroes), so the panel always resolves to a
 * usable width rather than to the clamp floor.
 */
function measureShellWidth(el: HTMLElement | null): number {
  const measured = el?.getBoundingClientRect().width ?? 0;
  if (measured > 0) return measured;
  return typeof window !== "undefined" ? window.innerWidth : 0;
}

export function useVerticalPanel(
  shellRef: React.RefObject<HTMLElement | null>,
  enabled: boolean,
  /**
   * Identity of the open project. Switching projects (OVW-FR-11) opens a
   * different layout slot in the backend, so the persisted fraction is re-read
   * — otherwise B renders at A's width and A's width is written into B's slot.
   */
  projectKey: string,
): VerticalPanelState {
  const [fraction, setFraction] = useState(DEFAULT_VPANEL_FRACTION);
  const [collapsed, setCollapsed] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [shellWidth, setShellWidth] = useState(() =>
    typeof window !== "undefined" ? window.innerWidth : 0,
  );
  const [dragging, setDragging] = useState(false);

  // The live fraction, readable from the pointer handlers without making them
  // depend on (and re-subscribe to) each render's state value.
  const fractionRef = useRef(fraction);
  fractionRef.current = fraction;
  // Whether a drag is in flight. Guards against a second pointerdown stacking a
  // second set of window listeners on top of the first — which would leave the
  // panel resizing on every pointer move with no button held, and firing a save
  // on every click.
  const draggingRef = useRef(false);
  // Tears down the current drag's listeners. Held in a ref so the unmount
  // cleanup can reach a drag that is still in flight.
  const endDragRef = useRef<(() => void) | null>(null);
  const projectRef = useRef(projectKey);
  projectRef.current = projectKey;

  // SNV-FR-34 / SNV-FR-36: restore the persisted fraction, and the collapse /
  // hide state that decides whether the splitter exists at all.
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    loadLayoutPreferences(projectKey)
      .then((prefs) => {
        if (cancelled) return;
        setFraction(
          typeof prefs.verticalPanelFraction === "number" &&
            prefs.verticalPanelFraction > 0
            ? prefs.verticalPanelFraction
            : DEFAULT_VPANEL_FRACTION,
        );
        setCollapsed(!!prefs.verticalPanelCollapsed);
        setHidden(!!prefs.verticalPanelHidden);
      })
      .catch(() => {
        // No persisted layout (or no backend): the default fraction stands.
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, projectKey]);

  // A drag still in flight when the shell unmounts must not keep listening —
  // its `pointerup` would otherwise fire after the project was torn down and
  // write this project's panel width into whichever slot the backend now
  // resolves to.
  useEffect(() => {
    return () => endDragRef.current?.();
  }, []);

  // SNV-FR-35: the fraction is invariant under a change of window size, so the
  // rendered pixel width has to be recomputed whenever the shell resizes. A
  // window `resize` listener rather than a `ResizeObserver`: the shell fills the
  // window, so the two fire together, and `resize` needs no polyfill under
  // jsdom.
  useEffect(() => {
    if (!enabled) return;
    const measure = () => setShellWidth(measureShellWidth(shellRef.current));
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [enabled, shellRef]);

  const innerWidth = shellInnerWidth(shellWidth);
  const widthPx = fractionToPx(fraction, innerWidth);

  /**
   * SNV-FR-33: track the pointer for the length of the drag, and SNV-FR-36:
   * persist exactly once, on release.
   *
   * The listeners go on `window`, not on the splitter, so a fast drag whose
   * pointer outruns the 5px-wide handle keeps resizing instead of stalling.
   * They are installed per drag and torn down on release, so nothing is
   * listening while the user is not dragging.
   */
  const onResizeStart = useCallback(
    (event: { clientX: number; preventDefault?: () => void }) => {
      // A drag already in flight owns the listeners. Starting a second one
      // would leave the first's permanently installed.
      if (draggingRef.current) return;
      event.preventDefault?.();
      const el = shellRef.current;
      const width = measureShellWidth(el);
      const inner = shellInnerWidth(width);
      // The panel's left edge in client coordinates: the shell's own left edge
      // plus the activity bar. Deriving the width from the pointer's offset
      // from that edge (rather than accumulating deltas) keeps the boundary
      // pinned to the cursor even if a move event is dropped.
      const shellLeft = el?.getBoundingClientRect().left ?? 0;
      const panelLeft = shellLeft + (width - inner);

      setShellWidth(width);
      setDragging(true);
      draggingRef.current = true;

      const onMove = (e: PointerEvent | MouseEvent) => {
        setFraction(pxToFraction(e.clientX - panelLeft, inner));
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
        // SNV-FR-36: one write, on release. Patched rather than replaced — the
        // same record carries the window geometry `useMainWindowState` owns.
        void patchLayoutPreferences(projectRef.current, {
          verticalPanelFraction: fractionRef.current,
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

  /**
   * SNV-FR-45: hiding and showing the panel from the activity bar. Persisted
   * immediately rather than on some later flush — the toggle is the whole
   * gesture, so there is no "end of the interaction" to wait for the way a drag
   * has one (SNV-FR-36).
   */
  const setHiddenAndPersist = useCallback((next: boolean) => {
    setHidden(next);
    void patchLayoutPreferences(projectRef.current, {
      verticalPanelHidden: next,
    }).catch(() => {
      // A failed write leaves the panel as the user left it for this session.
    });
  }, []);

  return {
    widthPx,
    dragging,
    resizable: !collapsed && !hidden,
    hidden,
    setHidden: setHiddenAndPersist,
    onResizeStart,
  };
}
