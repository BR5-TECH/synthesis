/**
 * What a run's state means to the author, and what they may do about it
 * (`../../../specifications/ui/GRU-graduation-runs.md`).
 */

import type { GraduationRun, GraduationRunState } from "../../types/graduation";
import { holdsStream, isTerminalRun } from "../../types/graduation";
import { isMergeRun, mergeProvenanceLine } from "./merge";

export { holdsStream, isTerminalRun };

/** GRU-FR-BHJO: the state, in the author's words. */
export function stateLabel(state: GraduationRunState): string {
  switch (state) {
    case "queued":
      return "Queued";
    case "working":
      return "Working";
    case "reviewing":
      return "In review";
    case "awaiting_author":
      return "Waiting on you";
    case "blocked":
      return "Blocked";
    case "interrupted":
      return "Interrupted";
    case "completed":
      return "Completed";
    case "discarded":
      return "Discarded";
    case "failed":
      return "Failed";
  }
}

/**
 * GRU-FR-YSTV: the state, in the words of the kind of run it is.
 *
 * A merge run reconciles where a draft run works, and a finished one is merged
 * rather than completed.
 */
export function stateLabelOf(run: GraduationRun): string {
  if (isMergeRun(run)) {
    switch (run.state) {
      case "working":
        return "Reconciling";
      case "reviewing":
        return "Reviewing";
      case "completed":
        return "Merged";
      default:
        break;
    }
  }
  return stateLabel(run.state);
}

/** The actions a run's region offers, in the order they are rendered. */
export type RunAction =
  | "open-draft"
  | "pause"
  | "continue"
  | "answer"
  | "revert"
  | "restart"
  | "discard";

/** The order a run's actions render in, whichever of them it offers. */
const ACTION_ORDER: RunAction[] = [
  "open-draft",
  "pause",
  "continue",
  "answer",
  "revert",
  "restart",
  "discard",
];

/**
 * GRU-FR-XQVG / GRU-FR-DVWY / GRU-FR-WDWB: what this run offers now.
 *
 * Every action here is one the backend accepts from that state; a surface that
 * offered more would render a control whose only outcome is a refusal.
 */
export function actionsFor(run: GraduationRun): RunAction[] {
  // GRU-FR-ELFZ: a merge run has no draft, so it offers no way to open one.
  const merge = isMergeRun(run);
  const offered = new Set<RunAction>(merge ? [] : ["open-draft"]);
  switch (run.state) {
    case "queued":
      // GRU-FR-PFBY: a held direct run offers Continue, which offers it a
      // dispatch at once.
      if (run.targetHold) offered.add("continue");
      offered.add("discard");
      break;
    case "working":
    case "reviewing":
      // GRD-FR-MDQZ: the author's own stop is accepted only while the run is
      // doing agent work.
      offered.add("pause").add("discard");
      break;
    case "awaiting_author":
      if (run.escalation) offered.add("answer");
      offered.add("continue").add("discard");
      break;
    case "blocked":
    case "interrupted":
      offered.add("continue").add("discard");
      break;
    case "completed":
      break;
    case "discarded":
      // GRU-FR-WDWB: a discarded merge run offers no Restart.
      if (!merge) offered.add("restart");
      break;
    case "failed":
      offered.add("discard");
      break;
  }
  // GRU-FR-DVWY / GRD-FR-BLCR: revert is offered for every run that made
  // commits, holds no stream, and is not queued — the queue may start a queued
  // run on the reverted branch. A record that names no commits made none.
  // GRU-FR-DVWY: a merge run offers none, whatever it published.
  if (
    !merge &&
    (run.commits ?? []).length > 0 &&
    !holdsStream(run.state) &&
    run.state !== "queued"
  ) {
    offered.add("revert");
  }
  return ACTION_ORDER.filter((action) => offered.has(action));
}

/** GRD-FR-BSNI: whether this run works directly in a pinned worktree. */
export function isDirectRun(run: GraduationRun): boolean {
  return Boolean(run.directTarget);
}

/**
 * GRD-FR-ZVNO: the key of the queue a run waits in. The stream's id where the
 * run has a stream, else its worktree's path.
 */
export function runQueueKey(run: GraduationRun): string {
  if (run.streamId) return run.streamId;
  return run.directTarget ? `worktree:${run.directTarget.worktreePath}` : "";
}

/** GRU-FR-LORX: what the rail and the region name as the run's place. */
export function targetName(run: GraduationRun): string {
  return run.directTarget ? run.directTarget.worktreeName : run.streamName;
}

/** GRU-FR-UKNC: what this run is writing against, in one line. */
export function provenanceLine(run: GraduationRun, streamExists: boolean): string {
  // GRU-FR-NWEC: a merge run names the stream, its branch and the base branch.
  if (run.merge) return mergeProvenanceLine(run);
  const target = run.directTarget;
  if (target) {
    // GRU-FR-LORX: a direct run on an ordinary worktree names no stream.
    const stream = run.streamId ? ` It is the working copy of the work stream “${run.streamName}”.` : "";
    return `Works directly in the worktree “${target.worktreeName}”, on branch “${target.branch}”.${stream}`;
  }
  if (!streamExists) {
    return `Ran in the work stream “${run.streamName}”, which no longer exists.`;
  }
  return `Work stream “${run.streamName}”.`;
}

/**
 * GRU-FR-PFBY: what a queued direct run says about the branch it waits for.
 *
 * It names the branch the run pinned and the branch the worktree holds now, and
 * says when it starts.
 */
export function targetHoldStatement(run: GraduationRun): string | null {
  const hold = run.targetHold;
  if (!hold || run.state !== "queued") return null;
  const now = hold.actualBranch
    ? `the worktree is on “${hold.actualBranch}”`
    : "the worktree is not on a branch";
  return `Waiting for the branch “${hold.expectedBranch}”: ${now}. The run starts when “${hold.expectedBranch}” is checked out again.`;
}

/** GRU-FR-QYEE: what a completed run committed. */
export function commitSummary(run: GraduationRun): string | null {
  // GRU-FR-JRMA: a merge run states its result instead.
  if (run.merge) return null;
  if (run.commits.length === 0) return null;
  return run.commits.length === 1
    ? `One commit on the ${run.directTarget ? "pinned branch" : "work stream"}.`
    : `${run.commits.length} commits on the ${run.directTarget ? "pinned branch" : "work stream"}.`;
}

/** Whether the section renders this run as still going. */
export function isActive(run: GraduationRun): boolean {
  return !isTerminalRun(run.state);
}
