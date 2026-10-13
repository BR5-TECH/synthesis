/**
 * The draft tab's History rail state (`NAW-new-artifact.md` NAW-FR-07 …
 * NAW-FR-10, NAW-FR-36 … NAW-FR-42). Split out of `index.tsx` to keep that
 * file within the size rule.
 *
 * This hook holds the rail's list, the version being read, and the return to
 * the live prompt. It renders nothing.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import * as api from "../../api";
import type {
  DraftHistoryChanged,
  DraftHistoryEntry,
  DraftHistoryList,
  DraftRecord,
} from "../../types";
import { DRAFT_HISTORY_CHANGED } from "../../events";
import type { EditMode } from "../../state/editHistory";
import { isRefusal } from "./labels";

/** NAW-FR-41 / DHS-FR-21: a history that could not be reconciled. */
const ERR_HISTORY_RECOVERY_FAILED = "history_recovery_failed";

export function useDraftHistory(draftId: string, record: DraftRecord | null) {
  /**
   * NAW-FR-40: the rail's list, loaded without loading anything it holds.
   *
   * Fetched when the tab mounts rather than when the rail is shown, because the
   * toggle carries the version count whether or not the rail is open
   * (NAW-FR-08) — and `"list draft history"` reads no snapshot, so an unopened
   * rail still costs one directory walk and nothing more.
   */
  const [history, setHistory] = useState<DraftHistoryList | null>(null);
  /** NAW-FR-41: a history that could not be reconciled, with its retry. */
  const [historyError, setHistoryError] = useState<string | null>(null);
  /** NAW-FR-09: the version being read, or null for the live prompt. */
  const [viewing, setViewing] = useState<DraftHistoryEntry | null>(null);
  const [viewText, setViewText] = useState<string | null>(null);
  /** NAW-FR-41: a version whose text could not be loaded. */
  const [viewError, setViewError] = useState<string | null>(null);
  /**
   * NAW-FR-09: which of the two modes the reading is in, once the author has
   * chosen one.
   *
   * Null until they do, which is what lets a version open in whichever mode the
   * live prompt is in — a version read the way the prompt is being read. Held
   * apart from the live prompt's own mode rather than shared with it, because
   * returning restores the live prompt's editor state exactly as it was left
   * (NAW-FR-10) and a toggle made over a reading is not an edit to that state.
   */
  const [viewMode, setViewMode] = useState<EditMode | null>(null);

  /**
   * NAW-FR-10: the way back to the live prompt is always at hand while a version
   * is showing — the rail's live-prompt row, the strip's own control, Escape
   * from anywhere in the reading, and hiding the rail.
   */
  /**
   * NAW-FR-10 / non-functional: the rail's live-prompt row, so a return made
   * from inside the reading puts the author back on it rather than dropping
   * focus to the document — where the next Tab restarts at the top of the tab
   * and a screen-reader user loses their place.
   */
  const liveRowRef = useRef<HTMLButtonElement | null>(null);

  /**
   * NAW-FR-42: leave the reading, keeping the mode the author chose for it.
   * What an acceptance that rewrote the prompt calls.
   */
  const dropReading = useCallback(() => {
    setViewing(null);
    setViewText(null);
    setViewError(null);
  }, []);

  const backToLive = useCallback(() => {
    dropReading();
    // NAW-FR-09: a reading is seeded from the live prompt's own mode, so the
    // author's choice holds for as long as they stay in the reading and ends
    // with it. Kept across versions selected one after another — that is one
    // reading — and dropped here, so the next one opens as the prompt is being
    // read rather than as some earlier reading was left.
    setViewMode(null);
  }, [dropReading]);

  /**
   * The return as the author makes it — from the strip, from Escape, or from a
   * failed reading — which lands focus on the row that says what the prompt now
   * is. Distinct from [`backToLive`] itself, which is also what an acceptance
   * landing elsewhere calls (NAW-FR-42): that one must not move focus, the
   * author not having asked for anything.
   */
  const returnToLive = useCallback(() => {
    backToLive();
    // After the reading has gone, so the row is the thing being focused rather
    // than a control inside a surface that is about to unmount.
    requestAnimationFrame(() => liveRowRef.current?.focus());
  }, [backToLive]);

  /**
   * NAW-FR-40: one `"list draft history"` per tab, and one more for each
   * `"draft history changed"` that could not be applied from its payload.
   *
   * A failure to reconcile is rendered rather than swallowed (NAW-FR-41): the
   * prompt is presented as neither current nor stale until it resolves, which is
   * why `history` is left null rather than being given an empty list.
   */
  const loadHistory = useCallback(async () => {
    try {
      const list = await api.listDraftHistory(draftId);
      setHistory(list);
      setHistoryError(null);
    } catch (e) {
      setHistory(null);
      setHistoryError(
        isRefusal(e, ERR_HISTORY_RECOVERY_FAILED)
          ? "This draft's history could not be recovered, so the prompt cannot be shown as current or stale yet."
          : String(e),
      );
    }
  }, [draftId]);

  useEffect(() => {
    // Only once the record has landed: a draft that does not open as a draft at
    // all (NAW-FR-41) has no history to list, and asking for one would put a
    // second refusal in front of the author for the same fact.
    if (record === null) return;
    void loadHistory();
  }, [loadHistory, record]);

  /**
   * NAW-FR-40: the rail follows `"draft history changed"` while the tab is open,
   * so a version an acceptance produced appears without the rail re-listing.
   *
   * The event carries the entry itself, but the *live prompt's* digest moves
   * with it — and that is what the modified marker is computed from (NAW-FR-37)
   * — so the list is re-read rather than patched. It is one directory walk and
   * no snapshot (DHS-FR-10), which is what makes re-reading the cheaper answer
   * than holding a second copy that could disagree.
   */
  useEffect(() => {
    const unlisten = listen<DraftHistoryChanged>(DRAFT_HISTORY_CHANGED, (event) => {
      if (event.payload?.draftId !== draftId) return;
      // NAW-FR-42: a version an acceptance produced puts the tab back on the
      // live prompt if one was being read, so the author sees what landed
      // rather than the reading they had open. A reconciliation that *withdrew*
      // an entry (a null payload) does the same, that entry no longer being a
      // version of the prompt at all (DHS-FR-22).
      backToLive();
      void loadHistory();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [draftId, loadHistory, backToLive]);

  /**
   * NAW-FR-09 / NAW-FR-40: read one version. `"load draft history entry"` is
   * invoked when a version is actually selected and at no other moment.
   *
   * It replaces no buffer: the live prompt's text, its dirty state, its editing
   * mode, its undo history and its find-panel state are all exactly as they were
   * left, and are restored the moment the author returns to it (NAW-FR-10).
   */
  const readVersion = useCallback(async (entry: DraftHistoryEntry) => {
    setViewing(entry);
    setViewText(null);
    setViewError(null);
    try {
      const loaded = await api.loadDraftHistoryEntry(entry.id);
      setViewText(loaded.content);
    } catch (e) {
      // NAW-FR-41: rendered where the author is looking, leaving the live
      // prompt and the editor untouched, with a retry and the return beside
      // it. The rail keeps its other rows and every one stays selectable.
      setViewError(
        isRefusal(e, "snapshot_corrupt")
          ? "Its stored snapshot did not match its checksum."
          : String(e),
      );
    }
  }, []);

  return {
    history,
    historyError,
    viewing,
    viewText,
    viewError,
    viewMode,
    setViewMode,
    liveRowRef,
    dropReading,
    backToLive,
    returnToLive,
    loadHistory,
    readVersion,
  };
}
