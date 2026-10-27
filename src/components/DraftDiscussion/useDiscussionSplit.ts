/**
 * How a tab is split between its document column and its discussion column
 * (`DDS-draft-discussion.md` DDS-FR-PNXR, DDS-FR-XQMF). The New Artifact tab
 * and the Editor tab use it alike, each under its own column key.
 */
import { useCallback, useEffect, useRef } from "react";

import { cycleRatio, type SplitRatio } from "./ratio";
import {
  discussionColumnState,
  setColumnHidden,
  setColumnRatio,
  useDiscussionColumn,
  type ColumnKey,
} from "../../state/discussionColumn";

/**
 * DDS-FR-PNXR: how this column's tab is split, and the element a drag is
 * measured against.
 *
 * The ratio is held per column in a module store rather than here, because it
 * must survive this tab being backgrounded and returned to — which is exactly
 * what component state does not do.
 */
export function useDiscussionSplit(key: ColumnKey, active = true) {
  const splitRef = useRef<HTMLDivElement | null>(null);
  const view = useDiscussionColumn(key);
  const setRatio = useCallback((ratio: SplitRatio) => setColumnRatio(key, ratio), [key]);
  // DDS-FR-XQMF: hiding the discussion column is not narrowing it, so it is its
  // own control rather than a fourth preset. The splitter and the presets act
  // on two columns that both stand.
  const toggleDiscussion = useCallback(
    () => setColumnHidden(key, !discussionColumnState(key).hidden),
    [key],
  );

  // DDS-FR-PNXR: one accelerator cycles the three presets. Unclaimed elsewhere
  // in the keymap, and deliberately not bound to a modifier the editing surface
  // uses — the caret is in the prose most of the time this tab is open. Only
  // the active tab is mounted, so one listener answers at a time.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "\\" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      // DDS-FR-XQMF: there is no split to cycle while one column is hidden, and
      // a keypress that silently rearranged what the author cannot see would
      // land them on another split the next time they show the column.
      if (discussionColumnState(key).hidden) return;
      e.preventDefault();
      setColumnRatio(key, cycleRatio(discussionColumnState(key).ratio));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [key, active]);

  return { splitRef, view, setRatio, toggleDiscussion };
}
