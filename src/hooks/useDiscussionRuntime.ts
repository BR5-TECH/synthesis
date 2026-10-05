/**
 * The agent-turn data one discussion surface draws, keyed by the discussion id
 * (`CVP-conversation-presentation.md` CVP-FR-36, CVP-FR-47, CVP-FR-62).
 *
 * The outstanding turns live in the module-level session store
 * (`../state/discussionSession.ts`), merged with the rules of
 * `../state/activeAgents.ts`. They do not live in a component, so a pending
 * placeholder survives a change of owner surface and a tab that closes and
 * opens again. The failed contribution and the unsupported-image notice are
 * read again from the backend by the hooks that own them, so a surface mounted
 * while one stands renders it.
 *
 * Every owner uses the same hook and so reads the same data for one discussion.
 */
import { useCallback, useEffect, useMemo } from "react";

import { cancelAgentTurn, listAgentTurns } from "../api";
import { onAgentTurnStateChanged } from "../events";
import { logDebug } from "../logging";
import {
  getDiscussionSession,
  mergeDiscussionTurn,
  patchDiscussionSession,
  pendingTurnsOf,
  removeDiscussionTurn,
  setDiscussionError,
  setDiscussionTurnFailure,
  setDiscussionTurns,
  useDiscussionSession,
} from "../state/discussionSession";
import { useImageNotices } from "./useImageNotices";
import { useRecoverableFailures } from "./useRecoverableFailures";
import type { AgentTurn } from "../types";

export interface DiscussionRuntime {
  /** CTA-FR-ZOLW: the turns still running. They draw as pending placeholders. */
  pendingTurns: readonly AgentTurn[];
  /** CTA-FR-QTNB: the typed failure of the last turn that failed. */
  turnFailure: string | undefined;
  /** CTA-FR-MGVJ: the current recoverable failure, offered with Retry. */
  failedTurn: AgentTurn | undefined;
  /** CTA-FR-ARBB: the turn whose images the model could not take. */
  imageNotice: AgentTurn | undefined;
  retryingTurnIds: ReadonlySet<string>;
  /** CVP-FR-62: starts one turn from this surface. */
  retryTurn: (turnId: string) => void;
  /** Optimistic: the turn is gone at once and the terminal event follows. */
  cancelTurn: (turnId: string) => void;
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/**
 * `enabled` is false for a surface that is not the active one, or for a caller
 * that supplies every value itself. Nothing is read or followed then.
 */
export function useDiscussionRuntime(
  discussionId: string,
  enabled = true,
): DiscussionRuntime {
  const session = useDiscussionSession(discussionId);

  // CTA-FR-ZOLW: one read per discussion per session. A change of owner
  // re-parents a live conversation and never asks the backend again.
  useEffect(() => {
    if (!enabled || session.turnsLoaded) return;
    let cancelled = false;
    void listAgentTurns(null)
      .then((turns) => {
        if (cancelled) return;
        const mine = (turns ?? []).filter((t) => t.origin.discussionId === discussionId);
        // A turn the event already merged is kept: the event is newer.
        const known = getDiscussionSession(discussionId).turns;
        setDiscussionTurns(discussionId, [
          ...mine.filter((t) => !known.some((k) => k.id === t.id)),
          ...known,
        ]);
      })
      .catch((e: unknown) => {
        logDebug(["frontend"], "outstanding agent turns could not be read", {
          discussionId,
          error: errorText(e),
        });
        if (!cancelled) setDiscussionTurns(discussionId, [], false);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, discussionId, session.turnsLoaded]);

  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentTurnStateChanged((turn) => {
      if (turn.origin.discussionId !== discussionId) return;
      mergeDiscussionTurn(discussionId, turn);
      // CTA-FR-QTNB: a recoverable failure draws as a failed contribution with
      // Retry, so only a failure that cannot be retried is an inline error.
      if (turn.state === "failed" && turn.failure && !turn.retryPermitted) {
        setDiscussionTurnFailure(discussionId, turn.failure);
      }
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [enabled, discussionId]);

  const isMine = useCallback(
    (turn: AgentTurn) => turn.origin.discussionId === discussionId,
    [discussionId],
  );
  const recoverable = useRecoverableFailures(
    enabled,
    isMine,
    useCallback(
      (_thread: string, reason: string) => setDiscussionError(discussionId, reason),
      [discussionId],
    ),
  );
  const notices = useImageNotices(enabled, isMine);

  const pendingTurns = useMemo(() => pendingTurnsOf(session.turns), [session.turns]);

  // CVP-FR-37: the failed mark is session state, so a Comments row reads it.
  const failedTurn = recoverable.failedTurns[discussionId];
  const failed = Boolean(failedTurn);
  useEffect(() => {
    if (!enabled) return;
    patchDiscussionSession(discussionId, { failedResponse: failed });
  }, [enabled, discussionId, failed]);

  const cancelTurn = useCallback(
    (turnId: string) => {
      removeDiscussionTurn(discussionId, turnId);
      void cancelAgentTurn(turnId).catch(() => {});
    },
    [discussionId],
  );

  return {
    pendingTurns,
    turnFailure: session.turnFailure,
    failedTurn: recoverable.failedTurns[discussionId],
    imageNotice: notices.imageNotices[discussionId],
    retryingTurnIds: recoverable.retryingTurnIds,
    retryTurn: (turnId: string) => void recoverable.retryTurn(turnId),
    cancelTurn,
  };
}
