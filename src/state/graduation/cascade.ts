/**
 * The queued runs a discard takes with it
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-CLFA,
 * GRU-FR-QYEE).
 *
 * Every function answers what the backend answers (GRD-FR-QCZF), so the
 * confirmation names the runs the operation will remove and no others.
 */

import type { GraduationQueue, GraduationRun } from "../../types/graduation";
import { runTitle } from "./merge";
import { holdsQueue, runQueueKey } from "./runState";

/** GRD-FR-BNTC: whether this run holds its queue, at rest or at work. */
export function holdsItsQueue(run: GraduationRun): boolean {
  return holdsQueue(run.state) || (run.state === "queued" && Boolean(run.dispatched));
}

/**
 * GRD-FR-QCZF: the runs queued behind this run in its own queue, in the
 * project's run order. A run that never held its queue takes none with it, and
 * a run ahead of it stays.
 */
export function runsRemovedByDiscard(
  queue: GraduationQueue | null | undefined,
  run: GraduationRun,
): GraduationRun[] {
  if (!queue || !holdsItsQueue(run)) return [];
  const position = queue.runs.findIndex((entry) => entry.id === run.id);
  if (position === -1) return [];
  const key = runQueueKey(run);
  return queue.runs
    .slice(position + 1)
    .filter(
      (entry) =>
        runQueueKey(entry) === key && entry.state === "queued" && !holdsItsQueue(entry),
    );
}

/** GRU-FR-CLFA: what the confirmation states about the runs it removes. */
export function removalStatement(removed: GraduationRun[]): string {
  const names = removed.map((entry) => `“${runTitle(entry)}”`).join(", ");
  const noun = removed.length === 1 ? "run" : "runs";
  return (
    `This also removes the ${noun} queued behind it: ${names}. ` +
    "Their drafts return to the active drafts, not graduated."
  );
}

/** GRU-FR-QYEE: what a failed run says about the runs queued behind it. */
export const FAILED_HOLD_STATEMENT =
  "The runs queued behind it stay queued until you discard it. Discarding it removes them.";
