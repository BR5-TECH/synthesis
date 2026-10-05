/**
 * What the graduation state modules share.
 */

import type { GraduationRun } from "../../types/graduation";
import type { PassRecord } from "../../types/graduationObservability";

/** The passes this run has made, oldest first. */
export function passesOf(run: GraduationRun): PassRecord[] {
  return run.observability?.passes ?? [];
}

/** GOB-FR-BTXN: whether a review sent this run round again. */
export function wentRoundAgain(run: GraduationRun): boolean {
  return (run.observability?.stageHistory ?? []).some(
    (entry) => entry.reason === "review_revision",
  );
}

/** The findings of every pass, newest pass first. */
export function allFindings(run: GraduationRun) {
  return passesOf(run)
    .slice()
    .reverse()
    .flatMap((pass) => pass.findings);
}
