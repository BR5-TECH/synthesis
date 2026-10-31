/**
 * Whether the content root is gone
 * (`../../specifications/ui/WTS-worktree-selector.md` WTS-FR-BIOJ, WTS-FR-FEEK,
 * WTS-FR-OSKY).
 *
 * A module store, because a teardown reads it outside React: the flush that
 * comes before a teardown has nothing to write to a root that is gone.
 */
import { useSyncExternalStore } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import type { ActiveRootState } from "../types";

let gone: ActiveRootState | null = null;
const listeners = new Set<() => void>();

const publish = (next: ActiveRootState | null) => {
  if (next === gone) return;
  gone = next;
  for (const listener of listeners) listener();
};

/** WTS-FR-FEEK: an `unknown` answer neither opens the dialog nor closes it. */
export function applyActiveRootState(answer: ActiveRootState): void {
  if (answer.state === "gone") publish(answer);
  if (answer.state === "healthy") publish(null);
}

/** The active worktree is the primary one, or no project is open. */
export function clearActiveRootState(): void {
  publish(null);
}

/** WTS-FR-OSKY: read by a teardown, which writes nothing while this holds. */
export function isActiveRootGone(): boolean {
  return gone !== null;
}

/**
 * WTS-FR-OSKY: whether the root is gone, by a fresh read where the last one said
 * so. What comes next discards unsaved work, and the worktree may have come back.
 * An answer that cannot be read leaves the last one standing.
 */
export async function rootIsGoneNow(): Promise<boolean> {
  if (!isActiveRootGone()) return false;
  try {
    const answer = await api.activeRootState();
    if (answer && typeof answer === "object") applyActiveRootState(answer);
  } catch (reason) {
    logWarn(["frontend"], "active root state could not be read again", {
      reason: String(reason),
    });
  }
  return isActiveRootGone();
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

/** The answer that says the content root is gone, or null. */
export function useGoneActiveRoot(): ActiveRootState | null {
  return useSyncExternalStore(subscribe, () => gone);
}

/** For a test: nothing goes to the next one. */
export function resetActiveRootHealth(): void {
  gone = null;
  listeners.clear();
}
