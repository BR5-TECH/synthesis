import { useEffect, useSyncExternalStore } from "react";
import { onAppPreferencesChanged } from "../events";
import { logDebug, logWarn } from "../logging";
import { loadAppPreferences, refreshAppPreferences } from "../state/appPreferences";
import {
  adoptStoredEditorPageWidth,
  getEditorPageWidth,
  subscribeEditorPageWidth,
} from "../state/editorPageWidth";

/** EPW-FR-QGUZ: the page width now, in pixels at 100% zoom. */
export function useEditorPageWidth(): number {
  return useSyncExternalStore(subscribeEditorPageWidth, getEditorPageWidth, getEditorPageWidth);
}

/**
 * EPW-FR-ZCBT: read the width at startup, and show the width that another
 * window saves with no reload. The same shape as `useEditorZoomSync`.
 */
export function useEditorPageWidthSync(): void {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const readFailed = (error: unknown) =>
      logWarn(["frontend"], "page width read failed", { error: String(error) });
    void loadAppPreferences()
      .then((prefs) => {
        if (!cancelled) adoptStoredEditorPageWidth(prefs);
      })
      .catch(readFailed);
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences()
        .then((prefs) => {
          if (!cancelled) adoptStoredEditorPageWidth(prefs);
        })
        .catch(readFailed);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((error: unknown) => {
        // Outside the Tauri runtime there is no event bus: the width read at
        // startup stays, and only the sync with other windows is lost.
        logDebug(["frontend"], "page width sync not listening", { error: String(error) });
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
