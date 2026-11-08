/**
 * GRU-FR-WVDV: the paragraph of a run at rest because its required checks did
 * not pass (`../../../specifications/ui/GRU-graduation-runs.md`). Continue and
 * Discard run stand in the action row beneath, as for every run at rest
 * without an escalation (GRU-FR-TOKG).
 */

import {
  CHECKS_NOT_PASSED_DETAIL,
  CHECKS_NOT_PASSED_HEADLINE,
  checksCauseSentence,
  checksNotPassedRestOf,
  restCheckStatus,
} from "../../state/graduation";
import type { GraduationRun } from "../../types";

export function ChecksNotPassedRest({ run }: { run: GraduationRun }) {
  const rest = checksNotPassedRestOf(run);
  if (!rest) return null;
  return (
    <div className="graduation__warning" data-testid="graduation-checks-rest">
      <p>
        {CHECKS_NOT_PASSED_HEADLINE}.{" "}
        <span data-testid="graduation-checks-rest-cause">{checksCauseSentence(rest.cause)}</span>
      </p>
      {rest.checks.length > 0 && (
        <ul className="graduation__checks" data-testid="graduation-checks-rest-list">
          {rest.checks.map((check, index) => (
            <li key={`${check.name}-${index}`} data-testid="graduation-checks-rest-check">
              <code>{check.name}</code> · {restCheckStatus(check)}
            </li>
          ))}
        </ul>
      )}
      <p className="t-meta" data-testid="graduation-checks-rest-detail">
        {CHECKS_NOT_PASSED_DETAIL}
      </p>
    </div>
  );
}
