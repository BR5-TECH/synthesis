/**
 * The universal search bar's active query mode (`SCH-search.md` SCH-FR-12 /
 * SCH-FR-13).
 *
 * Exactly one of the three modes is active at any moment, and the choice is
 * user-global: read from `"load app preferences"` when the search input mounts,
 * written through `"save app preferences"` whenever the user changes it
 * (GSS-FR-21). A user who has never chosen one starts in case-insensitive
 * literal.
 *
 * The write is a **patch**, not a replace. `save_app_preferences` writes the
 * record whole (GSS-FR-20), so sending a bare `{ searchQueryMode }` would clear
 * the theme and the main window's full-screen state, which share the record and
 * are edited elsewhere entirely (GLS-FR-14).
 */
import { useEffect, useState } from "react";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "../state/appPreferences";
import type { SearchMode } from "../types";

/** SCH-FR-13: the mode a user who has never chosen one starts in. */
export const DEFAULT_QUERY_MODE: SearchMode = "literal_insensitive";

/**
 * SCH-FR-12: the three toggles, in the order they render inside the input —
 * leading edge, ahead of the text caret.
 *
 * `label` is the affordance's glyph, as the wireframe specifies it; `title`
 * names the mode; `description` says what it actually does to a query. Two
 * glyphs a character apart (`Aa` / `aA`) cannot carry that difference on their
 * own, so the description is what the tooltip leads with.
 */
export const QUERY_MODES: ReadonlyArray<{
  value: SearchMode;
  label: string;
  title: string;
  description: string;
}> = [
  {
    value: "literal_insensitive",
    label: "Aa",
    title: "Case-insensitive",
    description: "Matches the query as plain text, ignoring capitalisation.",
  },
  {
    value: "smart_case",
    label: "aA",
    title: "Smart case",
    description:
      "Plain text, ignoring capitalisation — until you type a capital, then case must match.",
  },
  {
    value: "regex",
    label: ".*",
    title: "Regular expression",
    description: "Matches the query as a regular expression pattern.",
  },
];

export function useSearchQueryMode() {
  const [mode, setMode] = useState<SearchMode>(DEFAULT_QUERY_MODE);

  useEffect(() => {
    let cancelled = false;
    loadAppPreferences()
      .then((prefs) => {
        if (!cancelled) setMode(prefs?.searchQueryMode ?? DEFAULT_QUERY_MODE);
      })
      .catch(() => {
        if (!cancelled) setMode(DEFAULT_QUERY_MODE);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * SCH-FR-12: activating one deactivates the other two; there is no state in
   * which none is active. A persistence failure rolls the applied value back
   * rather than leaving the input showing a choice that will not survive
   * relaunch — the toggles have no error affordance of their own.
   */
  const selectMode = (next: SearchMode) => {
    const previous = mode;
    setMode(next);
    patchAppPreferences({ searchQueryMode: next }).catch(() => {
      setMode(previous);
    });
  };

  return { mode, selectMode };
}
