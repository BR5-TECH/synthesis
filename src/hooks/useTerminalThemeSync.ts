import { useEffect, useSyncExternalStore } from "react";
import { onAppPreferencesChanged } from "../events";
import { loadAppPreferences, refreshAppPreferences } from "../state/appPreferences";
import {
  adoptStoredTerminalTheme,
  getTerminalThemePreference,
  subscribeTerminalThemePreference,
} from "../state/terminalThemes";
import type { TerminalThemePreference } from "../types";

/** GLS-FR-SOJD: the Terminal theme of Settings now. */
export function useTerminalThemePreference(): TerminalThemePreference {
  return useSyncExternalStore(subscribeTerminalThemePreference, getTerminalThemePreference, getTerminalThemePreference);
}

/**
 * GLS-FR-VTVK, TRM-FR-IUMC: read the Terminal theme at startup, and apply the
 * one that another window saves with no reload. The same shape as
 * `useEditorZoomSync`.
 */
export function useTerminalThemeSync(): void {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void loadAppPreferences().then((prefs) => {
      if (!cancelled) adoptStoredTerminalTheme(prefs);
    });
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences().then((prefs) => {
        if (!cancelled) adoptStoredTerminalTheme(prefs);
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
