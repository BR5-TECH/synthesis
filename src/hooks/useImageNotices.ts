import { useEffect, useRef, useState } from "react";

import { listAgentTurnImageNotices } from "../api";
import { onAgentTurnStateChanged } from "../events";
import type { AgentTurn } from "../types";

/**
 * CTA-FR-ARBB: the conversation's **unsupported-image notice** — the most recent
 * turn whose images the selected provider and model could not take.
 *
 * At most one per conversation, so a reader sees the current state of the
 * conversation rather than a history of it (per
 * `../../specifications/core/AGC-agent-conversations.md` AGC-FR-39). Learnt on
 * exactly the terms a recoverable failure is learnt (CTA-FR-RHPP): `imagesOmitted`
 * on the terminal `"agent turn state changed"` event, and
 * `"list agent turn image notices"` when the conversation is mounted or
 * remounted — which is what lets an instance opened after the turn ended still
 * say that the pictures were not sent.
 *
 * It is a **status and not an error**: it blocks nothing, it is held in memory
 * by the rail alone, no log line records it, and no reader of the conversation
 * ever sees it (CTA-FR-OBRO).
 *
 * Held here rather than in each surface for the reason `useRecoverableFailures`
 * is: the rule is one rule, and the rail's anchored cards, a tab's discussions,
 * and a detached conversation must not disagree about whether a notice stands.
 */
export interface ImageNotices {
  /** The current notice per thread id, if any. */
  imageNotices: Readonly<Record<string, AgentTurn>>;
}

export function useImageNotices(
  enabled: boolean,
  /** Whether a turn belongs to the surface asking. */
  isMine: (turn: AgentTurn) => boolean,
): ImageNotices {
  const [imageNotices, setImageNotices] = useState<Record<string, AgentTurn>>({});
  const isMineRef = useRef(isMine);
  isMineRef.current = isMine;

  // One read when the surface mounts. Everything after it arrives on an event,
  // so a conversation carrying no notice costs one call and one carrying a
  // notice costs no polling.
  useEffect(() => {
    if (!enabled) {
      setImageNotices({});
      return;
    }
    let cancelled = false;
    void listAgentTurnImageNotices(null)
      .then((turns) => {
        if (cancelled) return;
        setImageNotices(byThread((turns ?? []).filter(isMineRef.current)));
      })
      .catch(() => {
        // A surface that cannot read the registry still renders its
        // conversation; the notice corrects itself on the next terminal event.
        if (!cancelled) setImageNotices({});
      });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  /**
   * AGC-FR-39: a terminal turn that omitted images enters or replaces this
   * conversation's notice; one that carried its images successfully or carried
   * none at all retires it.
   *
   * A turn that is still `running` says nothing either way — the decision is
   * taken before the request is built and is carried on every payload from then
   * on, so a running turn already knows, and a notice entered while it is still
   * answering is the honest state of the conversation.
   */
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentTurnStateChanged((turn) => {
      if (!isMineRef.current(turn)) return;
      const threadId = turn.origin.discussionId;
      setImageNotices((prev) => {
        if (turn.imagesOmitted) return { ...prev, [threadId]: turn };
        // A later turn that carried its images retires whatever stood there,
        // whichever turn put it there — which is what makes changing the
        // agent's model to one that takes images and asking again clear the
        // notice for that conversation.
        if (!(threadId in prev)) return prev;
        const { [threadId]: _gone, ...rest } = prev;
        return rest;
      });
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
  }, [enabled]);

  return { imageNotices };
}

function byThread(turns: readonly AgentTurn[]): Record<string, AgentTurn> {
  const out: Record<string, AgentTurn> = {};
  for (const turn of turns) out[turn.origin.discussionId] = turn;
  return out;
}
