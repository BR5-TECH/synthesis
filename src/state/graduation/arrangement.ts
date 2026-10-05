/**
 * Arranging a stream's queue: pause, resume, auto-start and reorder
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-XQVG).
 *
 * Every predicate here answers the same question the backend answers, so a
 * control the surface offers is one the operation accepts. A surface that
 * offered more would render a control whose only outcome is a refusal.
 */

import type { GraduationQueue, GraduationRun } from "../../types/graduation";
import { runTitle } from "./merge";
import { runQueueKey, targetName } from "./runState";

/** GRD-FR-MDQZ: pause is accepted only while the run is doing agent work. */
export function canPause(run: GraduationRun): boolean {
  return run.state === "working" || run.state === "reviewing";
}

/** GRD-FR-CYIB: Continue and Resume are one operation. */
export function canContinue(run: GraduationRun): boolean {
  return (
    run.state === "interrupted" ||
    run.state === "blocked" ||
    run.state === "awaiting_author" ||
    // GRD-FR-XRDY: a queued direct run held for its branch is offered a
    // dispatch by the same operation.
    (run.state === "queued" && Boolean(run.targetHold))
  );
}

/** Whether the control reads as Resume rather than Continue. */
export function isResume(run: GraduationRun): boolean {
  return run.interruption?.reason === "author_pause";
}

/** GRD-FR-TKUR: auto-start is settled from `queued` alone. */
export function canSetAutoStart(run: GraduationRun): boolean {
  return run.state === "queued";
}

/**
 * GRD-FR-RHNP: a reorder is accepted for a queued, unarchived member alone.
 *
 * A run the author has filed away is not one they are arranging.
 */
export function canReorder(run: GraduationRun): boolean {
  return run.state === "queued" && !run.archived;
}

/** GRD-FR-JOFE: archiving is accepted from every state. */
export function canArchive(): boolean {
  return true;
}

/**
 * GRD-FR-VLFO: one queue, in the project's run order. `key` is a stream's id
 * or an ordinary worktree's queue key (GRD-FR-ZVNO).
 */
export function queueOf(queue: GraduationQueue, key: string): GraduationRun[] {
  return queue.runs.filter((run) => runQueueKey(run) === key && run.state === "queued");
}

/**
 * GRD-FR-RHNP: a run's index in its own stream's queue, counted from the
 * earliest member as zero.
 */
export function positionOf(queue: GraduationQueue, runId: string): number | null {
  const run = queue.runs.find((r) => r.id === runId);
  if (!run) return null;
  const index = queueOf(queue, runQueueKey(run)).findIndex((r) => r.id === runId);
  return index === -1 ? null : index;
}

/** How many runs wait ahead of this one in its own stream. */
export function aheadOf(queue: GraduationQueue, runId: string): number | null {
  return positionOf(queue, runId);
}

/** What the rail announces when a run moves in its stream's queue. */
export function reorderAnnouncement(
  run: GraduationRun,
  from: number,
  to: number,
): string {
  const direction = to < from ? "earlier" : "later";
  return `Moved “${runTitle(run)}” ${direction} in ${targetName(run)}, to position ${to + 1}.`;
}

/** What the rail announces when the author pauses or resumes a run. */
export function pauseAnnouncement(run: GraduationRun, paused: boolean): string {
  return paused
    ? `Paused “${runTitle(run)}”. Its work stream is free for the run behind it.`
    : `Resumed “${runTitle(run)}”. It waits at the front of ${targetName(run)}.`;
}

/** What the rail announces when auto-start is turned off or on. */
export function autoStartAnnouncement(run: GraduationRun, enabled: boolean): string {
  return enabled
    ? `“${runTitle(run)}” will start when its work stream is free.`
    : `“${runTitle(run)}” will not start on its own.`;
}
