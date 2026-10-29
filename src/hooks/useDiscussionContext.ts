/**
 * DDS-FR-KXWQ / DDS-FR-TSPL / DDS-FR-HJMF: the figures the discussion column's
 * context indicator shows.
 *
 * The latest turn's reported figures come from the turn events, kept by
 * `../state/turnContexts`. Before any turn, and after a reload of the window,
 * the value is the backend's estimate for the shown discussion, read with the
 * window of the first agent of the column; after a turn, with the window of
 * the agent that answered it. The estimate is read again when the shown
 * discussion or that agent changes and when the document is saved, so the
 * warning follows the document's size.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import { useTurnContext, type HeldTurnContext } from "../state/turnContexts";
import { shownContext, type ShownContext } from "../components/DraftDiscussion/contextFigures";
import {
  discussionTargetKey,
  type DiscussionContext,
  type DiscussionTarget,
} from "../types";

interface DiscussionContextInput {
  target: DiscussionTarget;
  /** The discussion on screen, or `null` while the column shows none. */
  discussionId: string | null;
  /** DDS-FR-TSPL: the first agent of the column's agent list. */
  firstAgentId: string | null;
  /**
   * The document has changes not yet written. A change from `true` to `false`
   * is a save, which reads the estimate again (DDS-FR-HJMF).
   */
  documentDirty: boolean;
}

/** A value the backend returned that the indicator can read. */
function isContext(value: unknown): value is DiscussionContext {
  const v = value as DiscussionContext | null | undefined;
  return typeof v?.estimatedTokens === "number" && typeof v.split === "object";
}

export function useDiscussionContext({
  target,
  discussionId,
  firstAgentId,
  documentDirty,
}: DiscussionContextInput): ShownContext | null {
  const reported = useTurnContext(discussionId);

  // Which of the two arrived last, so the document size shown is the newer one.
  const sequence = useRef(0);
  const reportedMark = useRef<{ held: HeldTurnContext | null; seq: number }>({
    held: null,
    seq: 0,
  });
  if (reportedMark.current.held !== reported) {
    sequence.current += 1;
    reportedMark.current = { held: reported, seq: sequence.current };
  }

  // The estimate keeps the key it was read for. An estimate for another
  // discussion, target, or agent is never shown, also when the new read fails.
  const [estimate, setEstimate] = useState<{
    value: DiscussionContext;
    seq: number;
    key: string;
  } | null>(null);

  // DDS-FR-HJMF: a save is the moment the dirty flag clears.
  const [saves, setSaves] = useState(0);
  const wasDirty = useRef(documentDirty);
  useEffect(() => {
    if (wasDirty.current && !documentDirty) setSaves((n) => n + 1);
    wasDirty.current = documentDirty;
  }, [documentDirty]);

  // DDS-FR-TSPL: the window is the one of the agent that answered the latest
  // turn, so the estimate after a turn is read with that agent's window and
  // the limit and the warning agree with the window shown.
  const agentId = reported?.agentId ?? firstAgentId;
  const targetKey = discussionTargetKey(target);
  const estimateKey = `${targetKey}|${discussionId ?? ""}|${agentId ?? ""}`;
  useEffect(() => {
    let cancelled = false;
    // A promise from the start, so a call that throws is a rejection here.
    Promise.resolve()
      .then(() =>
        api.estimateDiscussionContext({
          target,
          discussionId,
          agentId,
        }),
      )
      .then((value) => {
        if (cancelled || !isContext(value)) return;
        sequence.current += 1;
        setEstimate({ value, seq: sequence.current, key: estimateKey });
      })
      .catch((e) => {
        if (cancelled) return;
        logWarn(["frontend", "ai"], "the discussion context estimate failed", {
          target: target.kind,
          error: String(e),
        });
      });
    return () => {
      cancelled = true;
    };
    // `target` is read through its key, so a new object for the same target
    // reads nothing again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey, discussionId, agentId, saves]);

  const current = estimate?.key === estimateKey ? estimate : null;
  return shownContext(
    reported?.context ?? null,
    current?.value ?? null,
    current !== null && current.seq > reportedMark.current.seq,
  );
}
