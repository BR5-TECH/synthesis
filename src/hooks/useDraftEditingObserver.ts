/**
 * Mount the editing-activity observer for the life of the window
 * (`specifications/ui/DFI-draft-information.md` DFI-FR-XKRM, DFI-FR-JDWS).
 *
 * The observer itself is plain state (`../draftEditingObserver`); this is the
 * three window-level signals it reads, bound once at the shell. It runs whether
 * or not the Information modal is open, because the time it measures is time the
 * author spent on a draft rather than time they spent reading about one.
 *
 * A **related surface** of a draft is its New Artifact tab, or a conversation
 * tab for a conversation linked to it (DFI-FR-YAOM). Two related surfaces of one draft hold one
 * interval rather than two, which is why this reduces surfaces to the set of
 * drafts they are about before the observer ever sees them; related surfaces of
 * two different drafts hold one interval each, because the author is at both.
 */
import { useEffect, useMemo, useRef } from "react";

import { DraftEditingObserver } from "../draftEditingObserver";

/** The signals a caller supplies, already reduced to drafts. */
export interface EditingSurfaces {
  /** The draft whose New Artifact tab is the active one, where one is. */
  activeDraftId?: string;
  /**
   * The drafts a conversation tab of theirs is currently on screen for. A tab
   * that is not the active one is not a surface the author is reading.
   */
  conversationDraftIds: readonly string[];
}

export function useDraftEditingObserver(surfaces: EditingSurfaces): void {
  const observer = useMemo(() => new DraftEditingObserver(), []);
  const disposed = useRef(false);

  // The set of drafts a related surface is visible for, as a stable key so a
  // re-render that changed nothing does not settle and reopen every interval.
  const visible = useMemo(() => {
    const ids = new Set(surfaces.conversationDraftIds);
    if (surfaces.activeDraftId) ids.add(surfaces.activeDraftId);
    return [...ids].sort().join(" ");
  }, [surfaces.activeDraftId, surfaces.conversationDraftIds]);

  useEffect(() => {
    observer.setVisibleDrafts(visible.split(" ").filter(Boolean));
  }, [observer, visible]);

  useEffect(() => {
    const onFocus = () => observer.setFocused(true);
    const onBlur = () => observer.setFocused(false);
    // What counts as an interaction: the author doing something with the
    // application. Agent processing is not one — it counts *inside* an open
    // interval rather than opening or extending one (DFI-FR-XKRM).
    const onInteract = () => observer.interacted();
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    window.addEventListener("pointerdown", onInteract, true);
    window.addEventListener("keydown", onInteract, true);
    window.addEventListener("wheel", onInteract, { capture: true, passive: true });
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("pointerdown", onInteract, true);
      window.removeEventListener("keydown", onInteract, true);
      window.removeEventListener("wheel", onInteract, true);
      // DFI-FR-UKFR: an interval this never settled is reported not at all
      // rather than guessed at, so the disposal reports nothing.
      if (!disposed.current) {
        disposed.current = true;
        observer.dispose();
      }
    };
  }, [observer]);
}
