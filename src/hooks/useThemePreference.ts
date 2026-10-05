import { useEffect, useState } from "react";
import {
  loadAppPreferences,
  patchAppPreferences,
  refreshAppPreferences,
} from "../state/appPreferences";
import { onAppPreferencesChanged } from "../events";
import { resolveTheme } from "../components/GlobalSettings";
import type { ThemePreference } from "../types";

/**
 * Owns the single shared user-global theme *preference* (light | dark | system)
 * and applies the resolved appearance to the document root.
 *
 * The *preference* (not the resolved value) is kept so "system" intent survives,
 * keeping OS-scheme tracking (OVW-FR-10) expressible. Two equal entry points
 * edit it (SNV-FR-15): the Global settings Appearance section (GLS-FR-05), which
 * reports its selection up via `setThemePref` and persists itself, and the
 * top-chrome selector, which goes through `selectTheme`. Default is `system`
 * (GSS-FR-04) until the persisted value loads (OVW-FR-09 / SNV-FR-15).
 */
export function useThemePreference() {
  const [themePref, setThemePref] = useState<ThemePreference>("system");
  const theme = resolveTheme(themePref);

  // Single applier of the app-wide theme to the root (OVW-FR-09).
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  // OVW-FR-09 / SNV-FR-15: load the persisted theme preference at startup so the
  // always-mounted top-chrome selector (and the root) reflect the saved value
  // from first paint, before the Global settings window is ever opened.
  useEffect(() => {
    loadAppPreferences()
      .then((prefs) => setThemePref(prefs?.theme ?? "system"))
      .catch(() => setThemePref("system"));
  }, []);

  // GLS-FR-05 / SWN-FR-01: the Appearance section is in a child window now, so
  // a theme chosen there reaches this window as an announcement rather than
  // through shared state. Applying it here is what makes "applies it to the
  // application root immediately" true of the window the author is looking past
  // the settings window at, and not only of the settings window itself.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences().then((prefs) => {
        if (!cancelled) setThemePref(prefs.theme ?? "system");
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

  // SNV-FR-16: choosing a theme in the top-chrome selector applies it to the
  // root immediately (via themePref -> the applier effect) and persists it
  // through the same save the Appearance section uses. The chrome selector has
  // no dirty/error affordance of its own (owned by Global settings, GLS-FR-13),
  // so on a persistence failure we roll back the applied value rather than leave
  // the UI showing a choice that will not survive relaunch.
  // GLS-FR-14 / GSS-FR-20: patched rather than replaced. `save_app_preferences`
  // writes the record whole, so sending a bare `{ theme }` would clear the
  // main window's full-screen state, which shares the record and is edited only
  // by the shell (SNV-FR-38).
  const selectTheme = (pref: ThemePreference) => {
    const previous = themePref;
    setThemePref(pref);
    patchAppPreferences({ theme: pref }).catch(() => {
      setThemePref(previous);
    });
  };

  return { themePref, setThemePref, theme, selectTheme };
}
