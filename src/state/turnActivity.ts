/**
 * The tool list of an agent turn and the activity row a turn leaves at the
 * foot of its discussion (`CTA-comment-agent-turns.md` CTA-FR-BJWN,
 * CTA-FR-ZBIB, CTA-FR-GCKO).
 *
 * Pure, so every rule is testable without a render. The rows themselves live
 * in the discussion session store (CTA-FR-MNPS), which calls these functions.
 * Nothing here logs a tool list or a key input.
 */
import type { AgentTurn, TurnToolCall } from "../types";

/** CTA-FR-ZBIB: the tool list of one ended turn, at the discussion foot. */
export interface ActivityRow {
  turnId: string;
  agentId: string;
  nickname: string;
  calls: readonly TurnToolCall[];
}

function startedMs(turn: AgentTurn): number {
  const ms = Date.parse(turn.startedAt);
  return Number.isNaN(ms) ? 0 : ms;
}

/** AGC-FR-HMUV: one turn's calls, in activation order. */
export function callsOf(turn: Pick<AgentTurn, "toolCalls">): TurnToolCall[] {
  return [...(turn.toolCalls ?? [])].sort((a, b) => a.activationSeq - b.activationSeq);
}

/**
 * CTA-FR-BJWN: the tool list of a pending contribution — the calls of the
 * agent's running turns named by `turnIds`, the oldest turn first and each
 * turn's calls in activation order.
 */
export function pendingToolCalls(
  turns: readonly AgentTurn[],
  turnIds: readonly string[],
): TurnToolCall[] {
  return turns
    .filter((t) => turnIds.includes(t.id))
    .sort((a, b) => startedMs(a) - startedMs(b))
    .flatMap(callsOf);
}

/**
 * CTA-FR-ZBIB: the activity row an ended turn leaves, or `null` for a turn
 * that made no tool call.
 */
export function activityRowOf(turn: AgentTurn): ActivityRow | null {
  const calls = callsOf(turn);
  if (calls.length === 0) return null;
  return { turnId: turn.id, agentId: turn.agentId, nickname: turn.nickname, calls };
}

/**
 * CTA-FR-ZOGA: add the row of a turn that ended, after the rows already there.
 * A second terminal record for the same turn changes nothing.
 */
export function withEndedTurn(
  rows: readonly ActivityRow[],
  turn: AgentTurn,
): readonly ActivityRow[] {
  if (rows.some((r) => r.turnId === turn.id)) return rows;
  const row = activityRowOf(turn);
  return row ? [...rows, row] : rows;
}

/** CTA-FR-QMWO: the fixed text of an activity row. */
export function activityRowLabel(row: Pick<ActivityRow, "nickname" | "calls">): string {
  const count = row.calls.length;
  return `@${row.nickname} called ${count} ${count === 1 ? "tool" : "tools"}`;
}

/** CTA-FR-BJWN: the word that states a call's state, legible without colour. */
export function toolCallStateLabel(state: TurnToolCall["state"]): string {
  switch (state) {
    case "running":
      return "running";
    case "done":
      return "done";
    case "failed":
      return "failed";
  }
}
