/**
 * The editor zoom level (`EZM-editor-zoom.md`).
 *
 * One level for every zoom region of the window (EZM-FR-SOZM). The level is a
 * percent. A pinch changes it continuously, so between two gesture events it
 * can hold a fraction; it shows, and it is saved, as a whole percent
 * (EZM-FR-GMSX).
 *
 * The level reaches the regions as the `--editor-zoom` property on the document
 * root. The stylesheet multiplies the Rich Markdown and Source code ladders and
 * the page width by it inside each `.editor-zoom` region (EZM-FR-LRVS), so a
 * change re-typesets every open region and remounts nothing (EZM-FR-MXAB).
 */

import { logDebug, logWarn } from "../logging";
import type { AppPreferences, Tab } from "../types";
import { DASHBOARD_TAB } from "../hooks/shell/tabRecords";
import { patchAppPreferences } from "./appPreferences";

/** EZM-FR-COYK: the zoom steps, in percent. */
export const EDITOR_ZOOM_STEPS: readonly number[] = Object.freeze([
  50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300,
]);
/** EZM-FR-COYK: the smallest level. */
export const EDITOR_ZOOM_MIN = 50;
/** EZM-FR-COYK: the largest level. */
export const EDITOR_ZOOM_MAX = 300;
/** EZM-FR-SOZM: the level with nothing stored. */
export const EDITOR_ZOOM_DEFAULT = 100;
/** EZM-FR-PZSN: a pinch that ends this near to 100% goes to 100%. */
const SNAP_DISTANCE = 3;
/**
 * EZM-FR-VPYL: a Ctrl or Cmd wheel has no end event, so a pinch made of wheel
 * events ends when no wheel event came for this long.
 */
export const WHEEL_PINCH_SETTLE_MS = 250;

/** The root property that the stylesheet reads. */
export const EDITOR_ZOOM_PROPERTY = "--editor-zoom";
/** The attribute that marks the element of a zoom region that scrolls. */
export const ZOOM_SCROLLER_ATTRIBUTE = "data-zoom-scroller";

/** EZM-FR-COYK: keep a level between the smallest and the largest step. */
export function clampZoom(level: number): number {
  if (!Number.isFinite(level)) return EDITOR_ZOOM_DEFAULT;
  return Math.min(EDITOR_ZOOM_MAX, Math.max(EDITOR_ZOOM_MIN, level));
}

/** EZM-FR-GMSX: the whole percent that the level shows as. */
export function displayZoom(level: number): number {
  return Math.round(clampZoom(level));
}

/** EZM-FR-HYQD: a stored level outside the range, or not a number, is 100%. */
export function readStoredZoom(stored: unknown): number {
  if (typeof stored !== "number" || !Number.isFinite(stored)) return EDITOR_ZOOM_DEFAULT;
  if (stored < EDITOR_ZOOM_MIN || stored > EDITOR_ZOOM_MAX) return EDITOR_ZOOM_DEFAULT;
  return Math.round(stored);
}

/**
 * EZM-FR-DMLT: the smallest step above the level. The level counts as the whole
 * percent it shows as, so "100%" from a pinch goes to 110%. At 300% it stays.
 */
export function zoomInFrom(level: number): number {
  const shown = displayZoom(level);
  return EDITOR_ZOOM_STEPS.find((step) => step > shown) ?? EDITOR_ZOOM_MAX;
}

/** EZM-FR-DMLT: the largest step below the level. At 50% it stays. */
export function zoomOutFrom(level: number): number {
  const shown = displayZoom(level);
  for (let i = EDITOR_ZOOM_STEPS.length - 1; i >= 0; i--) {
    if (EDITOR_ZOOM_STEPS[i] < shown) return EDITOR_ZOOM_STEPS[i];
  }
  return EDITOR_ZOOM_MIN;
}

/** EZM-FR-GMSX: the level after a pinch of `factor`. */
export function pinchZoom(level: number, factor: number): number {
  if (!Number.isFinite(factor) || factor <= 0) return clampZoom(level);
  return clampZoom(level * factor);
}

/** EZM-FR-PZSN: the whole-percent level that a pinch ends at. */
export function settleZoom(level: number): number {
  const shown = displayZoom(level);
  return Math.abs(shown - EDITOR_ZOOM_DEFAULT) <= SNAP_DISTANCE ? EDITOR_ZOOM_DEFAULT : shown;
}

/**
 * STB-FR-VRUS / STB-FR-OQTE: the options of the status bar control. These are
 * the steps, and a level between two steps as one more option in its place.
 */
export function zoomOptions(level: number): number[] {
  const shown = displayZoom(level);
  if (EDITOR_ZOOM_STEPS.includes(shown)) return [...EDITOR_ZOOM_STEPS];
  return [...EDITOR_ZOOM_STEPS, shown].sort((a, b) => a - b);
}

/**
 * EZM-FR-JGRP: whether a tab holds a zoom region. Every Diff tab does, also one
 * whose comparison deletes the file: its body shows the original text
 * (DFV-FR-15). A tab with no kind is an Editor tab, but the Dashboard also has
 * no kind.
 */
export function isZoomableTab(tab: Tab | null | undefined): boolean {
  if (!tab || tab.id === DASHBOARD_TAB.id) return false;
  switch (tab.kind ?? "editor") {
    case "editor":
    case "draft":
    case "document":
    case "draftPreview":
    case "openspecChange":
    case "diff":
      return true;
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

let level = EDITOR_ZOOM_DEFAULT;
const listeners = new Set<() => void>();
/** Saves that did not finish yet. While one is out, a stored level is not adopted. */
let pendingWrites = 0;
/** Changes on each reset, so a save from before a reset cannot change the count. */
let generation = 0;
/** True between the start and the end of a pinch. */
let gestureActive = false;
let settleTimer: ReturnType<typeof setTimeout> | null = null;
/** The level that was last saved or read, so a pinch that ends there saves nothing. */
let savedLevel = EDITOR_ZOOM_DEFAULT;
/** The pointer of the last pinch step, so the end of the pinch keeps it (EZM-FR-OXBL). */
let pinchAnchor: ZoomAnchor | undefined;

/** The level now, in percent. */
export function getEditorZoom(): number {
  return level;
}

/** EZM-FR-GMSX: the level as the whole percent that it shows as. */
export function getEditorZoomPercent(): number {
  return displayZoom(level);
}

/** Subscribe to changes of the level. Returns the function that unsubscribes. */
export function subscribeEditorZoom(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** A point of the screen that must stay where it is (EZM-FR-OXBL). */
export interface ZoomAnchor {
  clientX: number;
  clientY: number;
}

interface ScrollerMark {
  el: HTMLElement;
  top: number;
  height: number;
  anchorY: number;
  /** The element under the anchor point, and where in it the point falls (0 to 1). */
  target: { node: Element; fraction: number } | null;
}

/** The element of `scroller`'s document under a point of the screen, if any. */
function elementAt(scroller: HTMLElement, x: number, y: number): Element | null {
  if (typeof document.elementFromPoint !== "function") return null;
  const hit = document.elementFromPoint(x, y);
  if (!hit || hit === scroller || !scroller.contains(hit)) return null;
  return hit;
}

/** EZM-FR-OXBL: note where each visible region scroller is now. */
function markScrollers(anchor: ZoomAnchor | undefined): ScrollerMark[] {
  if (typeof document === "undefined") return [];
  const marks: ScrollerMark[] = [];
  document.querySelectorAll<HTMLElement>(`[${ZOOM_SCROLLER_ATTRIBUTE}]`).forEach((el) => {
    const height = el.scrollHeight;
    if (height <= 0 || height <= el.clientHeight) return;
    const box = el.getBoundingClientRect();
    const inside =
      anchor !== undefined &&
      anchor.clientX >= box.left &&
      anchor.clientX <= box.right &&
      anchor.clientY >= box.top &&
      anchor.clientY <= box.bottom;
    // The pointer for a pinch over this scroller; the top line otherwise.
    const pointX = inside ? anchor.clientX : box.left + box.width / 2;
    const pointY = inside ? anchor.clientY : box.top + 1;
    const anchorY = pointY - box.top;
    const node = elementAt(el, pointX, pointY);
    let target: ScrollerMark["target"] = null;
    if (node) {
      const rect = node.getBoundingClientRect();
      if (rect.height > 0) target = { node, fraction: (pointY - rect.top) / rect.height };
    }
    marks.push({ el, top: el.scrollTop, height, anchorY: inside ? anchorY : 0, target });
  });
  return marks;
}

/**
 * EZM-FR-OXBL: put each scroller back at the same part of its document. Where
 * an element was under the anchor point, the same point of that element goes
 * back under it, which holds also when the lines wrap again. Otherwise the same
 * fraction of the document height stays at the anchor: the pointer for a
 * pinch, the top for every other change.
 */
function restoreScrollers(marks: ScrollerMark[]) {
  for (const { el, top, height, anchorY, target } of marks) {
    if (target && target.node.isConnected) {
      const box = el.getBoundingClientRect();
      const rect = target.node.getBoundingClientRect();
      const now = rect.top + target.fraction * rect.height;
      const want = box.top + (anchorY || 1);
      if (rect.height > 0) {
        el.scrollTop = Math.max(0, el.scrollTop + (now - want));
        continue;
      }
    }
    const next = el.scrollHeight;
    if (next <= 0 || next === height) continue;
    el.scrollTop = Math.max(0, ((top + anchorY) * next) / height - anchorY);
  }
}

/** Show `next` in every region. Saves nothing. */
function apply(next: number, anchor?: ZoomAnchor) {
  const clamped = clampZoom(next);
  if (clamped === level) return;
  const marks = markScrollers(anchor);
  level = clamped;
  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty(EDITOR_ZOOM_PROPERTY, String(clamped / 100));
  }
  restoreScrollers(marks);
  listeners.forEach((listener) => listener());
}

/** EZM-FR-VPYL: save the level as a whole percent. */
function persist(percent: number) {
  const started = generation;
  savedLevel = percent;
  pendingWrites += 1;
  logDebug(["frontend"], "editor zoom save started", { percent });
  void patchAppPreferences({ editorZoomPercent: percent })
    .then(() => {
      logDebug(["frontend"], "editor zoom saved", { percent });
    })
    .catch((error: unknown) => {
      // The level is not known to be stored, so the next pinch that ends at
      // it saves it again.
      if (started === generation && savedLevel === percent) savedLevel = Number.NaN;
      logWarn(["frontend"], "editor zoom save failed", { percent, error: String(error) });
    })
    .finally(() => {
      if (started === generation) pendingWrites -= 1;
    });
}

function clearSettleTimer() {
  if (settleTimer !== null) {
    clearTimeout(settleTimer);
    settleTimer = null;
  }
}

/**
 * EZM-FR-DMLT / EZM-FR-VPYL: set the level from a step, a menu item, or the
 * status bar control. The level shows at once and is saved at once.
 */
export function setEditorZoom(next: number): void {
  clearSettleTimer();
  gestureActive = false;
  const percent = displayZoom(next);
  const changed = percent !== level;
  apply(percent);
  if (changed) persist(percent);
}

/** EZM-FR-DMLT: Zoom In. Does nothing at 300%. */
export function zoomEditorIn(): void {
  setEditorZoom(zoomInFrom(level));
}

/** EZM-FR-DMLT: Zoom Out. Does nothing at 50%. */
export function zoomEditorOut(): void {
  setEditorZoom(zoomOutFrom(level));
}

/** EZM-FR-DMLT: Actual Size. */
export function resetEditorZoom(): void {
  setEditorZoom(EDITOR_ZOOM_DEFAULT);
}

/** EZM-FR-GMSX: a pinch starts. It ends with `endEditorZoomPinch`. */
export function startEditorZoomPinch(): void {
  clearSettleTimer();
  gestureActive = true;
  pinchAnchor = undefined;
}

/**
 * EZM-FR-GMSX / EZM-FR-OXBL: one step of a pinch. The level changes by the
 * factor and the text under the pointer stays under it. Outside a started
 * pinch (a Ctrl or Cmd wheel), the pinch ends when no step came for
 * `WHEEL_PINCH_SETTLE_MS`.
 */
export function pinchEditorZoom(factor: number, anchor?: ZoomAnchor): void {
  pinchAnchor = anchor;
  apply(pinchZoom(level, factor), anchor);
  if (gestureActive) return;
  clearSettleTimer();
  settleTimer = setTimeout(() => {
    settleTimer = null;
    endEditorZoomPinch();
  }, WHEEL_PINCH_SETTLE_MS);
}

/**
 * EZM-FR-PZSN / EZM-FR-VPYL / EZM-FR-OXBL: a pinch ends. The level settles
 * about the last pointer, and it is saved once when it differs from the level
 * that was saved.
 */
export function endEditorZoomPinch(): void {
  clearSettleTimer();
  gestureActive = false;
  const settled = settleZoom(level);
  apply(settled, pinchAnchor);
  pinchAnchor = undefined;
  if (settled !== savedLevel) persist(settled);
}

/**
 * EZM-FR-HYQD: show the level of a record that was read. A record that comes
 * while this window has a pinch or a save in progress is not adopted, so the
 * echo of an earlier save cannot move the level back.
 */
export function adoptStoredEditorZoom(prefs: Pick<AppPreferences, "editorZoomPercent">): void {
  if (gestureActive || settleTimer !== null || pendingWrites > 0) return;
  savedLevel = readStoredZoom(prefs.editorZoomPercent);
  apply(savedLevel);
}

/** Reset the module state between tests. */
export function resetEditorZoomForTest(): void {
  clearSettleTimer();
  level = EDITOR_ZOOM_DEFAULT;
  savedLevel = EDITOR_ZOOM_DEFAULT;
  pinchAnchor = undefined;
  pendingWrites = 0;
  generation += 1;
  gestureActive = false;
  listeners.clear();
  if (typeof document !== "undefined") {
    document.documentElement.style.removeProperty(EDITOR_ZOOM_PROPERTY);
  }
}
