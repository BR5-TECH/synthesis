/**
 * EDT-FR-SJGJ: which diagram fence the diagram editor overlay edits now, and
 * the window rules around it.
 *
 * A module store rather than React state, because the Edit button is an editor
 * widget drawn outside React (EDT-FR-ADNU), and the overlay is drawn by the
 * window rather than by the surface. Each window is a webview of its own, so
 * each window has its own store: the main window and the Settings window never
 * share an overlay (EDT-FR-TOVY).
 *
 * The store also relays what happens to an editing surface — each new state,
 * and its end — to the overlay that watches it, because a whole-document undo,
 * a reload, or a reset can remove the fence (EDT-FR-SHYX) with no event the
 * overlay could see otherwise.
 */
import type { EditorView } from "@tiptap/pm/view";

import { logDebug, logInfo } from "../../logging";

export interface DiagramEditorRequest {
  /** A new number for each opening, so each opening starts fresh (EDT-FR-NKGX). */
  id: number;
  /** The surface that holds the fence. */
  view: EditorView;
  /** The start position of the fence when the overlay opened. */
  fence: number;
  /** The Edit button that opened the overlay. */
  opener: HTMLElement | null;
}

/** Why the overlay closed. Only for the log. */
export type CloseReason =
  | "close"
  | "escape"
  | "other-overlay"
  | "window"
  | "fence-removed"
  | "write-failed"
  | "surface-ended"
  | "surface-hidden"
  | "surface-inert";

type Listener = () => void;

let request: DiagramEditorRequest | null = null;
let nextId = 1;
const listeners = new Set<Listener>();
/**
 * SNV-FR-56: what the window does before the overlay opens. The main window
 * closes its other floating overlays here. The Settings window sets nothing.
 */
let opening: (() => void) | null = null;

function emit(): void {
  for (const listener of [...listeners]) listener();
}

export function subscribeDiagramEditor(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function diagramEditorRequest(): DiagramEditorRequest | null {
  return request;
}

export function isDiagramEditorOpen(): boolean {
  return request !== null;
}

/** SNV-FR-56: set what the window closes before the overlay opens. */
export function setDiagramEditorOpening(next: (() => void) | null): void {
  opening = next;
}

/** EDT-FR-ADNU: open the overlay on the fence at `fence` in `view`. */
export function openDiagramEditor(
  view: EditorView,
  fence: number,
  opener: HTMLElement | null,
): void {
  focusReturn = null;
  // EDT-FR-TOVY: opening closes every other overlay of the main window. That
  // also closes an overlay that is open on another fence.
  opening?.();
  request = { id: nextId++, view, fence, opener };
  logInfo(["frontend"], "diagram editor opened", { fence });
  emit();
}

/** EDT-FR-SRXG: close the overlay. A close discards nothing. */
export function closeDiagramEditor(reason: CloseReason): void {
  if (!request) return;
  request = null;
  logInfo(["frontend"], "diagram editor closed", { reason });
  emit();
}

/**
 * EDT-FR-HGAZ: the fence whose Edit button takes the focus back after the
 * overlay closed. The fence can draw its button again after the close, when a
 * render settles; the new button then takes the focus that the old one held.
 */
let focusReturn: { view: EditorView; fence: number } | null = null;

export function returnFocusToFence(view: EditorView, fence: number): void {
  focusReturn = { view, fence };
  // The next press of the author ends it, so the focus is never taken later
  // from where the author put it.
  const end = () => {
    focusReturn = null;
    document.removeEventListener("pointerdown", end, true);
    document.removeEventListener("keydown", end, true);
  };
  document.addEventListener("pointerdown", end, true);
  document.addEventListener("keydown", end, true);
}

/** Whether a new Edit button of `fence` in `view` takes the focus back. */
export function takesFocusBack(view: EditorView, fence: number): boolean {
  if (!focusReturn || focusReturn.view !== view || focusReturn.fence !== fence) return false;
  const active = document.activeElement;
  const resting = active === null || active === document.body || !active.isConnected;
  if (resting) focusReturn = null;
  return resting;
}

/** What a watched surface reports. */
export type SurfaceChange = "update" | "destroy";

const watchers = new WeakMap<EditorView, Set<(change: SurfaceChange) => void>>();

/** EDT-FR-SHYX: call `listener` on each new state of `view`, and at its end. */
export function watchDiagramSurface(
  view: EditorView,
  listener: (change: SurfaceChange) => void,
): () => void {
  let set = watchers.get(view);
  if (!set) {
    set = new Set();
    watchers.set(view, set);
  }
  set.add(listener);
  return () => {
    set.delete(listener);
  };
}

/** Called by the diagram plugin of each surface. */
export function notifyDiagramSurface(view: EditorView, change: SurfaceChange): void {
  const set = watchers.get(view);
  if (!set || set.size === 0) return;
  if (change === "destroy") {
    logDebug(["frontend"], "the surface of the diagram editor ended");
    if (focusReturn?.view === view) focusReturn = null;
  }
  for (const listener of [...set]) listener(change);
}

/** Test seam: forgets the request, the listeners, and the window rule. */
export function resetDiagramEditorForTest(): void {
  request = null;
  listeners.clear();
  opening = null;
  focusReturn = null;
  nextId = 1;
}
