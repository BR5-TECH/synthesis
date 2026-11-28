/**
 * The page width of the document page (`EPW-editor-page-width.md`).
 *
 * One width for every document page of the window (EPW-FR-QGUZ, EPW-FR-NSNH),
 * in whole CSS pixels at 100% editor zoom. It reaches the pages as the
 * `--doc-page-base-width` property on the document root. The stylesheet
 * multiplies it by the zoom level (EPW-FR-GNNZ) and caps each page at the
 * width of its column (EPW-FR-KCZU), so a change re-lays every open page and
 * remounts nothing (EPW-FR-XOMR).
 */

import { logDebug, logInfo, logWarn } from "../logging";
import type { AppPreferences } from "../types";
import { patchAppPreferences } from "./appPreferences";

/** EPW-FR-QGUZ: the smallest page width. */
export const PAGE_WIDTH_MIN = 560;
/** EPW-FR-QGUZ: the largest page width. */
export const PAGE_WIDTH_MAX = 1800;
/** EPW-FR-QGUZ: the page width with nothing stored. */
export const PAGE_WIDTH_DEFAULT = 820;
/** EPW-FR-IUHL: the step of an arrow key, and of Shift with an arrow key. */
export const PAGE_WIDTH_STEP = 20;
export const PAGE_WIDTH_LARGE_STEP = 100;

/** The root property that the stylesheet reads. */
export const PAGE_WIDTH_PROPERTY = "--doc-page-base-width";

/** EPW-FR-QGUZ / EPW-FR-AOVQ: a whole width between the smallest and the largest. */
export function clampPageWidth(width: number): number {
  if (!Number.isFinite(width)) return PAGE_WIDTH_DEFAULT;
  return Math.min(PAGE_WIDTH_MAX, Math.max(PAGE_WIDTH_MIN, Math.round(width)));
}

/** EPW-FR-ZCBT / GSS-FR-UYXH: a stored width out of range, or not whole, is 820. */
export function readStoredPageWidth(stored: unknown): number {
  if (typeof stored !== "number" || !Number.isInteger(stored)) return PAGE_WIDTH_DEFAULT;
  if (stored < PAGE_WIDTH_MIN || stored > PAGE_WIDTH_MAX) return PAGE_WIDTH_DEFAULT;
  return stored;
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

let width = PAGE_WIDTH_DEFAULT;
/** The width that was last saved or read, so a drag that ends there saves nothing. */
let savedWidth = PAGE_WIDTH_DEFAULT;
const listeners = new Set<() => void>();
/** Saves that did not finish yet. While one is out, a stored width is not adopted. */
let pendingWrites = 0;
/** Changes on each reset, so a save from before a reset cannot change the count. */
let generation = 0;
/** True between the start and the end of a drag. */
let dragActive = false;
/** The newest record that came during a drag, adopted when the drag saves nothing. */
let heldRecord: Pick<AppPreferences, "editorPageWidthPx"> | null = null;

/** The page width now, in pixels at 100% zoom. */
export function getEditorPageWidth(): number {
  return width;
}

/** Subscribe to changes of the width. Returns the function that unsubscribes. */
export function subscribeEditorPageWidth(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Show `next` on every page. Saves nothing. */
function apply(next: number) {
  const clamped = clampPageWidth(next);
  if (clamped === width) return;
  width = clamped;
  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty(PAGE_WIDTH_PROPERTY, `${clamped}px`);
  }
  listeners.forEach((listener) => listener());
}

/**
 * EPW-FR-JNCM / EPW-FR-LSZF: save the width. A refused save keeps the width on
 * screen and writes a warning; the returned promise rejects for a caller that
 * shows the refusal itself (GLS-FR-DCMT).
 */
function persist(px: number): Promise<void> {
  const started = generation;
  savedWidth = px;
  pendingWrites += 1;
  return patchAppPreferences({ editorPageWidthPx: px })
    .then(() => {
      logInfo(["frontend"], "page width saved", { px });
    })
    .catch((error: unknown) => {
      // The width is not known to be stored, so the next commit of it saves it again.
      if (started === generation && savedWidth === px) savedWidth = Number.NaN;
      logWarn(["frontend"], "page width save failed", { px, error: String(error) });
      throw error;
    })
    .finally(() => {
      if (started === generation) pendingWrites -= 1;
    });
}

/** EPW-FR-AOVQ: a drag starts. It ends with `commitEditorPageWidth` or `endEditorPageWidthDrag`. */
export function startEditorPageWidthDrag(): void {
  dragActive = true;
}

/**
 * EPW-FR-JNCM / EPW-FR-ZCBT: a drag ends with no change of the width, as a
 * click does. Nothing is saved, and a record that another window saved during
 * the drag is shown now.
 */
export function endEditorPageWidthDrag(): void {
  dragActive = false;
  adoptHeldRecord();
}

function adoptHeldRecord() {
  const held = heldRecord;
  heldRecord = null;
  if (held) adoptStoredEditorPageWidth(held);
}

/** EPW-FR-AOVQ: one step of a drag. The pages show the width; nothing is saved. */
export function previewEditorPageWidth(next: number): void {
  apply(next);
}

/**
 * EPW-FR-JNCM: set the width and save it once, when it differs from the width
 * that was saved. A drag ends here, and a key, a double-click, and the
 * Settings field come here directly. The promise rejects when the save fails.
 */
export function commitEditorPageWidth(next: number): Promise<void> {
  dragActive = false;
  const px = clampPageWidth(next);
  apply(px);
  if (px === savedWidth) {
    adoptHeldRecord();
    return Promise.resolve();
  }
  // This window's own save is newer than a record that came during the drag.
  heldRecord = null;
  return persist(px);
}

/**
 * GLS-FR-DCMT: after a refused save from the Settings field, show again the
 * width that is on disk, and know it as the saved width.
 */
export function restoreEditorPageWidth(stored: number): void {
  savedWidth = clampPageWidth(stored);
  apply(savedWidth);
}

/**
 * EPW-FR-ZCBT: show the width of a record that was read. A record that comes
 * while this window has a drag or a save in progress is not adopted, so the
 * echo of an earlier save cannot move the width back.
 */
export function adoptStoredEditorPageWidth(prefs: Pick<AppPreferences, "editorPageWidthPx">): void {
  if (dragActive) {
    heldRecord = prefs;
    logDebug(["frontend"], "page width record held during drag", {
      stored: prefs.editorPageWidthPx ?? null,
    });
    return;
  }
  if (pendingWrites > 0) return;
  const stored = prefs.editorPageWidthPx;
  savedWidth = readStoredPageWidth(stored);
  if (stored !== undefined && stored !== savedWidth) {
    logDebug(["frontend"], "page width stored out of range", { stored, applied: savedWidth });
  }
  if (savedWidth !== width) {
    logDebug(["frontend"], "page width adopted", { from: width, to: savedWidth });
  }
  apply(savedWidth);
}

/** Reset the module state between tests. */
export function resetEditorPageWidthForTest(): void {
  width = PAGE_WIDTH_DEFAULT;
  savedWidth = PAGE_WIDTH_DEFAULT;
  pendingWrites = 0;
  generation += 1;
  dragActive = false;
  heldRecord = null;
  listeners.clear();
  if (typeof document !== "undefined") {
    document.documentElement.style.removeProperty(PAGE_WIDTH_PROPERTY);
  }
}
