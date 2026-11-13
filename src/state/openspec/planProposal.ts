/**
 * The plan proposal of an apply run at rest on a plan rest, as the change
 * workspace reads it
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-XJMR,
 * OCW-FR-KZDG; `../../../specifications/core/OPU-openspec-plan-updates.md`
 * OPU-FR-EVBZ).
 *
 * A proposal is the plan proposal of a run where the run's rest names it. The
 * rest also names the plan thread and the question set. While that set stands
 * in the plan thread, the hunks of the plan proposal take no decision. Once it
 * is gone, the plan thread shows the hint that the edits are decided with the
 * chips, or by voice or comment.
 */
import { questionSetOf, useQuestionSet } from "../questionSets";
import { isPlanRestRecord, type PlanRest } from "../graduation/planRest";
import { useChangeRunQueue } from "./changeRunQueue";
import type { GraduationRun, PendingQuestionSet } from "../../types";

/** The run and the plan rest that name a proposal as their plan proposal. */
export interface PlanProposalRest {
  run: GraduationRun;
  rest: PlanRest;
}

/** OPU-FR-UOYJ / OPU-FR-WOKJ: the plan rest whose plan proposal is `proposalId`, or null. */
export function planRestForProposal(
  runs: readonly GraduationRun[],
  streamId: string,
  change: string,
  proposalId: string | null,
): PlanProposalRest | null {
  if (!proposalId) return null;
  for (const run of runs) {
    if (run.streamId !== streamId || run.openspec?.change !== change) continue;
    const rest = run.rest;
    if (isPlanRestRecord(rest) && rest.proposalId === proposalId) return { run, rest };
  }
  return null;
}

/**
 * OCW-FR-XJMR / OPU-FR-EVBZ: whether the question set of the plan update
 * stands in its plan thread.
 *
 * `set` is what the plan thread holds: `undefined` while it is not read yet.
 * Before the read answers, a run that still waits on the author with a set id
 * and no answers reads as one whose set stands, so a chip is never offered for
 * a moment that the backend would refuse.
 */
export function planSetStands(
  found: PlanProposalRest,
  set: PendingQuestionSet | null | undefined,
): boolean {
  const { run, rest } = found;
  if (rest.reason !== "plan_update" || !rest.setId) return false;
  if (set === undefined) {
    return run.state === "awaiting_author" && (rest.answers ?? []).length === 0;
  }
  return set !== null && set.setId === rest.setId;
}

/** What the change workspace reads of one proposal as a plan proposal. */
export interface PlanProposalView {
  /** OCW-FR-XJMR: the chips and Reject all are unavailable. */
  held: boolean;
  /** OCW-FR-KZDG: the plan thread holds no question set. */
  threadIdle: boolean;
  /** The plan thread, where the proposal is a plan proposal. */
  threadId: string | null;
}

const NONE: PlanProposalView = { held: false, threadIdle: false, threadId: null };

/**
 * OCW-FR-XJMR / OCW-FR-KZDG: the plan view of one proposal of the change.
 * It reads the runs the change workspace already reads, and the question set
 * of the plan thread once.
 */
export function usePlanProposal(
  streamId: string,
  change: string,
  proposalId: string | null,
): PlanProposalView {
  const runs = useChangeRunQueue(false);
  const found = planRestForProposal(runs, streamId, change, proposalId);
  const threadId = found?.rest.threadId || null;
  // Subscribes, and reads the set of the plan thread once.
  useQuestionSet(threadId);
  if (!found || !threadId) return NONE;
  const set = questionSetOf(threadId);
  return {
    held: planSetStands(found, set),
    threadIdle: set === null,
    threadId,
  };
}
