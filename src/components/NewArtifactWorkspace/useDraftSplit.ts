/**
 * How the tab of a draft is split between the document column and the
 * discussion column (`DDS-draft-discussion.md` DDS-FR-PNXR, DDS-FR-XQMF).
 * Split out of `index.tsx` to keep that file within the size rule.
 */
import { useCallback, useEffect, useRef } from "react";

import { cycleRatio, type SplitRatio } from "../DraftDiscussion";
import {
  draftDiscussionState,
  setDraftDiscussionHidden,
  setDraftRatio,
  useDraftDiscussion,
} from "../../state/draftDiscussion";

/**
 * DDS-FR-PNXR: how this draft's tab is split, and the element a drag is
 * measured against.
 *
 * The ratio is held per draft in a module store rather than here, because it
 * must survive this tab being backgrounded and returned to — which is exactly
 * what component state does not do.
 */
export function useDraftSplit(draftId: string) {
  const splitRef = useRef<HTMLDivElement | null>(null);
  const view = useDraftDiscussion(draftId);
  const setRatio = useCallback(
    (ratio: SplitRatio) => setDraftRatio(draftId, ratio),
    [draftId],
  );
  // DDS-FR-XQMF: hiding the discussion column is not narrowing it, so it is its
  // own control rather than a fourth preset. The splitter and the presets act
  // on two columns that both stand.
  const toggleDiscussion = useCallback(
    () => setDraftDiscussionHidden(draftId, !draftDiscussionState(draftId).hidden),
    [draftId],
  );

  // DDS-FR-PNXR: one accelerator cycles the three presets. Unclaimed elsewhere
  // in the keymap, and deliberately not bound to a modifier the editing surface
  // uses — the caret is in the prose most of the time this tab is open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "\\" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      // DDS-FR-XQMF: there is no split to cycle while one column is hidden, and
      // a keypress that silently rearranged what the author cannot see would
      // land them on another split the next time they show the column.
      if (draftDiscussionState(draftId).hidden) return;
      e.preventDefault();
      setDraftRatio(draftId, cycleRatio(draftDiscussionState(draftId).ratio));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [draftId]);

  return { splitRef, view, setRatio, toggleDiscussion };
}
