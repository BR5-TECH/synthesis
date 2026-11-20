import { useEffect, useRef } from "react";
import * as api from "../api";
import { logWarn } from "../logging";
import {
  MENU_ZOOM_IN,
  MENU_ZOOM_OUT,
  MENU_ZOOM_RESET,
  onMenuEvent,
  onMenuZoomSet,
} from "../events";
import { isActiveRootGone } from "../state/activeRootHealth";
import {
  getEditorZoomPercent,
  resetEditorZoom,
  setEditorZoom,
  subscribeEditorZoom,
  zoomEditorIn,
  zoomEditorOut,
} from "../state/editorZoom";

/**
 * SNV-FR-LVDP / SNV-FR-QHMA / SNV-FR-IPCN: the Zoom submenu of the Window menu.
 *
 * The items act on the level of this window (EZM-FR-DMLT). The menu is shared
 * by every window, so only the window that has focus pushes the check mark and
 * the enabled state, and it pushes them again each time it gains focus.
 *
 * The push follows the store directly rather than a render, so a pinch does not
 * render the host again (the non-functional requirements of EZM).
 */
export function useEditorZoomMenu(zoomable: boolean): void {
  const zoomableRef = useRef(zoomable);
  zoomableRef.current = zoomable;

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const keep = (pending: Promise<() => void>) => {
      void pending.then((fn) => {
        if (cancelled) fn();
        else unlisten.push(fn);
      });
    };
    // SNV-FR-IPCN: a disabled item sends nothing. The checks here also hold
    // the items while the worktree gone dialog is open (WTS-FR-ZHLG).
    const gated = (run: () => void) => () => {
      if (zoomableRef.current && !isActiveRootGone()) run();
    };
    keep(onMenuEvent(MENU_ZOOM_IN, gated(zoomEditorIn)));
    keep(onMenuEvent(MENU_ZOOM_OUT, gated(zoomEditorOut)));
    keep(onMenuEvent(MENU_ZOOM_RESET, gated(resetEditorZoom)));
    keep(onMenuZoomSet((percent) => gated(() => setEditorZoom(percent))()));
    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    let pushed: number | null = null;
    const push = (force: boolean) => {
      const level = getEditorZoomPercent();
      if (!force && level === pushed) return;
      if (typeof document !== "undefined" && !document.hasFocus()) return;
      pushed = level;
      void api.setZoomMenuState(level, zoomable).catch((error: unknown) => {
        logWarn(["frontend"], "zoom menu state push failed", { level, error: String(error) });
      });
    };
    const onFocus = () => push(true);
    push(true);
    const unsubscribe = subscribeEditorZoom(() => push(false));
    window.addEventListener("focus", onFocus);
    return () => {
      unsubscribe();
      window.removeEventListener("focus", onFocus);
    };
  }, [zoomable]);
}
