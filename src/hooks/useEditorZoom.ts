import { useEffect, useSyncExternalStore } from "react";
import { onAppPreferencesChanged } from "../events";
import { loadAppPreferences, refreshAppPreferences } from "../state/appPreferences";
import {
  adoptStoredEditorZoom,
  getEditorZoom,
  getEditorZoomPercent,
  subscribeEditorZoom,
} from "../state/editorZoom";

/** EZM-FR-SOZM: the editor zoom level now, in percent. */
export function useEditorZoomLevel(): number {
  return useSyncExternalStore(subscribeEditorZoom, getEditorZoom, getEditorZoom);
}

/**
 * EZM-FR-GMSX: the level as a whole percent. A component that shows only the
 * whole percent renders again only when that changes, not on each step of a
 * pinch.
 */
export function useEditorZoomPercent(): number {
  return useSyncExternalStore(subscribeEditorZoom, getEditorZoomPercent, getEditorZoomPercent);
}

/**
 * EZM-FR-HYQD: read the level at startup, and show the level that another
 * window saves with no reload. The same shape as `useFontRoles`.
 */
export function useEditorZoomSync(): void {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void loadAppPreferences().then((prefs) => {
      if (!cancelled) adoptStoredEditorZoom(prefs);
    });
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences().then((prefs) => {
        if (!cancelled) adoptStoredEditorZoom(prefs);
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
