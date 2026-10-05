/**
 * Why a queued run waits for a project slot
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-KMNF).
 *
 * Pure functions of the capacity the backend reports (GRD-FR-GRHC). Nothing
 * here guesses: a capacity that was not read states nothing.
 */

import type { GraduationCapacity, GraduationRun } from "../../types";

/** The words for a run that waits for a project slot. */
export const SLOT_WAIT_HEADLINE = "Waiting for a project slot";

/**
 * GRU-FR-QKDB: a capacity is usable only when it was read. A failed read, or
 * an answer that has no list of waiting runs, counts as unread.
 */
export function readableCapacity(
  value: GraduationCapacity | null | undefined,
): GraduationCapacity | null {
  if (!value || !Array.isArray(value.waitingForSlot)) return null;
  return value;
}

/**
 * GRU-FR-KMNF: whether the backend lists this queued run as waiting for a
 * project slot alone. A run that waits behind a run of its own stream or
 * worktree is not listed.
 */
export function waitsForSlot(
  run: GraduationRun,
  capacity: GraduationCapacity | null,
): boolean {
  return (
    run.state === "queued" &&
    capacity !== null &&
    capacity.waitingForSlot.includes(run.id)
  );
}

/**
 * GRU-FR-KMNF: the sentence that names the project slot, the limit and the
 * slots held, in words.
 */
export function slotWaitSentence(capacity: GraduationCapacity): string {
  if (typeof capacity.limit !== "number") return `${SLOT_WAIT_HEADLINE}.`;
  const runs = capacity.limit === 1 ? "run" : "runs";
  return (
    `${SLOT_WAIT_HEADLINE} — the project works ${capacity.inUse} of ` +
    `${capacity.limit} graduation ${runs}.`
  );
}
