/**
 * EDT-FR-SHYX, EDT-FR-WPGE, EDT-FR-JRDD, EDT-FR-EJBZ: how a diagram overlay
 * follows its fence through each new state of the surface, and how it closes.
 * The diagram editor overlay and the diagram viewer overlay share it.
 */
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import { logDebug } from "../../logging";
import { placeAt, relocate, sourceAt, surfaceHidden, type FencePlace } from "./fence";
import {
  closeDiagramEditor,
  returnFocusToFence,
  watchDiagramSurface,
  type CloseReason,
  type DiagramEditorRequest,
} from "./store";

/** The class of the button that opens each overlay on the fence. */
const OPENER_CLASS = { edit: ".mermaid-edit", view: ".mermaid-view" } as const;

export interface FollowedFence {
  /** Where the fence is now, or null when it is gone. */
  place: RefObject<FencePlace | null>;
  /** The source of the fence as the surface holds it now. */
  source: string;
  setSource: (source: string) => void;
  /** Close the overlay and put the focus where the requirement says. */
  finish: (reason: CloseReason) => void;
}

/**
 * `currentSource` gives the source that the overlay shows, so a fence can be
 * found again after a change that replaced the whole document.
 */
export function useFence(
  request: DiagramEditorRequest,
  currentSource: () => string | null,
): FollowedFence {
  const { view, kind } = request;
  const placeRef = useRef<FencePlace | null>(null);
  const placed = useRef(false);
  if (!placed.current) {
    placed.current = true;
    placeRef.current = placeAt(view.state.doc, request.fence);
  }
  const [source, setSource] = useState(() => sourceAt(view.state.doc, request.fence));
  const shown = useRef(currentSource);
  shown.current = currentSource;

  /**
   * Close from inside the overlay, and put focus where the requirement says:
   * the button of the fence that opened it (EDT-FR-HGAZ, EDT-FR-ELUJ), or the
   * surface when the fence is gone or holds no diagram (EDT-FR-SHYX,
   * EDT-FR-JRDD). An inert surface keeps focus where its own modal puts it
   * (EDT-FR-WPGE).
   */
  const finish = useCallback(
    (reason: CloseReason) => {
      const place = placeRef.current;
      if (reason === "close" || reason === "escape") {
        const dom = place ? view.nodeDOM(place.pos) : null;
        const button =
          dom instanceof HTMLElement ? dom.querySelector<HTMLElement>(OPENER_CLASS[kind]) : null;
        (button ?? request.opener)?.focus();
        // The fence draws its button again when its render settles, so the
        // new button takes the focus too.
        if (place) returnFocusToFence(view, place.pos, kind);
      } else if (
        (reason === "fence-removed" || reason === "write-failed" || reason === "not-a-diagram") &&
        !view.isDestroyed
      ) {
        view.focus();
      }
      closeDiagramEditor(reason);
    },
    [view, kind, request.opener],
  );

  // A fence that is gone, or a surface that takes no input, closes the overlay.
  useEffect(() => {
    if (!placeRef.current) {
      logDebug(["frontend"], "a diagram overlay found no fence", { kind });
      finish("fence-removed");
      return;
    }
    const follow = (change: "update" | "destroy") => {
      if (change === "destroy") {
        closeDiagramEditor("surface-ended");
        return;
      }
      if (!view.editable) {
        finish("surface-inert");
        return;
      }
      const last = placeRef.current;
      const next = last ? relocate(view.state.doc, last, shown.current() ?? "") : null;
      if (!next) {
        finish("fence-removed");
        return;
      }
      placeRef.current = next;
      setSource(sourceAt(view.state.doc, next.pos));
    };
    return watchDiagramSurface(view, follow);
  }, [view, kind, finish]);

  // The rich surface of an Editor tab is hidden while the tab shows its raw
  // text, and an undo can switch the tab there. A fence the author cannot see
  // is a fence the overlay no longer follows.
  useEffect(() => {
    if (typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (surfaceHidden(view)) finish("surface-hidden");
    });
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ["style", "hidden"],
    });
    return () => observer.disconnect();
  }, [view, finish]);

  return { place: placeRef, source, setSource, finish };
}
