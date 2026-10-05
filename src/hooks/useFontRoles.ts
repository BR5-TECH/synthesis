import { useEffect, useState } from "react";
import {
  loadAppPreferences,
  refreshAppPreferences,
} from "../state/appPreferences";
import { onAppPreferencesChanged } from "../events";
import { applyFontRoles } from "../state/fontRoles";
import type { FontSettings } from "../types";

/**
 * Owns the three typographic roles' font settings and applies them to the
 * document root (OVW-FR-14).
 *
 * The same shape as `useThemePreference`, and for the same reason: the roles
 * are a user-global preference with two properties — they must be on the root
 * before any surface renders, and a change from the Appearance section must
 * reach every open surface at once. Applying them to `:root` rather than
 * through a React context is what gives both: the stylesheet's tokens resolve
 * against the root, so a change re-typesets open surfaces **without remounting
 * them**, and an Editor keeps its buffer, its undo history, and its scroll
 * position across one (the non-functional promise of `OVW-overview.md`).
 *
 * The Appearance section is the one editor of these values (GLS-FR-17), so this
 * hook only reads and applies: the section reports its selection up through
 * `setFonts` and owns persisting it, exactly as it owns persisting the theme.
 */
export function useFontRoles() {
  const [fonts, setFonts] = useState<FontSettings | undefined>(undefined);

  // The single applier (OVW-FR-14). Runs on the first render too, so the
  // built-in defaults are in place before the persisted record arrives and
  // there is no window in which a role is unstyled.
  useEffect(() => {
    applyFontRoles(document.documentElement, fonts);
  }, [fonts]);

  // OVW-FR-14 / GLS-FR-17: load the persisted settings at startup so both the
  // Project picker and the main window render in them before the Global
  // settings window is ever opened.
  //
  // KNOWN LIMIT, stated rather than hidden: the record is read over the Tauri
  // bridge, which is asynchronous, so the first paint necessarily happens on
  // the built-in faces and the persisted values land a frame or two later.
  // OVW-FR-14's "no flash of the built-in face" is therefore satisfied only in
  // the sense that the gap is sub-perceptual on a warm start — it is not
  // structurally impossible. Closing it properly means applying the roles
  // before the web view's first paint (an injected style from the Rust side),
  // which is a change to how the window boots rather than to this hook. The
  // theme has exactly the same shape (`useThemePreference`), so the two would
  // be fixed together.
  useEffect(() => {
    loadAppPreferences().then((prefs) => setFonts(prefs?.fonts));
    // No `.catch`: `loadAppPreferences` resolves to the defaults rather than
    // rejecting, and a catch here would be unreachable code describing
    // behaviour that cannot occur.
  }, []);

  // GLS-FR-20 / SWN-FR-01: the Appearance section that edits the nine values is
  // a child window of its own, so a change made there reaches this window as an
  // announcement. Applying it here is what re-typesets every open Editor tab
  // "immediately" (GLS-FR-20, GLS-FR-13) rather than only the settings window's own text.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences().then((prefs) => {
        if (!cancelled) setFonts(prefs.fonts);
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

  return { fonts, setFonts };
}
