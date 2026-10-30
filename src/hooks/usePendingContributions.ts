/**
 * CTA-FR-ZOLW, CTA-FR-XMCQ, DDS-FR-TGWY: at most one pending contribution for
 * each agent in a discussion.
 *
 * An agent asked again before it answers runs a second turn (CTA-FR-BXNF). The
 * surface shows the turns of one agent as one row: the row reads the newest
 * turn, and its cancel ends every running turn of that agent there.
 */
import { useCallback, useMemo } from "react";

import { pendingContributionsOf } from "../state/activeAgents";
import { pendingToolCalls } from "../state/turnActivity";
import type { AgentTurn, TurnToolCall } from "../types";

export interface PendingContributions {
  /** One running turn for each agent: the newest one. */
  pendingTurns: readonly AgentTurn[];
  /** Cancels every running turn of the agent whose row holds `turnId`. */
  onCancelTurn: (turnId: string) => void;
  /**
   * CTA-FR-BJWN: the tool list of each row, keyed by the id of the turn the
   * row reads — the calls of every running turn of that agent there.
   */
  toolCallsByRow: ReadonlyMap<string, readonly TurnToolCall[]>;
}

export function usePendingContributions(
  turns: readonly AgentTurn[],
  cancelTurn: (turnId: string) => void,
): PendingContributions {
  const contributions = useMemo(() => pendingContributionsOf(turns), [turns]);
  const pendingTurns = useMemo(
    () => contributions.map((c) => c.turn),
    [contributions],
  );
  const onCancelTurn = useCallback(
    (turnId: string) => {
      const row = contributions.find((c) => c.turn.id === turnId);
      for (const id of row?.turnIds ?? [turnId]) cancelTurn(id);
    },
    [contributions, cancelTurn],
  );
  const toolCallsByRow = useMemo(
    () =>
      new Map(
        contributions.map((c) => [c.turn.id, pendingToolCalls(turns, c.turnIds)] as const),
      ),
    [contributions, turns],
  );
  return { pendingTurns, onCancelTurn, toolCallsByRow };
}
