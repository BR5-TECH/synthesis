/**
 * What an OpenSpec change run says about itself
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-SOFY,
 * GRU-FR-PXCW, GRU-FR-HZLV, GRU-FR-HABG, GRU-FR-IKEQ, GRU-FR-NGQE;
 * `../../../specifications/core/GOB-graduation-observability.md` GOB-FR-CAXV).
 *
 * An apply run and a rework run are stream runs with no draft. They carry
 * `openspec` data with the phase `apply` or `rework`, and the words a surface
 * shows for them are data here, selected by that phase. Pure functions of a
 * run as the backend reports it.
 */

import type { ApplyProgress } from "../../types/changeRuns";
import type { GraduationRun } from "../../types/graduation";
import { hasReadableProgress } from "../../types/graduationObservability";
import type { ProgressStage } from "../runProgress";

/** OCR-FR-VHQW: the phase of an OpenSpec change run, or null for any other run. */
export function changeRunPhase(run: GraduationRun): "apply" | "rework" | null {
  const phase = run.openspec?.phase;
  return phase === "apply" || phase === "rework" ? phase : null;
}

/** OCR-FR-VHQW: whether this run is an apply run or a rework run. */
export function isChangeRun(run: GraduationRun): boolean {
  return changeRunPhase(run) !== null;
}

/** OCR-FR-FZVF: the title of an apply run or a rework run, or null for any other run. */
export function changeRunTitle(run: GraduationRun): string | null {
  const phase = changeRunPhase(run);
  if (phase === null) return null;
  return `${phase === "apply" ? "Apply" : "Rework"} ${run.openspec!.change}`;
}

/**
 * GOB-FR-CAXV / GRU-FR-SOFY: the stages of an apply run. The ids and the order
 * are the ordinary ones; only the words are an apply run's.
 */
export const APPLY_STAGES: ProgressStage[] = [
  { id: "queued", label: "Queued", description: "Waiting for the work stream" },
  { id: "working", label: "Tasks", description: "Implementing the tasks of the change" },
  { id: "review", label: "Verify", description: "Verifying the work against the change" },
  { id: "done", label: "Landed", description: "Committed on the work stream" },
];

/** GOB-FR-CAXV / GRU-FR-SOFY: the stages of a rework run. */
export const REWORK_STAGES: ProgressStage[] = [
  { id: "queued", label: "Queued", description: "Waiting for the work stream" },
  { id: "working", label: "Reworking", description: "Revising the change on your instruction" },
  { id: "review", label: "Reviewing", description: "Judging the revised change" },
  { id: "done", label: "Proposed", description: "Given to you as a proposal" },
];

/** GOB-FR-CAXV: the `done` stage in the condition `stopped`, for each phase. */
const APPLY_STAGES_NOT_LANDED = APPLY_STAGES.map((stage) =>
  stage.id === "done"
    ? { ...stage, label: "Not landed", description: "The work was not committed" }
    : stage,
);
const REWORK_STAGES_NOT_PROPOSED = REWORK_STAGES.map((stage) =>
  stage.id === "done"
    ? { ...stage, label: "Not proposed", description: "No proposal was recorded" }
    : stage,
);

/** GOB-FR-CAXV: the stage descriptors of an OpenSpec change run, or null for any other run. */
export function changeRunStages(run: GraduationRun): ProgressStage[] | null {
  const phase = changeRunPhase(run);
  if (phase === null) return null;
  const stopped =
    hasReadableProgress(run.observability) &&
    run.observability.currentStage === "done" &&
    run.observability.stageCondition === "stopped";
  if (phase === "apply") return stopped ? APPLY_STAGES_NOT_LANDED : APPLY_STAGES;
  return stopped ? REWORK_STAGES_NOT_PROPOSED : REWORK_STAGES;
}

/**
 * GOB-FR-CAXV: the word an OpenSpec change run shows for its `working` and its
 * `review` stage, in the sentence of that stage.
 */
export function changeRunStageWord(run: GraduationRun, stage: "working" | "review"): string | null {
  const stages = changeRunStages(run);
  return stages?.find((s) => s.id === stage)?.label ?? null;
}

/**
 * GRU-FR-PXCW / OCW-FR-BTSI: the sentence of the Tasks stage of an apply run,
 * from `"read apply progress"`: the task counts and the next open task.
 */
export function tasksSentence(progress: ApplyProgress): string {
  const counts = taskCounts(progress);
  const next = progress.nextTask?.trim();
  return next ? `${counts} · next: ${next}` : counts;
}

/** OCW-FR-BTSI: the task counts alone, `<complete> of <total> tasks`. */
export function taskCounts(progress: ApplyProgress): string {
  return `${progress.complete} of ${progress.total} tasks`;
}

/**
 * GRU-FR-HZLV: the check state of an apply run, in words, or null where it
 * states nothing. Checks that did not pass win over a check gate, and a check
 * gate wins over a work turn in flight.
 */
export function checkStateOf(run: GraduationRun): string | null {
  if (changeRunPhase(run) !== "apply") return null;
  if (run.state === "awaiting_author" && run.rest?.reason === "checks_not_passed") {
    return "checks red";
  }
  if (run.state === "working" && run.checkpoint?.checkGate) return "correcting checks";
  if (run.state === "working") return "checks running";
  return null;
}

/** GRU-FR-NGQE: why the finish of a rework run could not record its proposal. */
const REWORK_FINISH_SENTENCES: Record<string, string> = {
  proposal_pending:
    "The change holds a pending edit proposal, so the result of the rework could not be recorded. Decide that proposal first, then continue the run.",
  discussion_resolved:
    "The request thread of the rework is resolved, so the result could not be recorded there. Reopen the thread, then continue the run.",
  write_failed: "The proposal could not be written.",
};

/** GRU-FR-NGQE: the sentence of a rework run's finish blocker, or undefined. */
export function reworkBlockerSentence(run: GraduationRun, code: string): string | undefined {
  if (changeRunPhase(run) !== "rework") return undefined;
  return REWORK_FINISH_SENTENCES[code];
}

/**
 * GRU-FR-IKEQ / RPV-FR-12: the one line an OpenSpec change run that ended
 * carries, or undefined where the ordinary line of a stream run holds.
 */
export function changeRunOutcome(run: GraduationRun): string | null | undefined {
  if (changeRunPhase(run) !== "rework") return undefined;
  switch (run.state) {
    case "completed":
      return run.openspec?.proposalId
        ? "The result is a proposal in the discussion of the change."
        : "The rework pass found nothing to change.";
    case "discarded":
      return "Discarded. Nothing was written to the change.";
    case "failed":
      return run.failure?.message ?? "The run failed.";
    default:
      return null;
  }
}
