/**
 * The tool list of an agent turn and the activity row a turn leaves at the
 * foot of its discussion (`CTA-comment-agent-turns.md` CTA-FR-BJWN,
 * CTA-FR-ZBIB, CTA-FR-GCKO).
 *
 * Pure, so every rule is testable without a render. The rows themselves live
 * in the discussion session store (CTA-FR-MNPS), which calls these functions.
 * Nothing here logs a tool list or a key input.
 */
import type { AgentTurn, Comment, TurnToolCall } from "../types";

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
  // CTA-FR-ZBIB: a turn that delivered an answer saved its tool list with the
  // comment (CTA-FR-RFQB), so it leaves no row at the foot.
  if (turn.state === "delivered") return null;
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

/**
 * CTA-FR-QXXR, CTA-FR-LZDE: whether a turn of an agent group ends without any
 * failure of its own to show. An advisor turn ends in the Advisors list of the
 * reply it advised, and a turn that the join skipped for time or by
 * cancellation was ended by the group and not by a fault of its agent.
 */
export function isQuietGroupTurn(turn: AgentTurn): boolean {
  const group = turn.group;
  if (!group) return false;
  if (group.role === "advisor") return true;
  return group.skipped === "timed_out" || group.skipped === "cancelled";
}

/**
 * CTA-FR-RFQB: the saved activity row of a comment, read from the comment and
 * from nothing else, or `null` where the comment carries no saved tool list.
 * The ids and the sequence are positions in the saved list: a saved call holds
 * neither (CMS-FR-HRDN).
 */
export function savedActivityRowOf(comment: Comment): ActivityRow | null {
  const saved = comment.toolCalls ?? [];
  if (saved.length === 0 || comment.author.kind !== "agent") return null;
  return {
    turnId: comment.id,
    agentId: comment.author.agentId,
    nickname: comment.author.handle,
    calls: saved.map((call, at) => ({
      id: `${comment.id}:${at}`,
      tool: call.tool,
      activationSeq: at,
      state: call.state,
      keyInput: call.keyInput,
      advisor: call.advisor ?? null,
    })),
  };
}
