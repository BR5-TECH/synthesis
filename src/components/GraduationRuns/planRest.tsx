/**
 * GRU-FR-TCSM / GRU-FR-TVDF / GRU-FR-PZJY: the paragraph of an apply run at
 * rest on a plan rest (`../../../specifications/ui/GRU-graduation-runs.md`).
 *
 * A plan rest holds no escalation, so it renders no question form
 * (GRU-FR-FZCN): the author answers and decides in the change discussion.
 * Open change, Continue, and Discard run stand in the action row beneath.
 */

import {
  PLAN_UPDATE_INVALID_DETAIL,
  planRestOf,
  planRestStatement,
} from "../../state/graduation";
import type { GraduationRun } from "../../types";

export function PlanRestNotice({ run }: { run: GraduationRun }) {
  const rest = planRestOf(run);
  if (!rest) return null;
  return (
    <div
      className="graduation__warning"
      data-testid="graduation-plan-rest"
      data-reason={rest.reason}
    >
      <p data-testid="graduation-plan-rest-statement">{planRestStatement(rest)}</p>
      {rest.reason === "plan_update_invalid" && (
        <p className="t-meta" data-testid="graduation-plan-rest-detail">
          {PLAN_UPDATE_INVALID_DETAIL}
        </p>
      )}
    </div>
  );
}
