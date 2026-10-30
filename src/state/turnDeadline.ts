/**
 * The whole-turn deadline the timeout bar draws
 * (`CVP-conversation-presentation.md` CVP-FR-JSHF, CVP-FR-IJEY, CVP-FR-ZRWW).
 *
 * Pure, so every rule is testable without a render or a clock. The bar reads
 * the deadline the backend sends on each turn record (AGC-FR-URXV) and draws
 * it on a local clock.
 */
import type { AgentTurn } from "../types";

/** The window of one whole-turn deadline, in epoch milliseconds. */
export interface TurnDeadline {
  startedMs: number;
  endsMs: number;
}

function deadlineOfTurn(turn: AgentTurn): TurnDeadline | null {
  if (turn.state !== "running" || !turn.deadlineStartedAt || !turn.deadlineAt) return null;
  const startedMs = Date.parse(turn.deadlineStartedAt);
  const endsMs = Date.parse(turn.deadlineAt);
  if (Number.isNaN(startedMs) || Number.isNaN(endsMs) || endsMs <= startedMs) return null;
  return { startedMs, endsMs };
}

/**
 * CVP-FR-JSHF / CVP-FR-ZRWW: the deadline the bar shows — that of the running
 * turn with the latest end, which has the most time left. `null` where no turn
 * runs with a deadline, a turn that waits for its slot among them.
 */
export function barDeadlineOf(turns: readonly AgentTurn[]): TurnDeadline | null {
  let best: TurnDeadline | null = null;
  for (const turn of turns) {
    const deadline = deadlineOfTurn(turn);
    if (deadline && (!best || deadline.endsMs > best.endsMs)) best = deadline;
  }
  return best;
}

/**
 * CVP-FR-IJEY: the share of the deadline that is left at `nowMs`, from 100 down
 * to 0 and never outside that range.
 */
export function percentLeft(deadline: TurnDeadline, nowMs: number): number {
  const span = deadline.endsMs - deadline.startedMs;
  const left = ((deadline.endsMs - nowMs) / span) * 100;
  return Math.min(100, Math.max(0, left));
}
