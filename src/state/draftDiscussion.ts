/**
 * What the New Artifact tab's two columns hold that is neither the draft nor
 * the conversation (`DDS-draft-discussion.md`).
 *
 * A module-level store rather than tab state, for the reason DDS-FR-PNXR gives
 * about the ratio and DDS-FR-GKMT gives about the reading position: both are
 * things about *the author's place in a draft*, and both must survive the tab
 * being backgrounded, another tab being read, and this tab being returned to.
 * Tab state is thrown away by the first of those.
 *
 * Only the ratio is persisted. Where the author had scrolled to is about this
 * sitting rather than about the draft, and a session that restored an unread
 * divider from last week would be describing messages the author has read.
 */
import { useSyncExternalStore } from "react";

import {
  DEFAULT_RATIO,
  parseRatio,
  serialiseRatio,
  type SplitRatio,
} from "../components/DraftDiscussion/ratio";
import { logDebug } from "../logging";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
} from "./layoutPreferences";

/**
 * What one draft's two columns hold that the discussion itself does not. The
 * reading position and the unread state are the shared surface's, in
 * `discussionSession.ts`.
 */
export interface DraftDiscussionState {
  ratio: SplitRatio;
  /**
   * DDS-FR-XQMF: the discussion column is hidden, so the document column takes
   * the tab's full width.
   *
   * Separate from the ratio because hiding is not narrowing: the narrowest
   * either column is dragged to is one that can still be read, and a draft
   * being read rather than discussed wants the whole window.
   */
  hidden: boolean;
  /** DDS-FR-CLBK: the collapsed head of a long discussion has been expanded. */
  historyExpanded: boolean;
  /** DCR-FR-12: the hunk the review bar and the action chip are about. */
  focusedHunkId: string | null;
  /**
   * DCR-FR-30: hunk navigation holds focus, so the accelerators that accept and
   * reject are live.
   *
   * False while the caret is in the prose, where the editing surface binds both
   * keys already — an unguarded accelerator would decide a change whenever the
   * author typed a new paragraph.
   */
  hunkFocusHeld: boolean;
}

const INITIAL: DraftDiscussionState = {
  ratio: DEFAULT_RATIO,
  hidden: false,
  historyExpanded: false,
  focusedHunkId: null,
  hunkFocusHeld: false,
};

let states: ReadonlyMap<string, DraftDiscussionState> = new Map();
const listeners = new Set<() => void>();
/** The project the persisted ratios belong to, so a switch cannot reuse them. */
let projectKey: string | null = null;

function emit(): void {
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** One draft's state, defaulted until something has been set on it. */
export function draftDiscussionState(draftId: string): DraftDiscussionState {
  return states.get(draftId) ?? INITIAL;
}

function update(
  draftId: string,
  change: Partial<DraftDiscussionState>,
): DraftDiscussionState {
  const held = draftDiscussionState(draftId);
  const next = { ...held, ...change };
  // A no-op write would still notify, and every subscriber would re-render for
  // a scroll event that changed nothing — which is most scroll events.
  if (
    next.ratio === held.ratio &&
    next.hidden === held.hidden &&
    next.historyExpanded === held.historyExpanded &&
    next.focusedHunkId === held.focusedHunkId &&
    next.hunkFocusHeld === held.hunkFocusHeld
  ) {
    return held;
  }
  const map = new Map(states);
  map.set(draftId, next);
  states = map;
  emit();
  return next;
}

/** DDS-FR-PNXR: how this draft's tab is split, restored per draft. */
export function setDraftRatio(draftId: string, ratio: SplitRatio): void {
  update(draftId, { ratio });
  persist(draftId, { ratio: serialiseRatio(ratio) });
}

/** DDS-FR-XQMF: whether this draft's discussion column is hidden. */
export function setDraftDiscussionHidden(draftId: string, hidden: boolean): void {
  update(draftId, { hidden });
  persist(draftId, { hidden });
  logDebug(["frontend"], "the draft discussion column was hidden or shown", {
    draftId,
    hidden,
  });
}

/**
 * Write one draft's entry into the record, keeping every other draft's.
 *
 * The entry is folded in **inside** the patch rather than before it, against the
 * record as it stands at that moment. A map built here from a copy read earlier
 * would replace the stored map wholesale, so hiding one draft's column would
 * drop the split another draft was left at — the two writes are separate but
 * the field they share is one.
 */
function persist(draftId: string, entry: { ratio?: string; hidden?: boolean }): void {
  if (projectKey === null) return;
  const key = projectKey;
  void patchLayoutPreferences(key, (base) => {
    const patch: {
      draftDiscussionRatios?: Record<string, string>;
      draftDiscussionHidden?: Record<string, boolean>;
    } = {};
    if (entry.ratio !== undefined) {
      patch.draftDiscussionRatios = {
        ...(base.draftDiscussionRatios ?? {}),
        [draftId]: entry.ratio,
      };
    }
    if (entry.hidden !== undefined) {
      patch.draftDiscussionHidden = {
        ...(base.draftDiscussionHidden ?? {}),
        [draftId]: entry.hidden,
      };
    }
    return patch;
  }).catch(() => {
    // A preference that could not be persisted is still the arrangement on
    // screen. The author is not told, because there is nothing they would do
    // about it and what they just chose is exactly what they are looking at.
  });
}

/**
 * DDS-FR-PNXR / DDS-FR-XQMF: restore one project's per-draft view preferences.
 *
 * Called when a project opens. A draft with nothing stored keeps the defaults,
 * and a stored value this build does not recognise is ignored rather than
 * refused — a preset added later must not break a session that reads it.
 */
export async function restoreDraftRatios(key: string): Promise<void> {
  projectKey = key;
  const prefs = await loadLayoutPreferences(key);
  const stored = prefs.draftDiscussionRatios ?? {};
  const storedHidden = prefs.draftDiscussionHidden ?? {};
  const map = new Map(states);
  let changed = false;
  for (const [draftId, raw] of Object.entries(stored)) {
    const ratio = parseRatio(raw);
    if (ratio === null) continue;
    map.set(draftId, { ...(map.get(draftId) ?? INITIAL), ratio });
    changed = true;
  }
  for (const [draftId, hidden] of Object.entries(storedHidden)) {
    if (typeof hidden !== "boolean") continue;
    map.set(draftId, { ...(map.get(draftId) ?? INITIAL), hidden });
    changed = true;
  }
  if (!changed) return;
  states = map;
  logDebug(["frontend"], "restored the draft column splits of a project", {
    drafts: Object.keys(stored).length,
    hidden: Object.keys(storedHidden).length,
  });
  emit();
}

/** DDS-FR-CLBK: show the messages above the viewport, in place. */
export function expandHistory(draftId: string): void {
  update(draftId, { historyExpanded: true });
}

/** DCR-FR-30: the hunk the review is on, and whether its accelerators are live. */
export function setFocusedHunk(
  draftId: string,
  hunkId: string | null,
  held = hunkId !== null,
): void {
  update(draftId, { focusedHunkId: hunkId, hunkFocusHeld: held });
}

/** DCR-FR-30: the caret entered the prose, so the accelerators stand down. */
export function releaseHunkFocus(draftId: string): void {
  update(draftId, { hunkFocusHeld: false });
}

/** One draft's state, in React. */
export function useDraftDiscussion(draftId: string): DraftDiscussionState {
  return useSyncExternalStore(
    subscribe,
    () => draftDiscussionState(draftId),
    () => draftDiscussionState(draftId),
  );
}

/**
 * DCR-FR-33: everything here belongs to the project it was read in, and is
 * discarded when the project closes or the active worktree changes.
 */
export function resetDraftDiscussions(): void {
  states = new Map();
  projectKey = null;
  emit();
}
