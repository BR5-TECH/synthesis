/**
 * Starting a new run over a discarded run's captured prompt
 * (`../../../specifications/ui/GRT-graduation-restart.md`).
 */

import type { GraduationRun } from "../../types/graduation";
import { isMergeRun } from "./merge";

/** GRT-FR-CTNO: Restart is offered for a discarded run and no other state. */
export function canRestart(run: GraduationRun): boolean {
  // GRU-FR-WDWB: a merge run has no captured draft to restart over.
  return run.state === "discarded" && !isMergeRun(run);
}

/**
 * GRT-FR-AMHS: the name both **Restart** controls carry. It names the run,
 * because the rail draws one control per row.
 */
export function restartLabel(run: GraduationRun): string {
  return `Restart “${run.input.draftName}”`;
}

/** GRT-FR-AMHS: the tooltip both **Restart** controls carry. */
export const RESTART_TOOLTIP =
  "Restart: starts a new run over this run's prompt, on a work stream you choose. This run stays discarded and unchanged.";

/**
 * GRT-FR-KSBC: a refused restart, rendered against the discarded run. It says
 * what the refusal was and what clears it, and that nothing was created.
 */
export function restartRefusal(message: string): string {
  return `${message} No new run was created, and this run is unchanged.`;
}

/**
 * GRT-FR-VWHM: what the confirmation says before anything is invoked.
 *
 * It names the run, states that a new run is created, and states that the
 * discarded run stays discarded and unchanged.
 */
export function restartConfirmation(run: GraduationRun): string {
  return (
    `Start a new run over the prompt “${run.input.draftName}” was captured from. ` +
    "The discarded run stays discarded and unchanged."
  );
}

/**
 * GRT-FR-NPDC: what the confirmation of a discarded direct run says about
 * where the new run works. It names the pinned worktree and branch, and it asks
 * for no stream.
 */
export function restartDirectStatement(run: GraduationRun): string | null {
  const target = run.directTarget;
  if (!target) return null;
  return `The new run works directly in the worktree “${target.worktreeName}”, on branch “${target.branch}”, as this run did. It asks for no work stream.`;
}

/**
 * GRT-FR-IMRI: the stream the confirmation defaults to.
 *
 * The stream the discarded run ran on, where that stream still exists.
 */
export function defaultStreamFor(
  run: GraduationRun,
  liveStreamIds: string[],
): string | null {
  return liveStreamIds.includes(run.streamId) ? run.streamId : (liveStreamIds[0] ?? null);
}

/** GRT-FR-FCUF: what a restarted run says about where it came from. */
export function restartedFromStatement(run: GraduationRun): string | null {
  return run.restartedFromRunId
    ? "This run was restarted from a run you discarded."
    : null;
}

/** GRT-FR-FCUF: what a discarded run says about the runs restarted from it. */
export function restartedAsStatement(
  run: GraduationRun,
  all: GraduationRun[],
): string | null {
  const children = all.filter((r) => r.restartedFromRunId === run.id);
  if (children.length === 0) return null;
  return children.length === 1
    ? "This run was restarted as one later run."
    : `This run was restarted as ${children.length} later runs.`;
}
