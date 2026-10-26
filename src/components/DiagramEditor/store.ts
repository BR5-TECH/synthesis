/**
 * EDT-FR-SJGJ, EDT-FR-KMSI: which diagram fence the diagram editor overlay
 * edits, or the diagram viewer overlay shows, now, and the window rules around
 * them. The two overlays are modal, so at most one of them is open.
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

/** The two diagram overlays: the editor (EDT-FR-SJGJ) and the viewer (EDT-FR-KMSI). */
export type DiagramOverlayKind = "edit" | "view";

export interface DiagramEditorRequest {
  /** A new number for each opening, so each opening starts fresh (EDT-FR-NKGX). */
  id: number;
  /** Which overlay is open. */
  kind: DiagramOverlayKind;
  /** The surface that holds the fence. */
  view: EditorView;
  /** The start position of the fence when the overlay opened. */
  fence: number;
  /** The Edit button or the View button that opened the overlay. */
  opener: HTMLElement | null;
}

/** Why the overlay closed. Only for the log. */
export type CloseReason =
  | "close"
  | "escape"
  | "other-overlay"
  | "window"
  | "fence-removed"
  | "not-a-diagram"
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

function openOverlay(
  kind: DiagramOverlayKind,
  view: EditorView,
  fence: number,
  opener: HTMLElement | null,
): void {
  focusReturn = null;
  // EDT-FR-TOVY: opening closes every other overlay of the main window. That
  // also closes a diagram overlay that is open on another fence.
  opening?.();
  request = { id: nextId++, kind, view, fence, opener };
  logInfo(["frontend"], kind === "edit" ? "diagram editor opened" : "diagram viewer opened", {
    fence,
  });
  emit();
}

/** EDT-FR-ADNU: open the editor overlay on the fence at `fence` in `view`. */
export function openDiagramEditor(
  view: EditorView,
  fence: number,
  opener: HTMLElement | null,
): void {
  openOverlay("edit", view, fence, opener);
}

/** EDT-FR-YVAI: open the viewer overlay on the fence at `fence` in `view`. */
export function openDiagramViewer(
  view: EditorView,
  fence: number,
  opener: HTMLElement | null,
): void {
  openOverlay("view", view, fence, opener);
}

/**
 * EDT-FR-SRXG, EDT-FR-ELUJ: close the diagram overlay that is open, the
 * editor or the viewer. A close discards nothing.
 */
export function closeDiagramEditor(reason: CloseReason): void {
  if (!request) return;
  const { kind } = request;
  request = null;
  logInfo(["frontend"], kind === "edit" ? "diagram editor closed" : "diagram viewer closed", {
    reason,
  });
  emit();
}

/**
 * EDT-FR-HGAZ, EDT-FR-ELUJ: the fence whose Edit button or View button takes
 * the focus back after its overlay closed. The fence can draw its button again
 * after the close, when a render settles; the new button then takes the focus
 * that the old one held.
 */
let focusReturn: { view: EditorView; fence: number; kind: DiagramOverlayKind } | null = null;

export function returnFocusToFence(
  view: EditorView,
  fence: number,
  kind: DiagramOverlayKind,
): void {
  focusReturn = { view, fence, kind };
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

/** Whether a new button of `kind` on `fence` in `view` takes the focus back. */
export function takesFocusBack(
  view: EditorView,
  fence: number,
  kind: DiagramOverlayKind,
): boolean {
  if (
    !focusReturn ||
    focusReturn.view !== view ||
    focusReturn.fence !== fence ||
    focusReturn.kind !== kind
  ) {
    return false;
  }
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
