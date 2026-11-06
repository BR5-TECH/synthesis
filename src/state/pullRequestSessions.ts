/**
 * The saved state of the Create a PR window, per head branch
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-SSQI,
 * CPR-FR-ESXZ).
 *
 * The window keeps the title, the description, the base branch, the draft
 * toggle, the features, and the PR summary run it follows, with its result or
 * failure, for the application session. A new opening for the same head branch
 * restores it instead of the source. The store is module-level and keyed by
 * the head branch, so it survives the window, as `discussionComposers` does for
 * the Discuss composer.
 *
 * It is held in memory only. A submission that creates the pull request clears
 * the state of its head branch, and a change of the project clears all of it.
 */
import { useSyncExternalStore } from "react";

import type { GraduationRun, PrSummaryMode } from "../types/graduation";

/** CPR-FR-GDDO: one parent issue the author picked as a feature. */
export interface PullRequestFeature {
  owner: string;
  repository: string;
  number: number;
  title: string;
}

/** CPR-FR-VOEB … CPR-FR-URIL: the PR summary run the window follows. */
export interface PullRequestSummaryFollow {
  /** Null when the start itself was refused. */
  runId: string | null;
  mode: PrSummaryMode;
  status: "starting" | "running" | "completed" | "failed" | "discarded";
  /** The newest record read of the run, for its progress. */
  run: GraduationRun | null;
  /** The typed cause, in words, of a failed or discarded run or start. */
  message: string | null;
}

/** CPR-FR-SSQI: what the window keeps for one head branch. */
export interface PullRequestSession {
  head: string;
  title: string;
  body: string;
  base: string;
  draft: boolean;
  features: PullRequestFeature[];
  summary: PullRequestSummaryFollow | null;
  /** CPR-FR-ZZNM: a combine ran for this head branch. */
  combined: boolean;
}

/** CPR-FR-JOIG: what an opener gives the window to start from. */
export interface PullRequestSessionSource {
  head: string;
  base: string;
  title: string;
}

const sessions = new Map<string, PullRequestSession>();
const listeners = new Set<() => void>();
let version = 0;
/**
 * Counts the project changes. An answer that was asked for in an older epoch
 * is for the project the author left, and changes nothing (CPR non-functional).
 */
let epoch = 0;

function emit(): void {
  version += 1;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The saved state of `head`, or null when the window holds none. */
export function getPullRequestSession(head: string): PullRequestSession | null {
  return sessions.get(head) ?? null;
}

/** Every saved state, for the follower of PR summary runs. */
export function allPullRequestSessions(): PullRequestSession[] {
  return [...sessions.values()];
}

/**
 * CPR-FR-JOIG / CPR-FR-SSQI: the saved state of `source.head`, made from the
 * source when none exists. `created` is true when it was made now.
 */
export function openPullRequestSession(source: PullRequestSessionSource): {
  session: PullRequestSession;
  created: boolean;
} {
  const saved = sessions.get(source.head);
  if (saved) return { session: saved, created: false };
  const session: PullRequestSession = {
    head: source.head,
    title: source.title,
    body: "",
    base: source.base,
    draft: false,
    features: [],
    summary: null,
    combined: false,
  };
  // No emit: the window that opens it reads it in the same render, and an
  // emit during a render would update other components in that render.
  sessions.set(source.head, session);
  return { session, created: true };
}

/** Change the saved state of `head`. A head with no saved state is left alone. */
export function updatePullRequestSession(
  head: string,
  change: Partial<PullRequestSession> | ((s: PullRequestSession) => Partial<PullRequestSession>),
): void {
  const current = sessions.get(head);
  if (!current) return;
  const patch = typeof change === "function" ? change(current) : change;
  sessions.set(head, { ...current, ...patch, head });
  emit();
}

/** CPR-FR-ESXZ: a created pull request clears the state of its head branch. */
export function clearPullRequestSession(head: string): void {
  if (!sessions.delete(head)) return;
  emit();
}

const clearListeners = new Set<() => void>();

/** Run `listener` each time every saved state is cleared. */
export function onPullRequestSessionsCleared(listener: () => void): () => void {
  clearListeners.add(listener);
  return () => clearListeners.delete(listener);
}

/** CPR-FR-ESXZ: a change of the project clears every saved state. */
export function clearAllPullRequestSessions(): void {
  epoch += 1;
  for (const listener of [...clearListeners]) listener();
  if (sessions.size === 0) return;
  sessions.clear();
  emit();
}

/** The project epoch an asynchronous answer is checked against. */
export function pullRequestSessionEpoch(): number {
  return epoch;
}

/** The saved state of `head` as React state. */
export function usePullRequestSession(head: string): PullRequestSession | null {
  useSyncExternalStore(
    subscribe,
    () => version,
    () => version,
  );
  return sessions.get(head) ?? null;
}

/** The head branch of the Create a PR window that is open, or `null`. */
let openWindowHead: string | null = null;

/**
 * Mark the window of `head` as open. Returns the call that marks it closed.
 * A window that a later one replaced does not clear the mark of that one.
 */
export function markPullRequestWindowOpen(head: string): () => void {
  openWindowHead = head;
  return () => {
    if (openWindowHead === head) openWindowHead = null;
  };
}

/** Whether the Create a PR window of `head` is open. */
export function isPullRequestWindowOpenFor(head: string): boolean {
  return openWindowHead === head;
}
