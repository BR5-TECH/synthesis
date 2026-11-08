/**
 * CTA-FR-UGBA, CTA-FR-QOLD: whether a posted comment is dispatched as an agent
 * group.
 *
 * The switch lives in the backend, per project and per machine (PSS-FR-CQML), and
 * the Settings window is another window, so this module keeps a copy that the
 * shell refreshes when the project opens and when the window regains focus. A
 * dispatch reads the copy synchronously, which keeps a dispatch with the switch
 * off exactly the dispatch it was without the feature.
 */
import { useEffect } from "react";

import { dispatchAgentGroup, dispatchAgentTurn, loadAgentGroupSettings } from "../api";
import { logDebug } from "../logging";
import type { AgentTurn, ConversationOrigin } from "../types";

let enabled = false;

/** CTA-FR-UGBA: the switch as the last refresh read it. Off until one reads it. */
export function agentGroupSwitchOn(): boolean {
  return enabled;
}

/** Test seam: the switch off again, as at launch. */
export function resetAgentGroupSwitch(): void {
  enabled = false;
}

/**
 * Read the switch from the backend. A read that fails leaves the switch off,
 * because a dispatch that cannot tell must behave as it did without the feature.
 */
export async function refreshAgentGroupSwitch(): Promise<void> {
  try {
    const settings = await loadAgentGroupSettings();
    const next = settings?.enabled === true;
    if (next !== enabled) {
      logDebug(["ai", "frontend"], "the agent group switch changed", { enabled: next });
    }
    enabled = next;
  } catch {
    enabled = false;
  }
}

/** Keeps the switch fresh for the life of the shell that mounts it. */
export function useAgentGroupSwitch(projectOpen: boolean): void {
  useEffect(() => {
    if (!projectOpen) {
      enabled = false;
      return;
    }
    void refreshAgentGroupSwitch();
    const onFocus = () => void refreshAgentGroupSwitch();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [projectOpen]);
}

export interface DispatchHandlers {
  /** A turn the backend registered, or the running turn that took the comment. */
  onTurn: (turn: AgentTurn) => void;
  /** A refusal. `nickname` is null where a group call as a whole is refused. */
  onError: (error: unknown, nickname: string | null) => void;
}

/**
 * CTA-FR-RPVU, CTA-FR-UGBA: dispatch one posted comment to the agents it
 * reaches. With the switch off it is one `"dispatch agent turn"` per agent. With
 * it on it is one `"dispatch agent group"`, whose turns the backend registers.
 */
export function dispatchComment(
  args: {
    nicknames: readonly string[];
    origin: ConversationOrigin;
    triggerCommentId: string;
  },
  handlers: DispatchHandlers,
): void {
  const { nicknames, origin, triggerCommentId } = args;
  if (agentGroupSwitchOn()) {
    void dispatchAgentGroup({ nicknames: [...nicknames], origin, triggerCommentId })
      .then((group) => {
        group.turns.forEach(handlers.onTurn);
        group.refusals.forEach((r) => handlers.onError(r.failure, r.nickname));
      })
      .catch((e: unknown) => handlers.onError(e, null));
    return;
  }
  for (const nickname of nicknames) {
    void dispatchAgentTurn({ nickname, origin, triggerCommentId })
      .then(handlers.onTurn)
      .catch((e: unknown) => handlers.onError(e, nickname));
  }
}
