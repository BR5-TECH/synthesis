/**
 * `EFR-editor-find-replace.md` EFR-FR-ABHF–EFR-FR-DBOW/EFR-FR-GBJT: the find panel's state, as it lives on an
 * artifact's edit session.
 *
 * It belongs to the artifact rather than to the tab showing it, so closing a tab
 * and reopening the artifact in the same session brings the panel back exactly
 * as it was (EFR-FR-GNBZ) — the same shape as the buffer, dirty flag, editing mode
 * and undo history it sits beside. Like all of that, it is held only in memory.
 *
 * The `mode` here is the artifact's own value: it neither reads nor writes the
 * user-global query mode the universal search input persists (EFR-FR-CWUR, per
 * `SCH-search.md` SCH-FR-13), so the two surfaces' modes move independently.
 */
import type { SearchMode } from "../types";

/**
 * Which of the two panels occupies the band (EFR-FR-ABHF). `find` is the query row
 * alone; `replace` adds the replacement row and its two actions (EFR-FR-EWRP).
 */
export type FindForm = "find" | "replace";

export interface FindState {
  /** `null` while neither panel is open — the formatting toolbar has the band. */
  form: FindForm | null;
  query: string;
  /**
   * Retained while the panel is collapsed to Find (EFR-FR-BBKS), so expanding it
   * again restores what the user had typed.
   */
  replacement: string;
  mode: SearchMode;
}

/** EFR-FR-DBOW: an artifact whose panel has not been opened starts here. */
export const DEFAULT_FIND_MODE: SearchMode = "literal_insensitive";

export function createFindState(): FindState {
  return { form: null, query: "", replacement: "", mode: DEFAULT_FIND_MODE };
}

/**
 * Whether the find state holds nothing the user authored.
 *
 * EDT-FR-28: an artifact's retained edit state comes into being with its first
 * edit *or* the first opening of its find panel, so this is what decides whether
 * a record with no edits behind it survives its tab closing. A panel that was
 * opened and closed again still counts while a query or replacement remains —
 * that text is the user's, and EFR-FR-GIPZ, EFR-FR-GBJT, EFR-FR-GNBZ requires it back on reopen — but a
 * panel closed with everything cleared leaves nothing worth retaining.
 */
/**
 * EFR-FR-AYNZ/EFR-FR-BJUY: the form the band takes when the user invokes ⌘F or ⌘R.
 *
 * Each accelerator toggles its own panel closed when that panel is already the
 * one showing, and otherwise switches the band to its form — so ⌘R over an open
 * Find expands it, ⌘F over an open Find & Replace collapses it, and only a
 * second press of the *same* accelerator closes the band. What carries across a
 * switch (the query, the replacement text, the mode) is untouched here, which is
 * exactly why this returns a form rather than a whole state.
 */
export function nextFindForm(
  current: FindForm | null,
  requested: FindForm,
): FindForm | null {
  return current === requested ? null : requested;
}

export function findStateIsPristine(f: FindState): boolean {
  return (
    f.form === null &&
    f.query === "" &&
    f.replacement === "" &&
    f.mode === DEFAULT_FIND_MODE
  );
}
