/**
 * The run's progress, as the stage row reads it
 * (`../../../specifications/ui/GRU-graduation-runs.md`).
 */

import type {
  GraduationInterruptionReason,
  GraduationRun,
  GraduationRunState,
} from "../../types/graduation";
import { passBoundOf } from "../../types/graduation";
import { blockerSentence } from "./blockers";
import { failureStatement, isMergeRun } from "./merge";
import { SLOT_WAIT_HEADLINE } from "./slotWait";
import type {
  GraduationObservability,
  GraduationVisualStage,
} from "../../types/graduationObservability";
import { hasReadableProgress } from "../../types/graduationObservability";
import type { ProgressStage, ProgressTransition, StageCondition } from "../runProgress";

/**
 * GRU-FR-IZKI: the **four** stages of a run, in order, as data.
 *
 * The labels and their accessible descriptions are this surface's; the
 * visualization holds none of them (`RPV-run-progress.md` RPV-FR-03), which is
 * what lets the same component render four stages exactly as it renders any
 * other number.
 */
export const GRADUATION_STAGES: ProgressStage[] = [
  {
    id: "queued",
    label: "Queued",
    description: "Waiting for the work stream",
  },
  {
    id: "working",
    label: "Working",
    description: "Doing the work the prompt asked for",
  },
  {
    id: "review",
    label: "Review",
    description: "Judging what the work turn wrote",
  },
  {
    id: "done",
    label: "Done",
    description: "Committed on the work stream",
  },
];

/**
 * GRU-FR-YSTV: a merge run's four stages. The ids and the order are the
 * ordinary ones; only the labels and descriptions are a merge's. Held as data
 * beside the draft run's, so no rule branches on a stage id to choose one
 * (GRU-FR-ZBMU).
 */
export const MERGE_STAGES: ProgressStage[] = [
  {
    id: "queued",
    label: "Queued",
    description: "Waiting for the work stream",
  },
  {
    id: "working",
    label: "Reconciling",
    description: "Reconciling the merge in a working copy of its own",
  },
  {
    id: "review",
    label: "Reviewing",
    description: "Judging the reconciled result against both branches",
  },
  {
    id: "done",
    label: "Merged",
    description: "Merge applied to the base branch",
  },
];

/**
 * GOB-FR-PLTB: the stages of a merge run that ended without applying its merge.
 * The `done` stage stands in the condition `stopped`, and its label says the
 * merge did not land.
 */
export const MERGE_STAGES_NOT_MERGED: ProgressStage[] = MERGE_STAGES.map((stage) =>
  stage.id === "done"
    ? { ...stage, label: "Not merged", description: "The merge was not applied" }
    : stage,
);

/** GRU-FR-ZBMU / GRU-FR-YSTV: the stage descriptors this run's row reads. */
export function stageDescriptorsFor(run: GraduationRun): ProgressStage[] {
  if (!isMergeRun(run)) return GRADUATION_STAGES;
  const ended =
    hasReadableProgress(run.observability) &&
    run.observability.currentStage === "done" &&
    run.observability.stageCondition === "stopped";
  return ended ? MERGE_STAGES_NOT_MERGED : MERGE_STAGES;
}

export { hasReadableProgress };

/**
 * GRU-FR-IZKI: the stage the run stands at, read from its persisted record.
 *
 * Never derived from the state name: the record is what the backend wrote, and
 * a surface that recomputed it would draw something the backend never held.
 */
export function stageOf(run: GraduationRun): GraduationVisualStage | null {
  if (!hasReadableProgress(run.observability)) return null;
  return run.observability.currentStage;
}

/** GOB-FR-ZMCA: the condition that stage stands in. */
export function conditionOf(run: GraduationRun): StageCondition {
  if (!hasReadableProgress(run.observability)) return "waiting";
  return run.observability.stageCondition;
}

/** GRU-FR-HKBD: what a run's own queue is called. */
function queueWord(run: GraduationRun): string {
  return run.directTarget && !run.streamId ? "worktree's queue" : "stream";
}

/**
 * GRU-FR-HKBD / GRU-FR-LBPR: the one sentence the row is built around.
 *
 * `queuePosition` is the run's index in **its own stream's** queue, counted
 * from the earliest member as zero.
 */
export function conditionSentence(
  run: GraduationRun,
  queuePosition?: number | null,
  waitsForProjectSlot = false,
): string {
  switch (run.state) {
    case "queued":
      {
        // GRU-FR-HKBD: the position AND the auto-start note. A run that is not
        // eligible still has a place, and saying only one of the two leaves the
        // author guessing at the other.
        // GRU-FR-KMNF: a run that waits for a project slot says that, and never
        // the sentence for its own queue in the same breath.
        const place = waitsForProjectSlot
          ? SLOT_WAIT_HEADLINE
          : queuePosition === 0
            ? `Next in the ${queueWord(run)}`
            : typeof queuePosition === "number"
              ? `${queuePosition} ahead of it in the ${queueWord(run)}`
              : run.directTarget
                ? "Waiting for the worktree"
                : "Waiting for the work stream";
        return run.autoStart
          ? place
          : `${place} · auto-start off, so it will not start on its own`;
      }
    case "working":
      // GRU-FR-YSTV: a merge run reconciles where a draft run works.
      return `${isMergeRun(run) ? "Reconciling" : "Working"} · pass ${passOf(run)} of ${passBoundOf(run)}`;
    case "reviewing":
      return `${isMergeRun(run) ? "Reviewing" : "Review"} · pass ${passOf(run)} of ${passBoundOf(run)}`;
    case "awaiting_author":
      // GRU-FR-FZCN: three shapes rest a run for the author — an escalation it
      // asked, a review that did not settle, and a blocker that repeated.
      if (run.escalation) return "Waiting for your answer";
      // What stopped it is stated whole in the blocker beneath, so the line
      // says the shape rather than repeating a code in it.
      if (run.blocker) return "Stopped twice on the same thing";
      // GRU-FR-FZCN: a review that did not settle before the budget was spent
      // says what it spent, so the author reads a bound rather than a run that
      // stopped for no stated reason. The findings themselves stand in the pass
      // history beneath. A run with no window states no spend.
      return spentSentence(passBudgetOf(run));
    case "blocked":
      // GRU-FR-LBPR: the row leads with what the author acts on. The backend's
      // own message is an internal error string carrying an absolute path, and
      // it belongs in the detail line beneath rather than in the headline.
      return run.blocker ? `Blocked — ${blockerSentence(run.blocker)}` : "Blocked";
    case "interrupted":
      return interruptionSentence(run);
    case "completed":
      return isMergeRun(run) ? "Merged" : "Done";
    case "discarded":
      return "Discarded";
    case "failed":
      return "Failed";
  }
}

/** GRU-FR-FZCN: the decision line, with the spend where there is one. */
function spentSentence(budget: number | null): string {
  if (budget === null) return "Waiting for your decision";
  const passes = budget === 1 ? "1 pass" : `${budget} passes`;
  return `Waiting for your decision · ${passes} spent`;
}

/** GRU-FR-BHJO: the headline that names why an interrupted run stopped. */
function interruptionSentence(run: GraduationRun): string {
  const reason = run.interruption?.reason;
  switch (reason) {
    case "author_pause":
      return "Paused — waiting for Resume";
    case "application_shutdown":
      return "Stopped when the application closed";
    case "project_changed":
      return "Stopped when the project changed";
    case undefined:
      return "Stopped — waiting for Continue";
    default:
      return `Stopped — ${interruptionCause(reason)}`;
  }
}

/**
 * GRU-FR-BHJO / GRU-FR-JAEY / GRU-FR-FJZD: the short cause of a stop, read in
 * the headline, the pass row and the notification alike, so the three never
 * name one stop in two ways.
 */
export function interruptionCause(reason: GraduationInterruptionReason): string {
  switch (reason) {
    case "author_pause":
      return "you paused it";
    case "application_shutdown":
      return "the application closed";
    case "project_changed":
      return "the project changed";
    case "log_persistence_failed":
      return "its log could not be written";
    case "execution_abandoned":
      return "the application closed while it was working";
    case "execution_timeout":
      return "the turn reached its time limit";
    case "agent_exited":
      return "the agent process exited";
    case "agent_terminated":
      return "the agent process was terminated";
    case "unreadable_answer":
      return "the agent's answer could not be read";
    case "launch_failed":
      return "the agent turn could not start";
    case "retryable_failure":
      return "the cause was not recorded";
  }
}

/**
 * GRU-FR-BLSS: whether a stop is a failure. A pause, a shutdown and a project
 * change are stops the author or the application made on purpose.
 */
export function isFailureStop(reason: GraduationInterruptionReason): boolean {
  return (
    reason !== "author_pause" &&
    reason !== "application_shutdown" &&
    reason !== "project_changed"
  );
}

/**
 * GRU-FR-PNVX: the pass the run stands at, counted from one.
 *
 * The checkpoint is where the pass is counted (per
 * `../../../specifications/core/GXD-graduation-execution.md` GXD-FR-PWYD), so it
 * is what a surface reports. Counting the pass records instead freezes the
 * number whenever a pass opened but never settled — which is every run that came
 * to rest on a blocker, however often the author continued it.
 */
export function passOf(run: GraduationRun): number {
  const recorded = run.checkpoint?.pass ?? 0;
  if (recorded > 0) return recorded;
  return Math.max(1, run.observability?.passes?.length ?? 0);
}

/**
 * GRL-FR-XBUE: how many passes the run's current budget window granted.
 *
 * The window's own size rather than the project's setting, so a run that rested
 * under one budget still reports that one after the author changed it. A run
 * that has not been dispatched has no window, and reports `null` rather than
 * the default — a spend it never made is worse than no number at all.
 */
export function passBudgetOf(run: GraduationRun): number | null {
  const limit = run.checkpoint?.passLimit ?? 0;
  if (limit <= 0) return null;
  const floor = Math.max(1, run.checkpoint?.passFloor ?? 1);
  return Math.max(1, limit - floor + 1);
}

/** RPV-FR-09: the label the condition line carries. */
export function iterationLabel(run: GraduationRun): string | null {
  if (!hasReadableProgress(run.observability)) return null;
  const pass = passOf(run);
  return pass > 1 ? `pass ${pass} of ${passBoundOf(run)}` : null;
}

/**
 * GRU-FR-LBPR / RPV-FR-10: how many times the author sent this run round again.
 *
 * The stage row draws no loop of its own here, because the host renders the
 * review's own revisions in the pass history beneath it. It renders none of the
 * retries either, so without this the one thing an author repeating a Continue
 * cannot see is that they are repeating it.
 */
export function retryStatement(run: GraduationRun): string | null {
  if (!hasReadableProgress(run.observability)) return null;
  const retries = run.observability.stageHistory.filter(
    (entry) => entry.reason === "blocked_retry",
  ).length;
  if (retries === 0) return null;
  return retries === 1
    ? "Sent round again once."
    : `Sent round again ${retries} times.`;
}

/** RPV-FR-10: the moves the row draws its loops from. */
export function stageHistory(run: GraduationRun): ProgressTransition[] {
  if (!hasReadableProgress(run.observability)) return [];
  return run.observability.stageHistory.map((entry) => ({
    from: entry.from,
    to: entry.to,
    iteration: entry.pass,
  }));
}

/** RPV-FR-12: the one line a run that has ended carries. */
export function outcomeStatement(run: GraduationRun): string | null {
  if (run.merge) {
    switch (run.state) {
      case "completed":
        return `Merged into ${run.merge.baseBranch}.`;
      case "discarded":
        return "Discarded. Nothing was written to either branch.";
      case "failed":
        return failureStatement(run);
      default:
        return null;
    }
  }
  switch (run.state) {
    case "completed":
      return run.commits.length === 1
        ? "Committed one change on the work stream."
        : `Committed ${run.commits.length} changes on the work stream.`;
    case "discarded":
      return "Discarded. Nothing it committed was undone.";
    case "failed":
      return run.failure?.message ?? "The run failed.";
    default:
      return null;
  }
}

/** The order the rail lists a project's runs in. */
export function railOrder(runs: GraduationRun[]): GraduationRun[] {
  return runs;
}

/** Whether this state is one the section renders as working. */
export function isWorking(state: GraduationRunState): boolean {
  return state === "working" || state === "reviewing";
}

/** The observability record a surface may render, or null. */
export function readableObservability(
  run: GraduationRun,
): GraduationObservability | null {
  return hasReadableProgress(run.observability) ? run.observability : null;
}
