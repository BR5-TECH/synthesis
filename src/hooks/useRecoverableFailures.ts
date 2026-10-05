import { useCallback, useEffect, useRef, useState } from "react";

import { listRecoverableAgentTurnFailures, retryAgentTurn } from "../api";
import { onAgentTurnStateChanged, onDiscussionChanged } from "../events";
import { logDebug, logWarn } from "../logging";
import type { AgentTurn, Discussion } from "../types";

/**
 * CTA-FR-RHPP: the conversation's **current** terminal retryable failure, and the
 * Retry that consumes it.
 *
 * At most one per conversation, so a card exposes Retry for the most recently
 * failed eligible turn and never a choice among several. Read from
 * `"list recoverable agent turn failures"` when the surface mounts and followed
 * by the `"agent turn state changed"` event thereafter, on exactly the terms the
 * surface already follows its outstanding turns (CTA-FR-ZOLW) — which is what lets
 * a card closed and reopened while a failure stands render it again rather than
 * appear to have delivered.
 *
 * Held here rather than in each surface because the rule is one rule: the rail's
 * anchored cards, a tab's discussions, and a detached conversation all show the
 * same offer for the same conversation, and two copies of this logic would be
 * two chances for them to disagree about whether one still stands.
 */
export interface RecoverableFailures {
  /** The current recoverable failure per thread id, if any. */
  failedTurns: Readonly<Record<string, AgentTurn>>;
  /**
   * CTA-FR-QDDG: the turns whose Retry is mid-dispatch, so *that* control is
   * disabled and one activation is one turn however impatiently it is pressed.
   *
   * A set rather than a single id, because two conversations can each carry an
   * offer and retrying one is no reason to disable the other's control.
   */
  retryingTurnIds: ReadonlySet<string>;
  /**
   * CTA-FR-QDDG: start a new turn for the same agent, origin, and trigger comment.
   *
   * Replaces the failed contribution with **Thinking…** by clearing the entry;
   * a synchronous refusal puts it back and reports the typed reason through
   * `onRefused`, so the offer stays available to try again.
   */
  retryTurn: (turnId: string) => Promise<void>;
}

export function useRecoverableFailures(
  enabled: boolean,
  /** Whether a turn belongs to the surface asking. */
  isMine: (turn: AgentTurn) => boolean,
  /**
   * CTA-FR-QDDG / CMT-FR-34: where a synchronous retry refusal is rendered — the
   * foot of the card that triggered it, like every other typed failure.
   */
  onRefused?: (threadId: string, reason: string) => void,
): RecoverableFailures {
  const [failedTurns, setFailedTurns] = useState<Record<string, AgentTurn>>({});
  const [retryingTurnIds, setRetryingTurnIds] = useState<ReadonlySet<string>>(
    EMPTY_RETRYING,
  );
  // The guard itself, read and written synchronously: the state above is what
  // *renders* a control disabled, and React has not re-rendered yet when a
  // second click arrives in the same tick — the disabled attribute closes the
  // window a frame too late to be the guard.
  const retryingRef = useRef<Set<string>>(new Set());

  // Read inside `retryTurn`, which must see the map as it stands at the moment
  // of the click rather than as it stood when its closure was built.
  const failedTurnsRef = useRef(failedTurns);
  failedTurnsRef.current = failedTurns;
  // Read inside subscriptions that must not re-run when the map changes.
  const isMineRef = useRef(isMine);
  isMineRef.current = isMine;
  const onRefusedRef = useRef(onRefused);
  onRefusedRef.current = onRefused;

  // CTA-FR-RHPP: one read when the surface mounts. Everything after it arrives on
  // an event, so a conversation carrying no failure costs one call and one
  // carrying a failure costs no polling.
  useEffect(() => {
    if (!enabled) {
      setFailedTurns({});
      return;
    }
    let cancelled = false;
    void listRecoverableAgentTurnFailures(null)
      .then((turns) => {
        if (cancelled) return;
        const mine = (turns ?? []).filter(isMineRef.current);
        setFailedTurns(byThread(mine));
      })
      .catch(() => {
        // A surface that cannot read the registry still renders its threads; the
        // offer corrects itself on the next event.
        if (!cancelled) setFailedTurns({});
      });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  /**
   * CTA-FR-QTNB / CTA-FR-RHPP: a turn that fails **recoverably** becomes its
   * conversation's offer, replacing whatever stood there; every other terminal
   * state clears one this conversation held.
   *
   * `retryPermitted` rather than the failure value, because `timed_out` names
   * both the retryable provider-call deadline and the whole-turn one that is not
   * (CVL-FR-18) and only the backend knows which a given turn hit.
   */
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentTurnStateChanged((turn) => {
      if (!isMineRef.current(turn)) return;
      const threadId = turn.origin.discussionId;
      setFailedTurns((prev) => {
        if (turn.state === "failed" && turn.retryPermitted) {
          return { ...prev, [threadId]: turn };
        }
        // A later turn in this conversation reaching any other terminal state
        // does not retire the offer — only a turn of its own would, and that
        // turn is the one the branch above replaced it with. What clears an
        // entry here is the turn that *held* it moving on.
        if (prev[threadId]?.id !== turn.id) return prev;
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

  /**
   * CTA-FR-RHPP: a later **human-authored** comment retires the offer; an
   * agent-authored one does not.
   *
   * Read from the thread the write announced rather than from a channel of its
   * own, because the append is already broadcast and its last comment says who
   * wrote it (CMS-FR-11). That also makes a comment posted from another window
   * retire the offer here exactly as one posted in this card does — the backend
   * having retired its own entry on the same rule (AGC-FR-31).
   */
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onDiscussionChanged((thread: Discussion) => {
      setFailedTurns((prev) => {
        const offer = prev[thread.id];
        if (!offer || !retires(thread, offer)) return prev;
        const { [thread.id]: gone, ...rest } = prev;
        logDebug(["ai", "frontend"], "an offer to retry was retired by a comment", {
          threadId: thread.id,
          turnId: gone.id,
        });
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

  const retryTurn = useCallback(async (turnId: string) => {
    // CTA-FR-QDDG: one activation is one turn, however impatiently the control is
    // pressed. Per turn rather than per surface, so a second conversation's
    // offer stays takeable while this one is dispatching.
    if (retryingRef.current.has(turnId)) return;
    retryingRef.current.add(turnId);
    setRetryingTurnIds(new Set(retryingRef.current));

    // The contribution stays put while the dispatch is initiating, which is what
    // gives CTA-FR-QDDG's disabled control something to *be*: a contribution
    // removed on activation takes its own Retry with it, and "disabled while
    // dispatch is being initiated" would describe a control nobody could see.
    // It is replaced by **Thinking…** when the dispatch is accepted — the entry
    // goes then, and the new turn's own `running` event supplies the pending
    // contribution that takes its place.
    //
    // Refusal therefore restores nothing, because nothing was taken away: the
    // offer is exactly where it was, re-enabled, with the typed reason beside
    // it. That is also one fewer frame of flicker than removing and re-adding.
    const offer = failedTurnsRef.current[threadOf(failedTurnsRef.current, turnId) ?? ""];
    try {
      await retryAgentTurn(turnId);
      setFailedTurns((prev) => {
        const found = Object.values(prev).find((t) => t.id === turnId);
        if (!found) return prev;
        const { [found.origin.discussionId]: _gone, ...rest } = prev;
        return rest;
      });
    } catch (e) {
      const reason = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
      const threadId = offer?.origin.discussionId;
      if (threadId) {
        onRefusedRef.current?.(threadId, reason);
        logWarn(["ai", "frontend"], "a retry was refused; the offer still stands", {
          turnId,
          threadId,
          reason,
        });
      }
    } finally {
      retryingRef.current.delete(turnId);
      setRetryingTurnIds(new Set(retryingRef.current));
    }
  }, []);

  return { failedTurns, retryingTurnIds, retryTurn };
}

const EMPTY_RETRYING: ReadonlySet<string> = new Set();

/** The conversation an offer with this turn id belongs to. */
function threadOf(
  offers: Readonly<Record<string, AgentTurn>>,
  turnId: string,
): string | undefined {
  return Object.values(offers).find((t) => t.id === turnId)?.origin.discussionId;
}

function byThread(turns: readonly AgentTurn[]): Record<string, AgentTurn> {
  const out: Record<string, AgentTurn> = {};
  for (const turn of turns) out[turn.origin.discussionId] = turn;
  return out;
}

/**
 * CTA-FR-RHPP: whether this write retires the offer — a **later** human-authored
 * comment, and nothing else.
 *
 * Three things have to be true, and each rules out a case that would otherwise
 * make an offer disappear on its own. The write has to have left a *human* last
 * comment, because an agent answering elsewhere in the thread does not mean the
 * author has stopped wanting the answer that failed. It has to be a comment
 * later than the one that addressed the agent — the failed turn appended nothing
 * (AGC-FR-18), so in the ordinary case the thread's last comment is still the
 * trigger itself, and treating that as "a later comment" would retire every
 * offer the moment it was made. And the identity check is what makes a **lock**
 * or a **resolution** harmless: both announce themselves on this same channel
 * with the comment list untouched (CMS-FR-51), and a locked thread is supposed
 * to lose its Retry control in place rather than lose the whole contribution
 * (CTA-FR-XZUO).
 */
function retires(thread: Discussion, offer: AgentTurn): boolean {
  const last = thread.comments[thread.comments.length - 1];
  if (!last || last.author.kind !== "human") return false;
  return last.id !== offer.triggerCommentId;
}
