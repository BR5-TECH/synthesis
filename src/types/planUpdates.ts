// The plan fields of a graduation run's rest
// (`../../specifications/core/OPU-openspec-plan-updates.md` contract surface).
// An apply run that asks to change the plan of its change rests on one of
// three plan rest reasons, and the rest holds what the change workspace and
// the Runs section show of it. Wire fields are camelCase.

import type { GraduationProposedResponse } from "./graduation";
import type { QuestionAnswerInput } from "./comments";

/** OPU contract surface: the three rest reasons of a plan rest. */
export type PlanRestReason = "plan_update" | "plan_findings" | "plan_update_invalid";

/** OPU-FR-UOYJ: where a plan update came from. */
export type PlanUpdateOrigin = "work" | "review";

/** OPU-FR-XLVF: why the application refused a plan update. */
export type PlanUpdateRefusalCause =
  | "plan_path_outside_change"
  | "plan_file_not_text"
  | "plan_deletes_file"
  | "plan_update_empty";

/** OPU-FR-UOYJ: one question of a plan update, as the agent gave it. */
export interface PlanUpdateQuestion {
  /** 1-based, in the order the agent gave them. */
  position: number;
  question: string;
  /** 0 to 3 options (CMS-FR-CSCD). */
  options: GraduationProposedResponse[];
}

/**
 * OPU contract surface: the plan fields of `GraduationRest`. Each is present
 * only on the plan rests that hold it.
 */
export interface PlanRestFields {
  /** `plan_update`: `work` or `review`; `plan_findings`: `review`. */
  origin?: PlanUpdateOrigin | null;
  /** `plan_update`, `plan_findings`: the plan thread. */
  threadId?: string | null;
  /** The plan proposal, where edits were recorded or admitted. */
  proposalId?: string | null;
  /** `plan_update`: the question set, where questions were recorded. */
  setId?: string | null;
  /** `plan_update`: the questions as the agent gave them. */
  questions?: PlanUpdateQuestion[] | null;
  /** `plan_update`: the submitted answers, once the set is submitted (OPU-FR-TPYT). */
  answers?: QuestionAnswerInput[] | null;
  /** `plan_update`: the agent's reason. Never logged. */
  planReason?: string | null;
}

/** OPU-FR-EPXL: the kind of a review finding. `work` where it is absent. */
export type ReviewFindingKind = "work" | "plan";

/** OPU contract surface: the typed refusals of a Continue from a plan rest. */
export const PLAN_CONTINUE_ERRORS = {
  planQuestionsPending: "plan_questions_pending",
  proposalPending: "proposal_pending",
  validationUnavailable: "openspec_validation_unavailable",
  writeFailed: "write_failed",
} as const;
