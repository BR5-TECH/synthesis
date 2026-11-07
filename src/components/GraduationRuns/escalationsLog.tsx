/**
 * The decisions the author made during one run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-NQLG,
 * GRU-FR-WMTH, GRU-FR-DRPW).
 *
 * One read-only list in the run region, read from the run's decision ledger.
 * It is collapsed by default, its header shows the count, and its rows are
 * plain text. It has no control that changes a decision.
 */

import { useState } from "react";

import type { GraduationDecision, GraduationRun } from "../../types";
import { Icon } from "../icons";

/** GRU-FR-DRPW: the turn a ledger entry names, in words. */
export function decisionTurnLabel(turn: string): string {
  switch (turn) {
    case "work":
      return "Work turn";
    case "review":
      return "Review turn";
    case "merge_work":
      return "Merge work turn";
    case "merge_review":
      return "Merge review turn";
    case "semantic_rebase":
      return "Update turn";
    default:
      return turn;
  }
}

function decisionCount(count: number): string {
  return count === 1 ? "1 decision" : `${count} decisions`;
}

export function EscalationsLog({ run }: { run: GraduationRun }) {
  // GRU-FR-WMTH: collapsed when the run is first selected. The region keys this
  // component by run, so another run starts collapsed again.
  const [open, setOpen] = useState(false);
  const decisions: GraduationDecision[] = run.checkpoint?.decisions ?? [];
  if (decisions.length === 0) return null;
  return (
    <section className="graduation__escalations-log" data-testid="graduation-escalations-log">
      <button
        type="button"
        className="graduation__paths-head t-eyebrow"
        aria-expanded={open}
        data-testid="graduation-escalations-log-toggle"
        onClick={() => setOpen((held) => !held)}
      >
        <span className="graduation__paths-caret" aria-hidden="true">
          {open ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
        </span>
        Escalations log · {decisionCount(decisions.length)}
      </button>
      {open && (
        <ol className="graduation__escalations-rows" data-testid="graduation-escalations-rows">
          {decisions.map((decision, index) => (
            <li
              // The ledger is append-only, so the position is the identity.
              key={index}
              className="graduation__escalations-row"
              data-testid="graduation-escalations-row"
            >
              <p className="graduation__escalations-meta t-meta">
                {decisionTurnLabel(decision.turn)} · pass {decision.pass}
              </p>
              <p className="graduation__escalations-question">{decision.question}</p>
              <p className="graduation__escalations-answer">{decision.answer}</p>
              {decision.note ? (
                <p className="graduation__escalations-note t-meta">Note: {decision.note}</p>
              ) : null}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
