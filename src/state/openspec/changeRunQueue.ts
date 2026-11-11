/**
 * The graduation runs that the change workspace and its request cards read
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-JHAB,
 * OCW-FR-SFYL, OCW-FR-CTWP).
 *
 * One reading of `list_graduation_queue` for the whole window, so the run
 * strip of a tab and the request cards in its discussion column never
 * disagree about a run. The tab reads it again on `"graduation queue changed"`
 * and `"graduation run changed"` for its stream. An older answer that lands
 * after a newer read is dropped.
 */
import { useEffect, useSyncExternalStore } from "react";

import { listGraduationQueue } from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { GraduationRun } from "../../types";

type Reading = "unread" | "asking" | "read" | "failed";

const NONE: readonly GraduationRun[] = Object.freeze([]);
let runs: readonly GraduationRun[] = NONE;
let status: Reading = "unread";
let asks = 0;
let generation = 0;
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Read the runs again. The answer replaces what the store held. */
export function rereadChangeRunQueue(): Promise<void> {
  const mine = ++asks;
  const asked = generation;
  status = "asking";
  return listGraduationQueue().then(
    (queue) => {
      if (asked !== generation || mine !== asks) return;
      runs = Array.isArray(queue?.runs) ? queue.runs : NONE;
      status = "read";
      logDebug(["frontend"], "the runs of the change workspace were read", {
        runs: runs.length,
      });
      notify();
    },
    (error: unknown) => {
      if (asked !== generation || mine !== asks) return;
      status = "failed";
      logWarn(["frontend"], "the runs of the change workspace could not be read", {
        error: String(error).slice(0, 80),
      });
      notify();
    },
  );
}

/** Take the one reading, where none is taken or the last one failed. */
export function ensureChangeRunQueue(): void {
  if (status === "asking" || status === "read") return;
  void rereadChangeRunQueue();
}

/** OCW-FR-DTJD / OCW-FR-FEOA: a run a start answered, before the queue is read again. */
export function noteStartedRun(run: GraduationRun): void {
  runs = [...runs.filter((r) => r.id !== run.id), run];
  notify();
}

/** One run as the store holds it. */
export function changeRunById(runId: string): GraduationRun | undefined {
  return runs.find((r) => r.id === runId);
}

/**
 * Every run the store holds, in the project's run order. `ensure` takes the
 * store's first read where none is taken.
 */
export function useChangeRunQueue(ensure = true): readonly GraduationRun[] {
  useEffect(() => {
    if (ensure) ensureChangeRunQueue();
  }, [ensure]);
  return useSyncExternalStore(subscribe, () => runs);
}

/** Tests only: start from an empty store. */
export function resetChangeRunQueue(): void {
  runs = NONE;
  status = "unread";
  asks = 0;
  generation += 1;
  notify();
}
