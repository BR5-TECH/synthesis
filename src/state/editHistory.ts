/**
 * EDT-FR-22–EDT-FR-24: one unified undo/redo history for an artifact.
 *
 * Every user edit is recorded as a whole-document snapshot tagged with the mode
 * and surface it was made on, so a single stack spans the WYSIWYG body, the
 * frontmatter region, and the raw-text source. Because only user edits are
 * recorded and index 0 is always the artifact as loaded, undo can never reach a
 * state the user did not author — in particular it can never empty a document
 * that loaded with content (EDT-FR-23).
 *
 * Plain data + pure functions rather than a hook: the history belongs to the
 * artifact's edit session (`editSessions.ts`), which outlives the Editor
 * component that displays it (EDT-FR-28), so it cannot live in component state.
 */

/** EDT-FR-17: the two editing surfaces the mode toggle switches between. */
export type EditMode = "wysiwyg" | "text";

/**
 * EDT-FR-22: the editing surface a recorded step was authored on. Undo carries
 * it back so the reversal is revealed where it happened (EDT-FR-25) — notably a
 * collapsed frontmatter region is expanded before its edit is undone.
 */
export type EditSurface = "body" | "frontmatter" | "source";

/**
 * One position in the history: the whole document as it stood after the edit,
 * plus where that edit was made. The floor (index 0) is the artifact as loaded
 * and carries no mode/surface, because no user edit produced it.
 */
export interface HistoryStep {
  doc: string;
  mode: EditMode | null;
  surface: EditSurface | null;
}

/**
 * The result of traversing one step: the document to restore, plus the mode and
 * surface of the step that was traversed (EDT-FR-25).
 */
export interface HistoryMove {
  doc: string;
  mode: EditMode | null;
  surface: EditSurface | null;
}

export interface EditHistory {
  steps: HistoryStep[];
  index: number;
  /** The open burst being coalesced into the top step, if any. */
  burst: { startedAt: number; lastAt: number; surface: EditSurface } | null;
}

/**
 * Consecutive edits on one surface fold into a single step while the burst is
 * younger than this (ms), so a run of keystrokes undoes as a unit and the stack
 * does not grow one whole-document snapshot per character. The cap is on the
 * burst's total age, not its idle gap, so continuous typing still seals steps at
 * this cadence. Exported for tests, which drive the clock explicitly.
 */
export const HISTORY_COALESCE_MS = 500;

export function createHistory(doc = ""): EditHistory {
  return { steps: [{ doc, mode: null, surface: null }], index: 0, burst: null };
}

/**
 * EDT-FR-24: make `doc` the floor and drop everything else. Called when a load
 * is adopted (first open, or Load-from-filesystem), so no undo can reach across
 * a load boundary. A reopen that merely revalidates the checksum does NOT call
 * this — the floor stays where the editing session left it (EDT-FR-29).
 */
export function resetHistory(h: EditHistory, doc: string): void {
  h.steps = [{ doc, mode: null, surface: null }];
  h.index = 0;
  h.burst = null;
}

/**
 * EDT-FR-22/EDT-FR-23: record a user edit. Programmatic writes must not call
 * this — only edits the user authored occupy a position in the history.
 */
export function recordEdit(
  h: EditHistory,
  doc: string,
  mode: EditMode,
  surface: EditSurface,
): void {
  const i = h.index;
  // An edit that leaves the bytes unchanged is not a step.
  if (doc === h.steps[i].doc) return;

  const now = Date.now();
  const burst = h.burst;
  const coalesce =
    i > 0 && // never fold an edit into the floor
    burst !== null &&
    burst.surface === surface &&
    now - burst.lastAt < HISTORY_COALESCE_MS &&
    now - burst.startedAt < HISTORY_COALESCE_MS;

  if (coalesce && burst) {
    h.steps[i] = { doc, mode, surface };
    burst.lastAt = now;
    return;
  }
  // A fresh step invalidates any redo tail.
  h.steps.length = i + 1;
  h.steps.push({ doc, mode, surface });
  h.index = i + 1;
  h.burst = { startedAt: now, lastAt: now, surface };
}

/** EDT-FR-24: the previous state, or null at the floor (undo is a no-op). */
export function undoStep(h: EditHistory): HistoryMove | null {
  const i = h.index;
  if (i <= 0) return null; // EDT-FR-24: the floor is the oldest reachable state
  const reversed = h.steps[i];
  h.index = i - 1;
  h.burst = null; // traversing seals the open burst
  return {
    doc: h.steps[i - 1].doc,
    mode: reversed.mode,
    surface: reversed.surface,
  };
}

/** The next state, or null when nothing has been undone. */
export function redoStep(h: EditHistory): HistoryMove | null {
  const i = h.index;
  if (i >= h.steps.length - 1) return null;
  const applied = h.steps[i + 1];
  h.index = i + 1;
  h.burst = null;
  return { doc: applied.doc, mode: applied.mode, surface: applied.surface };
}

/**
 * EFR-FR-FYNZ: close the open coalescing burst, so the next edit starts a step of
 * its own instead of folding into the previous one.
 *
 * A replacement is sealed on both sides: a Replace All must be exactly one step
 * however many occurrences it rewrote, which means neither the typing that
 * preceded it nor a Replace that follows within the coalescing window may join
 * it. Typing has no such requirement, which is why sealing is explicit here
 * rather than a property of `recordEdit`.
 */
export function sealBurst(h: EditHistory): void {
  h.burst = null;
}

/** True when the user has made at least one edit since the floor was set. */
export function hasEdits(h: EditHistory): boolean {
  return h.steps.length > 1;
}
