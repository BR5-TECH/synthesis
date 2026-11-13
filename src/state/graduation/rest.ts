/**
 * What a run at rest for the author without an escalation says
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WKTL,
 * GRU-FR-TTLR, GRU-FR-WVDV; `../../../specifications/core/GRD-graduation.md`
 * GRD-FR-SKLO, GRD-FR-PYJN).
 *
 * A run with a rest is not a run whose pass budget is spent, so nothing here
 * names a pass.
 */

import { formatDurationMs } from "../graduationLoopSettings";
import type {
  ChecksNotPassedRest,
  GraduationCheckCause,
  GraduationRestCheck,
  GraduationRun,
  TotalTimeLimitRest,
} from "../../types/graduation";

/** GRD-FR-SKLO: the rest of a run in `awaiting_author` on the total time limit. */
export function totalTimeLimitRestOf(run: GraduationRun): TotalTimeLimitRest | null {
  if (run.state !== "awaiting_author") return null;
  const rest = run.rest;
  if (rest?.reason !== "total_time_limit") return null;
  if (typeof rest.limitMs !== "number" || typeof rest.usedMs !== "number") return null;
  return rest as TotalTimeLimitRest;
}

/** GRU-FR-WKTL: the headline, with the time used and the limit. */
export function totalTimeLimitHeadline(rest: TotalTimeLimitRest): string {
  return `Stopped at the total time limit · ${formatDurationMs(rest.usedMs)} of ${formatDurationMs(rest.limitMs)} used`;
}

/** GRU-FR-WKTL / GRL-FR-SPRE: what the rest means, and what Continue does. */
export function totalTimeLimitDetail(rest: TotalTimeLimitRest): string {
  return (
    `The run used ${formatDurationMs(rest.usedMs)} of working and reviewing time. ` +
    `Its total time limit is ${formatDurationMs(rest.limitMs)}. ` +
    "Continue sets the used time to zero and resumes the run. It does not do a completed pass again."
  );
}

/** GRD-FR-PYJN: the rest of a run in `awaiting_author` whose checks did not pass. */
export function checksNotPassedRestOf(run: GraduationRun): ChecksNotPassedRest | null {
  if (run.state !== "awaiting_author") return null;
  const rest = run.rest;
  if (rest?.reason !== "checks_not_passed") return null;
  return {
    ...rest,
    reason: "checks_not_passed",
    // GRD-FR-PYJN: a rest on checks holds a cause of the check gate alone.
    cause: (rest.cause as GraduationCheckCause | undefined) ?? "record_missing",
    checks: rest.checks ?? [],
  };
}

/** GRU-FR-WVDV: the headline of a run whose required checks did not pass. */
export const CHECKS_NOT_PASSED_HEADLINE = "Stopped: the required checks did not pass";

/** GRU-FR-WVDV: why the checks did not pass, in words, from the rest's cause. */
export function checksCauseSentence(cause: GraduationCheckCause): string {
  switch (cause) {
    case "checks_red":
      return "A required check failed, and the work turn could not make it pass.";
    case "checks_unfinished":
      return "A required check did not finish, and the work turn could not finish it.";
    case "record_missing":
      return "The work turn did not report which checks it ran.";
    case "work_failed":
      return "The work turn reported that it could not finish the work.";
  }
}

/** GRU-FR-WVDV: one check of the rest, with its exit status or "not finished". */
export function restCheckStatus(check: GraduationRestCheck): string {
  if (!check.finished || check.exitStatus === null) return "not finished";
  return `exit status ${check.exitStatus}`;
}

/** GRU-FR-WVDV: what the rest left, and what Continue does. */
export const CHECKS_NOT_PASSED_DETAIL =
  "Nothing went to review and nothing was committed. " +
  "Continue asks the work turn again to make the checks pass. It keeps the used time and the pass budget.";
