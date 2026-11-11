/**
 * What an apply run at rest on a plan rest says
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-TCSM,
 * GRU-FR-TVDF, GRU-FR-PZJY, GRU-FR-BPNJ, GRU-FR-KWQX;
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-YRZK;
 * `../../../specifications/core/OPU-openspec-plan-updates.md`).
 *
 * A plan rest holds no escalation (OPU-FR-QUST): the author answers its
 * questions and decides its edits in the change discussion, and Continue
 * takes the run on. Pure functions of a run as the backend reports it.
 */

import type { GraduationRest, GraduationRun } from "../../types/graduation";
import {
  PLAN_CONTINUE_ERRORS,
  type PlanRestReason,
  type PlanUpdateRefusalCause,
} from "../../types/planUpdates";

/** OPU contract surface: the reasons of a plan rest. */
export const PLAN_REST_REASONS: readonly PlanRestReason[] = [
  "plan_update",
  "plan_findings",
  "plan_update_invalid",
];

/** A rest whose reason is a plan rest reason. */
export type PlanRest = GraduationRest & { reason: PlanRestReason };

/** Whether this rest is a plan rest. */
export function isPlanRestRecord(rest: GraduationRest | null | undefined): rest is PlanRest {
  return !!rest && (PLAN_REST_REASONS as readonly string[]).includes(rest.reason);
}

/** OPU-FR-RMZN / OPU-FR-QKLX / OPU-FR-CLYW: the plan rest of a run in `awaiting_author`. */
export function planRestOf(run: GraduationRun): PlanRest | null {
  if (run.state !== "awaiting_author") return null;
  return isPlanRestRecord(run.rest) ? run.rest : null;
}

/**
 * GRU-FR-TCSM / GRU-FR-TVDF / OCW-FR-QSZP: the plan thread an Open change
 * selects, or null. A `plan_update_invalid` rest has no plan thread.
 */
export function planThreadOf(run: GraduationRun): string | null {
  const rest = planRestOf(run);
  if (!rest || rest.reason === "plan_update_invalid") return null;
  return rest.threadId || null;
}

/** OCW-FR-YRZK: the words of the strip for each plan rest. */
export const PLAN_REST_STRIP_WORDS: Record<PlanRestReason, string> = {
  plan_update: "waiting for plan decisions",
  plan_findings: "plan findings to settle",
  plan_update_invalid: "plan update refused",
};

/** GRU-FR-TVDF: how many findings of the verdict are `plan` findings. */
export function planFindingCount(rest: GraduationRest): number {
  return (rest.findings ?? []).filter((finding) => finding.kind === "plan").length;
}

/** GRU-FR-PZJY: why the application refused the plan update, in words. */
export function planUpdateCauseSentence(cause: GraduationRest["cause"]): string {
  switch (cause as PlanUpdateRefusalCause | undefined) {
    case "plan_path_outside_change":
      return "It proposed a file outside the folder of its change.";
    case "plan_file_not_text":
      return "It proposed a file that is not text.";
    case "plan_deletes_file":
      return "It proposed to delete a file of the change, and a plan update cannot delete a file.";
    case "plan_update_empty":
      return "It proposed no edit and asked no question.";
    default:
      return "The cause was not recorded.";
  }
}

/** GRU-FR-FZCN / GRU-FR-TCSM / GRU-FR-TVDF / GRU-FR-PZJY: the condition line of a plan rest. */
export function planRestHeadline(rest: PlanRest): string {
  switch (rest.reason) {
    case "plan_update":
      return "Waiting for your plan decisions";
    case "plan_findings":
      return "Waiting for you to settle the plan findings";
    case "plan_update_invalid":
      return "Stopped: the plan update was refused";
  }
}

function findings(count: number): string {
  return count === 1 ? "1 plan finding" : `${count} plan findings`;
}

/** GRU-FR-TCSM / GRU-FR-TVDF / GRU-FR-PZJY: what the rest says, in words. */
export function planRestStatement(rest: PlanRest): string {
  switch (rest.reason) {
    case "plan_update":
      return (
        "The run asks to change the plan of its change. " +
        "Answer its questions and decide its edits in the change discussion, then continue the run."
      );
    case "plan_findings":
      return (
        `The review found that the plan of the change is wrong: ${findings(planFindingCount(rest))}. ` +
        "Settle them in the change discussion, then continue the run."
      );
    case "plan_update_invalid":
      return `The run gave a plan update that the application refused. ${planUpdateCauseSentence(rest.cause)}`;
  }
}

/** GRU-FR-PZJY / OPU-FR-VKBO: what a refused plan update left, and what Continue does. */
export const PLAN_UPDATE_INVALID_DETAIL =
  "Nothing was committed. Continue starts the work turn of the same pass again.";

/**
 * GRU-FR-KWQX: the body of the raise of a plan rest. It names the change, and
 * for `plan_update` and `plan_findings` says that the author decides it in the
 * change discussion.
 */
export function planRaiseStatement(rest: PlanRest, title: string, change: string): string {
  switch (rest.reason) {
    case "plan_update":
      return `“${title}” asks to change the plan of ${change}. Decide it in the change discussion.`;
    case "plan_findings":
      return `The review of “${title}” found the plan of ${change} wrong. Decide it in the change discussion.`;
    case "plan_update_invalid":
      return `“${title}” gave a plan update for ${change} that was refused.`;
  }
}

/**
 * GRU-FR-BPNJ / OCW-FR-ZPDC: a refused Continue of a plan rest, in words, or
 * null for a code that is not a refusal of a plan Continue.
 */
export function planContinueRefusal(code: string, detail: string | null): string | null {
  switch (code) {
    case PLAN_CONTINUE_ERRORS.planQuestionsPending:
      return "Answer the questions in the change discussion first, then continue the run.";
    case PLAN_CONTINUE_ERRORS.proposalPending:
      return "Decide the plan edits in the change discussion first, or their commit is still owed. Then continue the run.";
    case PLAN_CONTINUE_ERRORS.validationUnavailable:
      return detail
        ? `OpenSpec could not be read (${detail}), so the run did not continue.`
        : "OpenSpec could not be read, so the run did not continue.";
    case PLAN_CONTINUE_ERRORS.writeFailed:
      return "The run could not be made ready to continue. Nothing changed; try Continue again.";
    default:
      return null;
  }
}
