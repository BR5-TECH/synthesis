/**
 * The comment rail's data lifecycle (`CMT-comments.md`).
 *
 * Owns the artifact's threads: loading them when it opens (CMT-FR-04), keeping
 * each anchor pointed at the right passage as the buffer changes (CMT-FR-20),
 * persisting a drifted anchor on save (CMT-FR-21), and resolving the identity the
 * rail writes as (CMT-FR-24).
 *
 * The anchoring arithmetic lives in `../state/commentAnchors`, kept pure. This
 * hook is the part that has to talk to the backend and to React.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  addComment as addCommentApi,
  cancelAgentTurn,
  dispatchAgentTurn,
  listAgentTurns,
  listDiscussions,
  listProjectAgents,
  openDiscussion,
  reanchorDiscussionFragment,
  resolveCommentAuthorIdentity,
  setDiscussionLock,
  setDiscussionResolution,
} from "../api";
import {
  onAgentTurnStateChanged,
  onDiscussionChanged,
  onGithubTokensChanged,
} from "../events";
import { useRecoverableFailures } from "./useRecoverableFailures";
import { useImageNotices } from "./useImageNotices";
import {
  dispatchTargets,
  withoutAwaitingReply,
  mergeKnownTurn,
} from "../state/activeAgents";
import { addressesEveryone, agentRoster } from "../components/agentTags";
import { logDebug, logWarn } from "../logging";
import { hasTurnEnded } from "../state/discussionSession";
import { loggableTurnFailure } from "../components/CommentRail/messages";
import { publishThread, publishThreads } from "../state/conversationThreads";
import { originArtifactId, originFor, originKind } from "../state/discussionOrigin";
import {
  diffEdit,
  driftedAnchors,
  resolveAnchor,
  resolveThreads,
  shiftThreads,
  unresolvedCount,
  type AnchoredThread,
} from "../state/commentAnchors";
import {
  COMMENT_IDENTITY_ERRORS,
  type AgentTurn,
  type AttachmentInput,
  type CommentQuote,
  type Discussion,
  type FragmentTarget,
  isFragmentTargeted,
  type Participant,
  type ProjectAgent,
} from "../types";

/**
 * The key a draft card's error is filed under. A draft has no thread id yet, so
 * it needs a reserved one that cannot collide with a real thread id.
 */
export const DRAFT_KEY = "__draft__";

/** Why the rail cannot accept a comment, and what the user does about it. */
export interface IdentityState {
  identity: Participant | null;
  /** The typed error the last resolution rejected with, when it did. */
  error: string | null;
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export interface UseCommentsResult {
  threads: AnchoredThread[];
  /**
   * CTA-FR-VQFJ: the agents this project enrolled, which is what the composer's
   * mention picker offers. Empty in a project that has enrolled none, which
   * renders a composer with no picker rather than an error.
   */
  agents: ProjectAgent[];
  /**
   * CTA-FR-ZOLW: the turns outstanding in this artifact's threads. Read from
   * `"list agent turns"` when the artifact opens and followed by the
   * `"agent turn state changed"` event thereafter, so a card reopened while an
   * agent is still thinking shows what is still coming.
   */
  /**
   * Every turn this tab knows to be running, unfiltered — so a second
   * conversational surface in the same tab can filter the same snapshot by its
   * own origin rather than issuing an application-wide read of its own
   * (`ACT-action-control.md` ACT-FR-22).
   */
  allTurns: readonly AgentTurn[];
  pendingTurns: AgentTurn[];
  /** CTA-FR-QTNB: the typed failure of the last turn that failed, per thread. */
  turnFailures: Record<string, string>;
  /**
   * CTA-FR-RHPP: the conversation's current terminal **retryable** failure, per
   * thread — what a card renders as a failed contribution carrying Retry.
   */
  failedTurns: Readonly<Record<string, AgentTurn>>;
  /**
   * CTA-FR-ARBB: the conversation's current unsupported-image notice, per thread
   * — the turn whose images the selected provider and model could not take.
   */
  imageNotices: Readonly<Record<string, AgentTurn>>;
  /** CTA-FR-QDDG: the turns whose Retry is mid-dispatch, so those controls are off. */
  retryingTurnIds: ReadonlySet<string>;
  /** CTA-FR-QDDG: start a fresh turn for a failed one. */
  retryTurn: (turnId: string) => Promise<void>;
  /** CTA-FR-ZOLW: abandon one outstanding turn. */
  cancelTurn: (turnId: string) => Promise<void>;
  identity: Participant | null;
  identityError: string | null;
  /** CMT-FR-29: unresolved threads, orphaned ones included. */
  unresolved: number;
  errors: Record<string, string>;
  openThread: (
    fragmentTarget: FragmentTarget,
    body: string,
    attachments: AttachmentInput[],
  ) => Promise<Discussion>;
  reply: (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<void>;
  setLock: (threadId: string, locked: boolean) => Promise<void>;
  setResolved: (threadId: string, resolved: boolean) => Promise<void>;
  /** CMT-FR-20: track anchors through an edit that took `before` to `after`. */
  noteBufferChange: (before: string, after: string) => void;
  /** CMT-FR-18/CMT-FR-22: re-resolve every anchor against freshly loaded content. */
  reanchorAll: (content: string) => void;
  /** CMT-FR-21: persist anchors that drifted. Called after a successful save. */
  persistDrift: () => Promise<void>;
  retryIdentity: () => Promise<void>;
}

export function useComments(
  artifactId: string | undefined,
  /** The artifact's current Markdown source, for the initial anchor resolution. */
  initialContent: () => string,
  enabled: boolean,
): UseCommentsResult {
  const [threads, setThreads] = useState<AnchoredThread[]>([]);
  const [identityState, setIdentityState] = useState<IdentityState>({
    identity: null,
    error: null,
  });
  const [errors, setErrors] = useState<Record<string, string>>({});
  // Read inside async callbacks that must not re-subscribe on every keystroke.
  const contentRef = useRef(initialContent);
  contentRef.current = initialContent;
  const threadsRef = useRef<AnchoredThread[]>(threads);
  threadsRef.current = threads;

  // CTA-FR-VQFJ / AGT-FR-25: the agents this project enrolled, loaded once. The
  // picker filters what is already here and issues nothing per keystroke.
  const [agents, setAgents] = useState<ProjectAgent[]>([]);
  /**
   * Every turn this rail has heard of. Filtered to *this artifact's* threads at
   * render time rather than on arrival, because the read happens while the
   * thread list may still be loading — filtering then would drop exactly the
   * turns a freshly-opened artifact is supposed to show (CTA-FR-ZOLW).
   */
  const [knownTurns, setKnownTurns] = useState<AgentTurn[]>([]);
  // CTA-FR-QTNB: a turn that failed renders its typed failure at the foot of its
  // card, and leaves every comment in the thread untouched.
  const [turnFailures, setTurnFailures] = useState<Record<string, string>>({});

  const applyThreads = useCallback((next: Discussion[]) => {
    // CVP-FR-53: a presentation whose owning surface is mounted takes the
    // conversation from the list that surface already performed and issues no
    // read of its own, so moving a conversation out of a rail that is still open
    // costs nothing.
    publishThreads(next);
    setThreads(resolveThreads(contentRef.current(), next));
  }, []);

  // CMT-FR-04: one load when the artifact opens. Everything after that arrives
  // on the event — a thread a *different process* appends becomes visible when
  // the artifact is next opened, nothing watching the logs (CMS-FR-31).
  useEffect(() => {
    if (!artifactId || !enabled) {
      setThreads([]);
      return;
    }
    let cancelled = false;
    void listDiscussions({ kind: "artifact", artifactId })
      .then((loaded) => {
        if (!cancelled) applyThreads(loaded.filter(isFragmentTargeted));
      })
      .catch(() => {
        if (!cancelled) setThreads([]);
      });
    return () => {
      cancelled = true;
    };
  }, [artifactId, enabled, applyThreads]);

  // CTA-FR-VQFJ: read the project's enrolled agents when the rail comes up. A
  // project enrolling none simply offers no picker — not an error, and not a
  // reason to withhold the composer.
  useEffect(() => {
    if (!enabled) {
      setAgents([]);
      return;
    }
    let cancelled = false;
    void listProjectAgents()
      .then((list) => {
        if (!cancelled) setAgents(list ?? []);
      })
      .catch(() => {
        if (!cancelled) setAgents([]);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  /**
   * CTA-FR-ZOLW: what is still coming, from `"list agent turns"` when the artifact
   * opens and from the event thereafter.
   *
   * The origin is what scopes it to this rail. `list_agent_turns` is asked for
   * everything rather than per thread, because the rail holds many threads and
   * one call is what a mount should cost; the filter below is the same one the
   * event handler applies, so the two cannot disagree about what belongs here.
   */
  const isArtifactTurn = (turn: AgentTurn) =>
    originKind(turn.origin) === "artifact_comment" ||
    // A discussion about THIS artifact is the other conversation this tab hosts
    // (ACT-FR-22). Its turns are kept here so `allTurns` is genuinely every turn
    // the tab knows, which is what the action control filters by its own origin;
    // `pendingTurns` below still narrows to the threads the rail itself holds,
    // so keeping them costs the anchored cards nothing.
    (originKind(turn.origin) === "artifact_discussion" &&
      originArtifactId(turn.origin) === artifactId);

  useEffect(() => {
    if (!artifactId || !enabled) {
      setKnownTurns([]);
      return;
    }
    let cancelled = false;
    void listAgentTurns(null)
      .then((turns) => {
        if (!cancelled) setKnownTurns((turns ?? []).filter(isArtifactTurn));
      })
      .catch(() => {
        // A rail that cannot read the in-flight set still renders its threads;
        // the pending contributions correct themselves on the next event.
        if (!cancelled) setKnownTurns([]);
      });
    return () => {
      cancelled = true;
    };
  }, [artifactId, enabled]);

  /**
   * CTA-FR-QTNB: a turn that delivers replaces its pending contribution with the
   * agent's comment; one that fails or is cancelled removes it, leaving every
   * comment in the thread untouched.
   *
   * Only the *turn's* lifecycle is handled here. The agent's comment itself
   * arrives on `"discussion changed"` like any other append (CMS-FR-51), so
   * this handler does not re-read: it clears the pending contribution and lets
   * the thread event supply the line.
   */
  useEffect(() => {
    if (!artifactId || !enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentTurnStateChanged((turn) => {
      if (!isArtifactTurn(turn)) return;
      // CTA-FR-ZOLW: replaced where it already stands rather than moved to the
      // end, so the many activity events a running turn now emits (AGC-FR-34)
      // do not reorder the pending contributions a card is showing.
      setKnownTurns((prev) => mergeKnownTurn(prev, turn));
      // CTA-FR-QTNB: filed for this rail's OWN threads only, which is narrower
      // than the set of turns kept above. A discussion's turns are kept so that
      // `allTurns` is complete for the action control (ACT-FR-22), but its
      // failures belong to the control — and the control is also the only thing
      // that clears one, a retry clearing the failure of the thread it
      // dispatched for. A copy recorded here would be permanent: nothing in this
      // hook ever dispatches for a discussion, so nothing here would ever clear
      // it, and the card would go on reporting a failure already retried past.
      //
      // CTA-FR-QTNB: a **recoverable** failure is not one of these. It replaces
      // the pending contribution with a failed contribution carrying Retry
      // (CTA-FR-MGVJ), which `useRecoverableFailures` holds; rendering it inline
      // here as well would say the same thing twice, once with an offer to act
      // on it and once without.
      if (
        originKind(turn.origin) === "artifact_comment" &&
        turn.state === "failed" &&
        turn.failure &&
        !turn.retryPermitted
      ) {
        setTurnFailures((prev) => ({
          ...prev,
          [turn.origin.discussionId]: turn.failure as string,
        }));
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
  }, [artifactId, enabled]);


  /**
   * CTA-FR-MGVJ / CTA-FR-RHPP: the offer to ask again, for this rail's own threads.
   *
   * A refusal is rendered where every other typed failure is — the foot of the
   * card that triggered it (CMT-FR-34).
   */
  /**
   * CTA-FR-ARBB: and the notice a turn that could not send its pictures leaves,
   * read and followed on exactly the same terms and narrowed to the same
   * conversations.
   */
  const isOwnComment = useCallback(
    (turn: AgentTurn) => originKind(turn.origin) === "artifact_comment",
    [],
  );
  const notices = useImageNotices(Boolean(artifactId) && enabled, isOwnComment);

  const recoverable = useRecoverableFailures(
    Boolean(artifactId) && enabled,
    // This rail's OWN threads, which is narrower than `isArtifactTurn` — for
    // exactly the reason `turnFailures` narrows the same way above. A discussion
    // about this artifact is the *other* conversation this tab hosts, and its
    // offers belong to the control that dispatches for it: nothing in this hook
    // ever retries a discussion, so a copy kept here would never be cleared by
    // the retry that consumed it and the card would go on offering a turn
    // already asked again — beside the **Thinking…** of the turn that replaced
    // it (CTA-FR-MGVJ).
    useCallback(
      (turn: AgentTurn) => originKind(turn.origin) === "artifact_comment",
      [],
    ),
    useCallback((threadId: string, reason: string) => {
      setErrors((prev) => ({ ...prev, [threadId]: reason }));
    }, []),
  );

  /**
   * CTA-FR-RPVU / CTA-FR-QUXJ: the comment is appended first and unconditionally;
   * only then is a turn dispatched, once per **distinct** agent it is to be
   * answered by — the agents its own tags resolve to, or failing those the
   * conversation's active agents, or nobody. The author's message stands whether
   * or not any agent answers it, so a refused dispatch never unwinds the post —
   * it renders as that thread's turn failure.
   *
   * CTA-FR-DGOC: an anchored thread routes on exactly the terms a discussion does.
   * A remark pinned to a line is a conversation the author is holding with
   * somebody as much as a discussion is, and an author who has said who they are
   * talking to should not have to say it again because of what the thread is
   * anchored to.
   *
   * CTA-FR-ZVKL: nothing here waits on, or cancels, a turn already outstanding.
   */
  const dispatchFor = useCallback(
    (thread: Discussion, body: string) => {
      // AGT-FR-36: `@all` stands for the agents that can *answer*, so the
      // enrolment is handed over whole rather than flattened to names — the ready
      // subset is what the handle reads and only this shape carries it.
      const roster = agentRoster(agents);
      const nicknames = dispatchTargets(thread, body, roster);
      if (nicknames.length === 0) {
        // CTA-FR-QUXJ: an ordinary outcome — a comment in a conversation with no
        // active agent reaches nobody, and the author's message stands. Recorded
        // because a conversation that answered nothing is otherwise
        // indistinguishable from one whose dispatch was lost. Never the body: a
        // comment is user content, and the buffer is exported into bug reports.
        logDebug(["ai", "frontend"], "a comment reached no agent", {
          threadId: thread.id,
          enrolled: roster.nicknames.length,
        });
        return;
      }
      const triggerCommentId = thread.comments[thread.comments.length - 1]?.id;
      if (!triggerCommentId) return;
      // AGT-FR-35: the backend logs each turn it registers, so what those records
      // cannot show is that one `@all` rather than several nicknames is why there
      // are three of them. Emitted only when the handle actually expanded, so an
      // ordinary tagged comment adds nothing to the buffer. Never the body: a
      // comment is user content, and the buffer is exported into bug reports.
      if (addressesEveryone(body, roster)) {
        logDebug(["ai", "frontend"], "a comment addressed every agent that can answer", {
          threadId: thread.id,
          agents: nicknames.join(" "),
          count: nicknames.length,
        });
      }
      // CTA-FR-JQDM / AGC-FR-29: dispatching is what ends the wait, and the
      // backend retires its own registration silently. Dropped here so the turn
      // list this rail reports matches what `"list agent turns"` would now
      // return; nothing about routing turns on it.
      setKnownTurns((prev) => withoutAwaitingReply(prev, nicknames, thread.id));
      // CTA-FR-UUXA: a new message retires the refusal the last one met, once
      // for the whole batch. Cleared on each accepted dispatch instead, one
      // agent's acceptance would hide another agent's refusal in the same batch.
      setTurnFailures((prev) => {
        if (!(thread.id in prev)) return prev;
        const next = { ...prev };
        delete next[thread.id];
        return next;
      });
      for (const nickname of nicknames) {
        void dispatchAgentTurn({
          nickname,
          origin: originFor(thread),
          triggerCommentId,
        })
          .then((turn) => {
            // Upsert rather than append: the backend publishes the
            // registration event before `dispatch_agent_turn` resolves, and
            // there is no ordering between the two — a blind append renders the
            // same outstanding turn as two pending contributions. CVP-FR-HWTN:
            // a turn that already ended is not added again.
            if (hasTurnEnded(turn.id)) return;
            setKnownTurns((prev) => [
              ...prev.filter((t) => t.id !== turn.id),
              turn,
            ]);
          })
          .catch((e) => {
            setTurnFailures((prev) => ({ ...prev, [thread.id]: errorText(e) }));
            // Never the raw error: it can quote the argument it refused (see
            // `loggableTurnFailure`).
            logWarn(["ai", "frontend"], "an agent turn could not be dispatched", {
              threadId: thread.id,
              nickname,
              failure: loggableTurnFailure(errorText(e)),
            });
          });
      }
    },
    [agents],
  );

  const cancelTurn = useCallback(async (turnId: string) => {
    // Optimistic: the turn is gone from the card the moment the author asks,
    // and the terminal event that follows removes it again harmlessly.
    setKnownTurns((prev) => prev.filter((t) => t.id !== turnId));
    await cancelAgentTurn(turnId).catch(() => {});
  }, []);

  // CMT-FR-24: resolve who the rail writes as before the author types anything,
  // so the compose affordance can be disabled with a reason rather than failing
  // at the moment they try to post.
  const loadIdentity = useCallback(async () => {
    if (!enabled) return;
    try {
      const identity = await resolveCommentAuthorIdentity();
      setIdentityState({ identity, error: null });
    } catch (e) {
      setIdentityState({ identity: null, error: errorText(e) });
    }
  }, [enabled]);

  useEffect(() => {
    void loadIdentity();
  }, [loadIdentity]);

  // GTS-FR-AEQO: a token or a binding changed, so the identity this surface
  // writes as may have changed with it.
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onGithubTokensChanged(() => {
      void loadIdentity();
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
  }, [enabled, loadIdentity]);

  const noteError = useCallback((threadId: string, e: unknown) => {
    setErrors((prev) => ({ ...prev, [threadId]: errorText(e) }));
  }, []);

  const clearError = useCallback((threadId: string) => {
    setErrors((prev) => {
      if (!(threadId in prev)) return prev;
      const next = { ...prev };
      delete next[threadId];
      return next;
    });
  }, []);

  /**
   * Fold one command's returned thread back into the rail.
   *
   * The backend returns the whole thread, so this replaces rather than patches —
   * which is what keeps the rail honest when another participant's event landed
   * in the same log between the read and the write.
   *
   * CMT-FR-61: a discussion is not one of these, whatever produced it. This hook
   * lists and follows the artifact's *anchored* log alone, and an operation
   * naming a thread id resolves against either log at the backend (CMS-FR-55),
   * so a caller that hands a discussion to one of the commands here gets the
   * discussion back — and folding it in would leave the tab counting one
   * conversation twice (CMT-FR-56) and rendering it as an orphan among the
   * aligned cards. Refused at the one door every path goes through, on the same
   * terms as the event handler above.
   */
  const mergeThread = useCallback((updated: Discussion) => {
    publishThread(updated);
    if (!isFragmentTargeted(updated)) return;
    setThreads((prev) => {
      const index = prev.findIndex((t) => t.thread.id === updated.id);
      if (index === -1) {
        // A thread this rail has not seen — it arrived while a reload was in
        // flight. Resolve just this one; every other entry keeps the anchor it
        // has been tracking, which re-resolving them all would throw away.
        return [
          ...prev,
          {
            thread: updated,
            // A thread the rail can render here is anchored by construction —
            // this hook lists and follows anchored threads alone (CMT-FR-04).
            anchor:
              updated.fragmentTarget === null
                ? null
                : resolveAnchor(contentRef.current(), updated.fragmentTarget),
          },
        ];
      }
      const next = [...prev];
      // Keep the *live* anchor: the buffer may have moved on since the backend
      // last heard about it, and adopting the stored one would jump the card
      // back to where the passage used to be.
      next[index] = { thread: updated, anchor: next[index].anchor };
      return next;
    });
  }, []);

  /**
   * CMT-FR-04: every append this application performs redraws the card it landed
   * in, from the payload rather than from a re-read.
   *
   * One subscription covers every source — a comment posted in this card, one
   * posted from another Editor tab on the same artifact, a lock or a resolution
   * set anywhere in the window, and an agent's answer arriving. The rail
   * therefore issues no operation at all between the artifact's first read and
   * the next time it is opened.
   *
   * Filtered to this artifact's own threads: the event is application-wide, and
   * a thread belonging to a different artifact has nothing to do with this rail.
   * A thread this rail has never seen is admitted only when it names *this*
   * artifact, which is what makes a thread opened in a second tab appear here.
   */
  useEffect(() => {
    if (!artifactId || !enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onDiscussionChanged((thread) => {
      if (thread.target.kind !== "artifact" || thread.target.artifactId !== artifactId) return;
      // CMS-FR-53: an artifact's DISCUSSION carries this same `artifactId`, and
      // it is not one of this rail's anchored threads — it is read through
      // `useDiscussions` and rendered in the Discussion section (CMT-FR-53).
      // Merging it here would put one thread in two lists: two cards once it
      // resolved, a duplicate key, and a count that changed on the first event.
      if (!isFragmentTargeted(thread)) return;
      mergeThread(thread);
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
  }, [artifactId, enabled, mergeThread]);

  const openThread = useCallback(
    async (
      fragmentTarget: FragmentTarget,
      body: string,
      attachments: AttachmentInput[],
    ): Promise<Discussion> => {
      if (!artifactId) throw new Error("no artifact is open");
      try {
        const created = await openDiscussion({
          target: { kind: "artifact", artifactId },
          fragmentTarget,
          body,
          attachments,
        });
        clearError(DRAFT_KEY);
        // Upsert rather than append, for the reason the turn registration above
        // is: `open_comment_thread` emits `"discussion changed"` from inside
        // the command, so the event can arrive before this `await` resolves and
        // there is no ordering between the two. A blind append would render the
        // same new thread as two cards with one id.
        //
        // Only the new thread is resolved from content. Re-resolving every entry
        // would discard the anchors the rail has been tracking through the
        // author's edits and re-derive them from the backend's stale offsets —
        // which, on a quote that appears more than once, can land a card on the
        // wrong occurrence (the nearest-to-a-stale-offset trap CMT-FR-18's rule
        // exists to avoid).
        mergeThread(created);
        // CTA-FR-RPVU: the comment stands first; the turns follow it.
        dispatchFor(created, body);
        return created;
      } catch (e) {
        setErrors((prev) => ({ ...prev, [DRAFT_KEY]: errorText(e) }));
        throw e;
      }
    },
    [artifactId, clearError, dispatchFor, mergeThread],
  );

  const reply = useCallback(
    async (
      threadId: string,
      body: string,
      quotes: CommentQuote[],
      attachments: AttachmentInput[],
    ) => {
      if (!artifactId) return;
      try {
        const updated = await addCommentApi({
          artifactId,
          discussionId: threadId,
          body,
          quotes,
          attachments,
        });
        clearError(threadId);
        mergeThread(updated);
        // CTA-FR-RPVU / CTA-FR-ZVKL: dispatched after the append, once per distinct
        // agent, without waiting on or cancelling anything already outstanding.
        dispatchFor(updated, body);
      } catch (e) {
        // CMT-FR-34: the error attaches to the card and the composer keeps its
        // content, so a post refused because someone else locked the thread is
        // retried (or copied out) rather than lost.
        noteError(threadId, e);
        // A refusal because the thread was locked since the rail last read it
        // must also re-render the card as locked, or the composer invites a
        // second attempt at something that cannot succeed.
        void listDiscussions({ kind: "artifact", artifactId })
          .then((loaded) => applyThreads(loaded.filter(isFragmentTargeted)))
          .catch(() => {});
        throw e;
      }
    },
    [artifactId, applyThreads, clearError, mergeThread, noteError, dispatchFor],
  );

  const setLock = useCallback(
    async (threadId: string, locked: boolean) => {
      if (!artifactId) return;
      try {
        const updated = await setDiscussionLock({ artifactId, discussionId: threadId, locked });
        clearError(threadId);
        mergeThread(updated);
      } catch (e) {
        noteError(threadId, e);
      }
    },
    [artifactId, clearError, mergeThread, noteError],
  );

  const setResolved = useCallback(
    async (threadId: string, resolved: boolean) => {
      if (!artifactId) return;
      try {
        const updated = await setDiscussionResolution({
          artifactId,
          discussionId: threadId,
          resolved,
        });
        clearError(threadId);
        mergeThread(updated);
      } catch (e) {
        noteError(threadId, e);
      }
    },
    [artifactId, clearError, mergeThread, noteError],
  );

  // CMT-FR-20: an edit moves every anchor after it and orphans any it destroyed.
  const noteBufferChange = useCallback((before: string, after: string) => {
    const edit = diffEdit(before, after);
    if (!edit) return;
    setThreads((prev) => shiftThreads(prev, edit));
  }, []);

  // CMT-FR-18 / CMT-FR-22: re-resolve from scratch against content the Editor
  // just adopted — a first load, or a Load-from-filesystem that replaced the
  // buffer wholesale, where tracking individual edits has nothing to track.
  const reanchorAll = useCallback((content: string) => {
    setThreads((prev) => resolveThreads(content, prev.map((t) => t.thread)));
  }, []);

  /**
   * CMT-FR-21: record the anchors that moved, so the next load starts its search
   * from where the passage now is.
   *
   * Best-effort and deliberately silent: this runs after a successful save, and
   * a failed re-anchor costs the user nothing they can see — the quote is still
   * stored, so the thread re-finds its passage on the next load anyway. Surfacing
   * it would put an error on a card for an operation the author never asked for.
   */
  const persistDrift = useCallback(async () => {
    if (!artifactId) return;
    const drifted = driftedAnchors(threadsRef.current);
    if (drifted.length === 0) return;
    const results = await Promise.allSettled(
      drifted.map((d) => {
        const stored = threadsRef.current.find((t) => t.thread.id === d.threadId)
          ?.thread.fragmentTarget;
        const fragmentTarget: FragmentTarget = {
          owner: { kind: "artifact", artifactId },
          path: artifactId,
          ...stored,
          ...d.anchor,
        };
        return reanchorDiscussionFragment({
          artifactId,
          discussionId: d.threadId,
          fragmentTarget,
        });
      }),
    );
    setThreads((prev) => {
      const byId = new Map<string, Discussion>();
      for (const r of results) {
        if (r.status === "fulfilled") byId.set(r.value.id, r.value);
      }
      if (byId.size === 0) return prev;
      return prev.map((entry) => {
        const updated = byId.get(entry.thread.id);
        return updated ? { thread: updated, anchor: entry.anchor } : entry;
      });
    });
  }, [artifactId]);

  const unresolved = useMemo(() => unresolvedCount(threads), [threads]);

  // CTA-FR-ZOLW: only the turns belonging to a thread this rail is showing, and
  // only those still `running`. A turn awaiting a reply renders nothing: that
  // agent is waiting on the author rather than composing anything, and its
  // question is already in the thread as an ordinary comment (CTA-FR-QTNB).
  const pendingTurns = useMemo(
    () =>
      knownTurns.filter(
        (turn) =>
          turn.state === "running" &&
          threads.some((t) => t.thread.id === turn.origin.discussionId),
      ),
    [knownTurns, threads],
  );

  return {
    threads,
    agents,
    /**
     * Every turn this tab knows to be running, unfiltered.
     *
     * Exposed so a second conversational surface in the same tab — the action
     * control's discussions (`ACT-action-control.md` ACT-FR-22) — can filter the
     * same snapshot by its own origin instead of issuing a second
     * application-wide read of its own.
     */
    allTurns: knownTurns,
    pendingTurns,
    turnFailures,
    failedTurns: recoverable.failedTurns,
    imageNotices: notices.imageNotices,
    retryingTurnIds: recoverable.retryingTurnIds,
    retryTurn: recoverable.retryTurn,
    cancelTurn,
    identity: identityState.identity,
    identityError: identityState.error,
    unresolved,
    errors,
    openThread,
    reply,
    setLock,
    setResolved,
    noteBufferChange,
    reanchorAll,
    persistDrift,
    retryIdentity: loadIdentity,
  };
}

/**
 * CMT-FR-25 / CMT-FR-26: turn a typed identity refusal into what the rail says
 * and what the user does next.
 *
 * The four causes call for four different responses — pick a token, verify one,
 * wait for the network, unlock the keychain — which is why the rail matches on
 * them rather than printing whatever came back. A project that stores no token is
 * not among them: it writes as **Me** (CMT-FR-26). `opensPicker` is the one that the token
 * picker resolves; every other renders inline.
 */
export function identityBlockFor(error: string | null): {
  message: string;
  route?: string;
  opensPicker: boolean;
} | null {
  if (error === null) return null;
  switch (error) {
    case COMMENT_IDENTITY_ERRORS.selectionRequired:
      return {
        message: "Choose which GitHub token this project uses before commenting.",
        opensPicker: true,
      };
    case COMMENT_IDENTITY_ERRORS.unresolved:
      return {
        message: "That GitHub token names no account yet.",
        route: "Verify it in Global settings → GitHub.",
        opensPicker: false,
      };
    case COMMENT_IDENTITY_ERRORS.githubUnreachable:
      return {
        message: "Could not reach GitHub, so comments cannot be attributed yet.",
        opensPicker: false,
      };
    case COMMENT_IDENTITY_ERRORS.keychainUnavailable:
      return {
        message: "The system keychain is unavailable, so comments cannot be attributed.",
        opensPicker: false,
      };
    default:
      return { message: error, opensPicker: false };
  }
}
