/**
 * What the New Artifact tab's review holds that is neither the draft nor the
 * conversation (`DCR-draft-change-review.md`): the hunk under review and the
 * requests to show it.
 *
 * How the tab is split between its columns is the shared column store's
 * (`./discussionColumn.ts`), which the Editor tab uses too. The draft-named
 * functions below reach that store under the draft's own column key.
 *
 * A module-level store rather than tab state, because the hunk under review
 * must survive the tab being backgrounded and returned to.
 */
import { useSyncExternalStore } from "react";

import type { SplitRatio } from "../components/DraftDiscussion/ratio";
import {
  discussionColumnState,
  draftColumn,
  resetDiscussionColumns,
  setColumnHidden,
  setColumnRatio,
  type DiscussionColumnState,
} from "./discussionColumn";

/** What one draft's review holds that the discussion itself does not. */
export interface DraftReviewState {
  /** DCR-FR-12: the hunk the review bar and the action chip are about. */
  focusedHunkId: string | null;
  /**
   * CMT-FR-48 / DDS-FR-PBXN: how many times the author asked to see a change
   * in the document. A new count scrolls the document to the focused change,
   * also when that change is already the focused one.
   */
  hunkReveal: number;
}

/** A draft's column and review state together, as one read. */
export type DraftDiscussionState = DiscussionColumnState & DraftReviewState;

const INITIAL: DraftReviewState = {
  focusedHunkId: null,
  hunkReveal: 0,
};

let states: ReadonlyMap<string, DraftReviewState> = new Map();
const listeners = new Set<() => void>();

function emit(): void {
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function reviewState(draftId: string): DraftReviewState {
  return states.get(draftId) ?? INITIAL;
}

/** One draft's column and review state, defaulted until something is set. */
export function draftDiscussionState(draftId: string): DraftDiscussionState {
  return { ...discussionColumnState(draftColumn(draftId)), ...reviewState(draftId) };
}

function update(draftId: string, change: Partial<DraftReviewState>): void {
  const held = reviewState(draftId);
  const next = { ...held, ...change };
  if (
    next.focusedHunkId === held.focusedHunkId &&
    next.hunkReveal === held.hunkReveal
  ) {
    return;
  }
  const map = new Map(states);
  map.set(draftId, next);
  states = map;
  emit();
}

/** DDS-FR-PNXR: how this draft's tab is split, restored per draft. */
export function setDraftRatio(draftId: string, ratio: SplitRatio): void {
  setColumnRatio(draftColumn(draftId), ratio);
}

/** DDS-FR-XQMF: whether this draft's discussion column is hidden. */
export function setDraftDiscussionHidden(draftId: string, hidden: boolean): void {
  setColumnHidden(draftColumn(draftId), hidden);
}

/** DCR-FR-12: the hunk the review is on. */
export function setFocusedHunk(draftId: string, hunkId: string | null): void {
  update(draftId, { focusedHunkId: hunkId });
}

/**
 * CMT-FR-48 / DDS-FR-PBXN: put the review on this hunk and scroll the document
 * to it, also when the review is on it already.
 */
export function revealHunk(draftId: string, hunkId: string): void {
  const held = reviewState(draftId);
  update(draftId, { focusedHunkId: hunkId, hunkReveal: held.hunkReveal + 1 });
}

/** One draft's review state, in React. */
export function useDraftReviewState(draftId: string): DraftReviewState {
  return useSyncExternalStore(
    subscribe,
    () => reviewState(draftId),
    () => reviewState(draftId),
  );
}

/**
 * DCR-FR-33: everything here belongs to the project it was read in, and is
 * discarded when the project closes or the active worktree changes — the
 * column splits of every draft and every file with it.
 */
export function resetDraftDiscussions(): void {
  states = new Map();
  resetDiscussionColumns();
  emit();
}
