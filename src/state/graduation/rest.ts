/**
 * What a run at rest on its total time limit says
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WKTL,
 * GRU-FR-TTLR; `../../../specifications/core/GRD-graduation.md` GRD-FR-SKLO).
 *
 * A run with a rest is not a run whose pass budget is spent, so nothing here
 * names a pass.
 */

import { formatDurationMs } from "../graduationLoopSettings";
import type { GraduationRest, GraduationRun } from "../../types/graduation";

/** GRD-FR-SKLO: the rest of a run in `awaiting_author` on the total time limit. */
export function totalTimeLimitRestOf(run: GraduationRun): GraduationRest | null {
  if (run.state !== "awaiting_author") return null;
  return run.rest?.reason === "total_time_limit" ? run.rest : null;
}

/** GRU-FR-WKTL: the headline, with the time used and the limit. */
export function totalTimeLimitHeadline(rest: GraduationRest): string {
  return `Stopped at the total time limit · ${formatDurationMs(rest.usedMs)} of ${formatDurationMs(rest.limitMs)} used`;
}

/** GRU-FR-WKTL / GRL-FR-SPRE: what the rest means, and what Continue does. */
export function totalTimeLimitDetail(rest: GraduationRest): string {
  return (
    `The run used ${formatDurationMs(rest.usedMs)} of working and reviewing time. ` +
    `Its total time limit is ${formatDurationMs(rest.limitMs)}. ` +
    "Continue sets the used time to zero and resumes the run. It does not do a completed pass again."
  );
}
