/**
 * What a PR summary run says about itself
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WEBM,
 * GRU-FR-AQKZ, GRU-FR-DQSA, GRU-FR-LJUY).
 *
 * A PR summary run is a graduation run that carries `prSummary` data
 * (`../../../specifications/core/GRD-graduation.md` GRD-FR-KAKU). It has no
 * draft and no stream: it is one agent turn that writes the title and the
 * description of a pull request from a head branch. Pure functions of a run as
 * the backend reports it.
 */

import type { GraduationRun, RunSummary } from "../../types/graduation";
import type { ProgressStage } from "../runProgress";

/** GRD-FR-KAKU: a run is a PR summary run when its record carries `prSummary`. */
export function isPrSummaryRun(run: GraduationRun): boolean {
  return Boolean(run.prSummary);
}

/** GRU-FR-WEBM: the mode of a PR summary run, in the author's words. */
export function prSummaryModeWords(run: GraduationRun): string {
  return run.prSummary?.mode === "combine" ? "from run summaries" : "from the diff";
}

/**
 * GRU-FR-WEBM: the provenance line of a PR summary run. It names the head
 * branch, the base branch, and the mode.
 */
export function prSummaryProvenanceLine(run: GraduationRun): string {
  const data = run.prSummary!;
  return `Summarizes the pull request from “${data.head}” into “${data.base}”, ${prSummaryModeWords(run)}.`;
}

/**
 * GRU-FR-AQKZ: the three stages of a PR summary run: Queued, Summarizing and
 * Done. The ids are the ordinary ones, and the run never reaches the review
 * stage, so it holds none.
 */
export const PR_SUMMARY_STAGES: ProgressStage[] = [
  {
    id: "queued",
    label: "Queued",
    description: "Waiting for a free slot",
  },
  {
    id: "working",
    label: "Summarizing",
    description: "Writing the title and the description of the pull request",
  },
  {
    id: "done",
    label: "Done",
    description: "The title and the description are written",
  },
];

/**
 * GRU-FR-DQSA: the result a PR summary run renders. Null in every state but
 * `completed`, so no other state renders one.
 */
export function prSummaryResultOf(run: GraduationRun): RunSummary | null {
  if (run.state !== "completed") return null;
  return run.prSummary?.result ?? null;
}
