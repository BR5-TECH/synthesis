/**
 * How a tab is split between its document column and its discussion column
 * (`DDS-draft-discussion.md` DDS-FR-PNXR, DDS-FR-XQMF). The New Artifact tab
 * keeps one split per draft, and the Editor tab one per file
 * (`CMT-comments.md` CMT-FR-30).
 *
 * A module-level store rather than tab state: the split is about *the author's
 * place in a draft or a file*, and it must survive the tab being backgrounded,
 * another tab being read, and this tab being returned to. Tab state is thrown
 * away by the first of those.
 *
 * The ratio and the hidden state are persisted. Whether the head of a long
 * discussion was expanded is about this sitting, and is not.
 */
import { useSyncExternalStore } from "react";

import {
  DEFAULT_RATIO,
  parseRatio,
  serialiseRatio,
  type SplitRatio,
} from "../components/DraftDiscussion/ratio";
import { logDebug, logWarn } from "../logging";
import type { LayoutPreferences } from "../types";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
} from "./layoutPreferences";

/** What one column holds that the discussion itself does not. */
export interface DiscussionColumnState {
  ratio: SplitRatio;
  /**
   * DDS-FR-XQMF: the discussion column is hidden, so the document column takes
   * the tab's full width.
   *
   * Separate from the ratio because hiding is not narrowing: the narrowest
   * either column is dragged to is one that can still be read, and a document
   * being read rather than discussed wants the whole window.
   */
  hidden: boolean;
  /** DDS-FR-CLBK: the collapsed head of a long discussion has been expanded. */
  historyExpanded: boolean;
}

/**
 * Which column a state belongs to. A draft and a file are two kinds of key, so
 * the kind is part of the key and a draft id never names a file's column.
 */
export type ColumnKey = `draft:${string}` | `file:${string}`;

export function draftColumn(draftId: string): ColumnKey {
  return `draft:${draftId}`;
}

/** CMT-FR-30: a file's column is keyed by its project-relative path. */
export function fileColumn(path: string): ColumnKey {
  return `file:${path}`;
}

const INITIAL: DiscussionColumnState = {
  ratio: DEFAULT_RATIO,
  hidden: false,
  historyExpanded: false,
};

let states: ReadonlyMap<ColumnKey, DiscussionColumnState> = new Map();
const listeners = new Set<() => void>();
/** The project the persisted splits belong to, so a switch cannot reuse them. */
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

/** One column's state, defaulted until something has been set on it. */
export function discussionColumnState(key: ColumnKey): DiscussionColumnState {
  return states.get(key) ?? INITIAL;
}

/** Write the change, and say whether it changed anything. */
function update(key: ColumnKey, change: Partial<DiscussionColumnState>): boolean {
  const held = discussionColumnState(key);
  const next = { ...held, ...change };
  // A no-op write would still notify, and every subscriber would re-render for
  // nothing.
  if (
    next.ratio === held.ratio &&
    next.hidden === held.hidden &&
    next.historyExpanded === held.historyExpanded
  ) {
    return false;
  }
  const map = new Map(states);
  map.set(key, next);
  states = map;
  emit();
  return true;
}

/** DDS-FR-PNXR: how this column's tab is split, restored per draft and per file. */
export function setColumnRatio(key: ColumnKey, ratio: SplitRatio): void {
  // A write that changes nothing is not stored: a reveal that finds the column
  // already shown must not touch the layout record.
  if (!update(key, { ratio })) return;
  persist(key, { ratio: serialiseRatio(ratio) });
  logDebug(["frontend"], "the split of a discussion column changed", {
    column: key,
    ratio: serialiseRatio(ratio),
  });
}

/** DDS-FR-XQMF: whether this discussion column is hidden. */
export function setColumnHidden(key: ColumnKey, hidden: boolean): void {
  if (!update(key, { hidden })) return;
  persist(key, { hidden });
  logDebug(["frontend"], "a discussion column was hidden or shown", {
    column: key,
    hidden,
  });
}

/** DDS-FR-CLBK: show the messages above the viewport, in place. */
export function expandColumnHistory(key: ColumnKey): void {
  update(key, { historyExpanded: true });
}

/** Where a key's entries are stored, and the id they are stored under. */
function storedAs(key: ColumnKey): {
  id: string;
  ratios: "draftDiscussionRatios" | "fileDiscussionRatios";
  hidden: "draftDiscussionHidden" | "fileDiscussionHidden";
} {
  if (key.startsWith("draft:")) {
    return {
      id: key.slice("draft:".length),
      ratios: "draftDiscussionRatios",
      hidden: "draftDiscussionHidden",
    };
  }
  return {
    id: key.slice("file:".length),
    ratios: "fileDiscussionRatios",
    hidden: "fileDiscussionHidden",
  };
}

/**
 * Write one column's entry into the record, keeping every other column's.
 *
 * The entry is folded in **inside** the patch rather than before it, against
 * the record as it stands at that moment. A map built here from a copy read
 * earlier would replace the stored map wholesale, so hiding one column would
 * drop the split another column was left at — the two writes are separate but
 * the field they share is one.
 */
function persist(key: ColumnKey, entry: { ratio?: string; hidden?: boolean }): void {
  if (projectKey === null) return;
  const project = projectKey;
  const at = storedAs(key);
  void patchLayoutPreferences(project, (base) => {
    const patch: Partial<LayoutPreferences> = {};
    if (entry.ratio !== undefined) {
      patch[at.ratios] = { ...(base[at.ratios] ?? {}), [at.id]: entry.ratio };
    }
    if (entry.hidden !== undefined) {
      patch[at.hidden] = { ...(base[at.hidden] ?? {}), [at.id]: entry.hidden };
    }
    return patch;
  }).catch((error: unknown) => {
    // A preference that could not be persisted is still the arrangement on
    // screen. The author is not told, because there is nothing they would do
    // about it and what they just chose is exactly what they are looking at.
    logWarn(["frontend"], "the split of a discussion column was not stored", {
      column: key,
      error: String(error),
    });
  });
}

/**
 * DDS-FR-PNXR / DDS-FR-XQMF / CMT-FR-30: restore one project's per-draft and
 * per-file column preferences.
 *
 * Called when a project opens. A column with nothing stored keeps the
 * defaults, and a stored value this build does not recognise is ignored rather
 * than refused — a preset added later must not break a session that reads it.
 */
export async function restoreDiscussionColumns(key: string): Promise<void> {
  projectKey = key;
  const prefs = await loadLayoutPreferences(key);
  // Another project opened while this one read its record: what was read is
  // not this session's any more.
  if (projectKey !== key) return;
  const map = new Map(states);
  let restored = 0;
  const restore = (
    ratios: Record<string, string> | undefined,
    hidden: Record<string, boolean> | undefined,
    keyOf: (id: string) => ColumnKey,
  ) => {
    // A column the author arranged while the record was read keeps what they
    // chose: the stored value is older than that choice.
    for (const [id, raw] of Object.entries(ratios ?? {})) {
      const ratio = parseRatio(raw);
      if (ratio === null || states.has(keyOf(id))) continue;
      map.set(keyOf(id), { ...(map.get(keyOf(id)) ?? INITIAL), ratio });
      restored += 1;
    }
    for (const [id, value] of Object.entries(hidden ?? {})) {
      if (typeof value !== "boolean" || states.has(keyOf(id))) continue;
      map.set(keyOf(id), { ...(map.get(keyOf(id)) ?? INITIAL), hidden: value });
      restored += 1;
    }
  };
  restore(prefs.draftDiscussionRatios, prefs.draftDiscussionHidden, draftColumn);
  restore(prefs.fileDiscussionRatios, prefs.fileDiscussionHidden, fileColumn);
  if (restored === 0) return;
  states = map;
  logDebug(["frontend"], "restored the discussion column splits of a project", {
    entries: restored,
  });
  emit();
}

/** One column's state, in React. */
export function useDiscussionColumn(key: ColumnKey): DiscussionColumnState {
  return useSyncExternalStore(
    subscribe,
    () => discussionColumnState(key),
    () => discussionColumnState(key),
  );
}

/**
 * DCR-FR-33: everything here belongs to the project it was read in, and is
 * discarded when the project closes or the active worktree changes.
 */
export function resetDiscussionColumns(): void {
  states = new Map();
  projectKey = null;
  emit();
}
