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
import type { AgentTurn } from "../types";

export interface PendingContributions {
  /** One running turn for each agent: the newest one. */
  pendingTurns: readonly AgentTurn[];
  /** Cancels every running turn of the agent whose row holds `turnId`. */
  onCancelTurn: (turnId: string) => void;
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
  return { pendingTurns, onCancelTurn };
}
