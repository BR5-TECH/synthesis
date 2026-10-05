/**
 * One thing's discussions, as the tab showing it reads and continues them
 * (`CMT-comments.md` CMT-FR-52 … CMT-FR-62, `ACT-action-control.md` ACT-FR-16,
 * ACT-FR-22).
 *
 * The counterpart of `useComments` for threads that are about a whole thing
 * rather than about a passage of one. Everything anchoring costs that hook — the
 * re-anchor search, the buffer tracking, the drift persistence — is simply absent
 * here, because a discussion aligns to nothing and is never orphaned and never
 * reanchored (CMT-FR-55).
 *
 * The **target** is what the hook is keyed on rather than a draft id, because a
 * discussion is opened against a file of the project exactly as it is against a
 * draft (CMS-FR-56) and the two differ in nothing this hook does.
 *
 * Routing is the same rule every conversation this application holds obeys
 * (CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-DGOC): a message reaches the agents its own tags
 * name, or failing those the conversation's **active agents** — the agents the
 * newest human comment naming anybody named. The rule itself is pure and lives
 * in `../state/activeAgents`; this hook is the part that has to talk to the
 * backend and to React.
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
  resolveCommentAuthorIdentity,
  setDiscussionLock,
  setDiscussionResolution,
} from "../api";
import {
  onAgentTurnStateChanged,
  onDiscussionChanged,
  onGithubTokensChanged,
} from "../events";
import {
  dispatchTargets,
  withoutAwaitingReply,
  mergeKnownTurn,
} from "../state/activeAgents";
import { addressesEveryone, agentRoster } from "../components/agentTags";
import type { AgentRoster } from "../components/agentTags";
import { logDebug, logWarn } from "../logging";
import { loggableTurnFailure } from "../components/CommentRail/messages";
import {
  setDiscussionTurnFailure,
  hasTurnEnded,
  upsertDiscussionTurn,
} from "../state/discussionSession";
import { publishThread, publishThreads } from "../state/conversationThreads";
import { useRecoverableFailures } from "./useRecoverableFailures";
import { useImageNotices } from "./useImageNotices";
import type { AnchoredThread } from "../state/commentAnchors";
import {
  locatorOf,
  originArtifactId,
  originDraftId,
  originFor,
  originKind,
} from "../state/discussionOrigin";
import {
  discussionTargetKey,
  isFragmentTargeted,
  type AgentTurn,
  type AttachmentInput,
  type CommentQuote,
  type Discussion,
  type DiscussionTarget,
  type FragmentTarget,
  type Participant,
  type ProjectAgent,
} from "../types";

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/** The key the Discuss composer's own inline error is filed under (ACT-FR-17). */
export const COMPOSER_KEY = "__composer__";

export interface UseDiscussionsResult {
  /** The owner target these discussions belong to, when there is one. */
  target?: DiscussionTarget;
  /**
   * CMT-FR-53: the draft's discussions, in the order they were opened. Shaped as
   * `AnchoredThread` so the rail renders them with the same card it renders an
   * anchored thread with — the anchor is always null, which is exactly what a
   * discussion has (CMT-FR-55).
   */
  discussions: AnchoredThread[];
  /** CTA-FR-VQFJ: the agents a composer's mention picker offers. */
  agents: ProjectAgent[];
  /** CTA-FR-ZOLW: the turns outstanding in this draft's discussions. */
  pendingTurns: AgentTurn[];
  /** CTA-FR-QTNB: the typed failure of the last turn that failed, per thread. */
  turnFailures: Record<string, string>;
  /**
   * CTA-FR-RHPP: the conversation's current terminal **retryable** failure, per
   * thread — what a card renders as a failed contribution carrying Retry.
   */
  failedTurns: Readonly<Record<string, AgentTurn>>;
  /** CTA-FR-ARBB: this target's conversations that could not send their images. */
  imageNotices: Readonly<Record<string, AgentTurn>>;
  /** CTA-FR-QDDG: the turns whose Retry is mid-dispatch, so those controls are off. */
  retryingTurnIds: ReadonlySet<string>;
  /** CTA-FR-QDDG: start a fresh turn for a failed one. */
  retryTurn: (turnId: string) => Promise<void>;
  cancelTurn: (turnId: string) => Promise<void>;
  identity: Participant | null;
  /** NAW-FR-33 / CMT-FR-24: why posting is unavailable, when it is. */
  identityError: string | null;
  retryIdentity: () => Promise<void>;
  /** CMT-FR-56: unresolved discussions, counted in the action row's total. */
  unresolved: number;
  /** Per-thread inline error from the last failed operation (CMT-FR-34). */
  errors: Record<string, string>;
  /** NAW-FR-32: open a discussion and dispatch for the agents it addressed. */
  open: (
    body: string,
    attachments: AttachmentInput[],
    fragmentTarget?: FragmentTarget | null,
  ) => Promise<Discussion>;
  /** CTA-FR-QUXJ: post into an existing discussion, then dispatch by its rules. */
  reply: (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<void>;
  setLock: (threadId: string, locked: boolean) => Promise<void>;
  setResolved: (threadId: string, resolved: boolean) => Promise<void>;
}

/**
 * The application-wide reads a conversational surface needs, when the host has
 * already issued them.
 *
 * The agent roster and the running turns are facts about the project rather than
 * about one conversation, so a tab carrying both a comment rail and an action
 * control would otherwise read each of them twice. Passing what the rail already
 * holds keeps it at one read per tab (ACT-FR-22); a host with no such surface
 * passes nothing and this hook reads them itself.
 */
export interface SharedConversationContext {
  agents: readonly ProjectAgent[];
  allTurns: readonly AgentTurn[];
}

export function useDiscussions(
  target: DiscussionTarget | undefined,
  enabled: boolean,
  shared?: SharedConversationContext,
): UseDiscussionsResult {
  const [threads, setThreads] = useState<Discussion[]>([]);
  const [identity, setIdentity] = useState<Participant | null>(null);
  const [identityError, setIdentityError] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [agents, setAgents] = useState<ProjectAgent[]>([]);
  const [knownTurns, setKnownTurns] = useState<AgentTurn[]>([]);
  /**
   * Turns the author cancelled here, held only until the backend agrees.
   *
   * A cancellation is optimistic and the host's snapshot is not this hook's to
   * edit, so the id is remembered rather than the turn removed — and forgotten
   * again the moment that turn's terminal event lands, which is what keeps this
   * from growing for the life of the tab.
   */
  const [cancelledTurns, setCancelledTurns] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [turnFailures, setTurnFailures] = useState<Record<string, string>>({});

  // The target arrives as a fresh object on every render of the caller, so every
  // effect below depends on this string rather than on the object — otherwise
  // each render re-reads the log and re-subscribes to the event bus.
  const targetKey = target ? discussionTargetKey(target) : null;
  const targetRef = useRef<DiscussionTarget | undefined>(target);
  targetRef.current = target;

  // The roster actually in use: the host's where it supplied one, this hook's
  // otherwise. Declared before `nicknames` because tag resolution reads it, and
  // resolving against an empty local roster while the host holds a full one is
  // exactly the bug that dispatches no turn for a message that addressed someone.
  const effectiveAgents = shared ? shared.agents : agents;

  // AGT-FR-36: both lists, because the two tags resolve against different sets —
  // a nickname against every enrolled agent, `@all` against those that can
  // answer. Flattening to names here would silently make the handle address an
  // agent whose key has expired.
  const roster = useMemo(() => agentRoster(effectiveAgents), [effectiveAgents]);
  // Read inside async callbacks that must not re-create on every roster change.
  const rosterRef = useRef<AgentRoster>(roster);
  rosterRef.current = roster;
  /**
   * CTA-FR-JQDM: the turns this hook knows, as they stand at the moment of a
   * post. Declared here and filled once `effectiveTurns` below is computed,
   * because `dispatchFor` closes over it and runs long after this render.
   */
  const turnsRef = useRef<AgentTurn[]>([]);

  /**
   * CMT-FR-54: one read when the tab opens, and the `"discussion changed"`
   * event thereafter — on exactly the terms the rail follows it for an anchored
   * thread (CMT-FR-04). Selecting another file of the draft re-lists that file's
   * anchored threads and leaves these as they stand, which is why nothing here
   * is keyed on the selection.
   */
  useEffect(() => {
    const current = targetRef.current;
    if (!targetKey || !current || !enabled) {
      setThreads([]);
      return;
    }
    let cancelled = false;
    void listDiscussions(current)
      .then((loaded) => {
        // A payload that is not a list is treated as no discussions rather than
        // rendered: one malformed answer costs the surface its conversations,
        // never the tab it is in.
        if (cancelled) return;
        const list = Array.isArray(loaded) ? loaded.filter((d) => current.kind === "draft" || !isFragmentTargeted(d)) : [];
        publishThreads(list);
        setThreads(list);
      })
      .catch(() => {
        if (!cancelled) setThreads([]);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey, enabled]);

  const mergeThread = useCallback((updated: Discussion) => {
    // CVP-FR-53: published so a discussion detached from this panel renders
    // without a read of its own.
    publishThread(updated);
    setThreads((prev) => {
      const index = prev.findIndex((t) => t.id === updated.id);
      // Appended rather than inserted by date: the backend orders discussions by
      // `createdAt` ascending (CMS-FR-58) and a newly opened one is the newest,
      // so the end of the list is where it belongs.
      if (index === -1) return [...prev, updated];
      const next = [...prev];
      next[index] = updated;
      return next;
    });
  }, []);

  useEffect(() => {
    if (!targetKey || !enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onDiscussionChanged((thread) => {
      // Application-wide: another file's discussion, another draft's, and every
      // anchored thread anywhere have nothing to do with this surface.
      if (isFragmentTargeted(thread) && thread.target.kind !== "draft") return;
      if (discussionTargetKey(thread.target) !== targetKey) return;
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
  }, [targetKey, enabled, mergeThread]);

  // CTA-FR-VQFJ / AGT-FR-25: the roster the composers' mention picker offers.
  // The agent record carries the title each entry names (AGT-FR-43), so this is
  // the whole of what the picker needs.
  const hasShared = shared !== undefined;
  useEffect(() => {
    if (!enabled || hasShared) {
      if (!enabled) setAgents([]);
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
  }, [enabled, hasShared]);

  /**
   * NAW-FR-33 / CMT-FR-24: resolve who this machine writes as before the author
   * types anything, so the composer is disabled with a reason rather than
   * failing at the moment they post.
   */
  const loadIdentity = useCallback(async () => {
    if (!enabled) return;
    try {
      setIdentity(await resolveCommentAuthorIdentity());
      setIdentityError(null);
    } catch (e) {
      setIdentity(null);
      setIdentityError(errorText(e));
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

  // CTA-FR-ZOLW: what is still coming, from `"list agent turns"` when the tab
  // opens and from the event thereafter.
  const isMine = useCallback(
    (turn: AgentTurn) => {
      const kind = originKind(turn.origin);
      if (kind === "draft_comment" || kind === "draft_discussion")
        return `draft:${originDraftId(turn.origin)}` === targetKey;
      if (kind === "artifact_discussion")
        return `artifact:${originArtifactId(turn.origin)}` === targetKey;
      return false;
    },
    [targetKey],
  );

  /**
   * CTA-FR-MGVJ / CTA-FR-RHPP: the offer to ask again, for this target's own
   * conversations. A refusal renders at the foot of the card that triggered it
   * (CMT-FR-34), like every other typed failure here.
   */
  /**
   * CTA-FR-ARBB: and the notice a turn that could not send its pictures leaves,
   * read and followed on exactly the same terms.
   */
  const notices = useImageNotices(Boolean(targetKey) && enabled, isMine);

  const recoverable = useRecoverableFailures(
    Boolean(targetKey) && enabled,
    isMine,
    useCallback((threadId: string, reason: string) => {
      setErrors((prev) => ({ ...prev, [threadId]: reason }));
    }, []),
  );

  useEffect(() => {
    // A cancellation suppresses one id of one target's conversation; carrying it
    // to the next target could hide a turn that happens to share it.
    setCancelledTurns(new Set());
    if (!targetKey || !enabled) {
      setKnownTurns([]);
      return;
    }
    if (hasShared) return;
    let cancelled = false;
    void listAgentTurns(null)
      .then((turns) => {
        if (!cancelled) setKnownTurns((turns ?? []).filter(isMine));
      })
      .catch(() => {
        if (!cancelled) setKnownTurns([]);
      });
    return () => {
      cancelled = true;
    };
  }, [targetKey, enabled, isMine, hasShared]);

  useEffect(() => {
    if (!targetKey || !enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentTurnStateChanged((turn) => {
      if (!isMine(turn)) return;
      // CTA-FR-ZOLW: replaced where it already stands rather than moved to the
      // end, so the many activity events a running turn now emits (AGC-FR-34)
      // do not reorder the pending contributions a card is showing.
      setKnownTurns((prev) => mergeKnownTurn(prev, turn));
      // The backend has spoken for this turn, so the optimistic suppression has
      // nothing left to hide and is dropped.
      if (turn.state !== "running") {
        setCancelledTurns((prev) => {
          if (!prev.has(turn.id)) return prev;
          const next = new Set(prev);
          next.delete(turn.id);
          return next;
        });
      }
      // CTA-FR-QTNB: a **recoverable** failure renders as a failed contribution
      // carrying Retry (CTA-FR-MGVJ) rather than as an inline typed error, so it
      // is `useRecoverableFailures`' to hold and not this map's.
      if (turn.state === "failed" && turn.failure && !turn.retryPermitted) {
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
  }, [targetKey, enabled, isMine]);

  /**
   * CTA-FR-QUXJ: the message is appended first and unconditionally; only then is a
   * turn dispatched, once per distinct agent the rules name — the agents the
   * message's own tags resolve to, or failing those the conversation's active
   * agents (CTA-FR-LCFU), or nobody. CTA-FR-YWSU: an agent's delivered answer never
   * reaches here, because only a comment a person posted starts a round of
   * turns, and it changes the active set no more than it dispatches.
   */
  const dispatchFor = useCallback(
    (thread: Discussion, body: string) => {
      const targets = dispatchTargets(thread, body, rosterRef.current);
      if (targets.length === 0) {
        // CTA-FR-QUXJ: see `useComments` — an ordinary outcome, recorded so a
        // conversation that answered nothing is not mistaken for a lost
        // dispatch. Never the body: it is user content.
        logDebug(["ai", "frontend"], "a message reached no agent", {
          threadId: thread.id,
          enrolled: rosterRef.current.nicknames.length,
        });
        return;
      }
      const triggerCommentId = thread.comments[thread.comments.length - 1]?.id;
      if (!triggerCommentId) return;
      // As in `useComments`: the handle is what a reader asking "why three
      // answers?" needs, and each turn's own record cannot show it. The body is
      // deliberately absent — it is user content.
      if (addressesEveryone(body, rosterRef.current)) {
        logDebug(["ai", "frontend"], "a message addressed every agent that can answer", {
          threadId: thread.id,
          agents: targets.join(" "),
          count: targets.length,
        });
      }
      // CTA-FR-JQDM / AGC-FR-29: see `useComments` — dispatching ends the wait,
      // the retirement is silent, so the surface that caused it forgets it here.
      // Routing never read it.
      setKnownTurns((prev) => withoutAwaitingReply(prev, targets, thread.id));
      turnsRef.current = withoutAwaitingReply(
        turnsRef.current,
        targets,
        thread.id,
      );
      // CTA-FR-UUXA: a new message retires the refusal the last one met, once
      // for the whole batch. Cleared on each accepted dispatch instead, one
      // agent's acceptance would hide another agent's refusal in the same batch.
      setTurnFailures((prev) => {
        if (!(thread.id in prev)) return prev;
        const next = { ...prev };
        delete next[thread.id];
        return next;
      });
      setDiscussionTurnFailure(thread.id, undefined);
      const origin = originFor(thread);
      for (const nickname of targets) {
        void dispatchAgentTurn({
          nickname,
          origin,
          triggerCommentId,
        })
          .then((turn) => {
            // Upsert rather than append: the backend publishes the registration
            // event before `dispatch_agent_turn` resolves, and there is no
            // ordering between the two. CVP-FR-HWTN: a turn that already ended
            // is not added again.
            if (!hasTurnEnded(turn.id)) {
              setKnownTurns((prev) => [
                ...prev.filter((t) => t.id !== turn.id),
                turn,
              ]);
            }
            // CVP-FR-47: the shared surface reads the turns from the session
            // store, so a placeholder stands whichever owner shows the discussion.
            upsertDiscussionTurn(thread.id, turn);
          })
          .catch((e) => {
            setTurnFailures((prev) => ({ ...prev, [thread.id]: errorText(e) }));
            // CTA-FR-UUXA: the shared surface reads the refusal from the session
            // store, so it renders at the foot of the discussion whichever owner
            // shows it.
            setDiscussionTurnFailure(thread.id, errorText(e));
            // Never the body, and never the raw error: it can quote the
            // argument it refused (see `loggableTurnFailure`).
            logWarn(["ai", "frontend"], "an agent turn could not be dispatched", {
              threadId: thread.id,
              nickname,
              failure: loggableTurnFailure(errorText(e)),
            });
          });
      }
    },
    [],
  );

  const clearError = useCallback((key: string) => {
    setErrors((prev) => {
      if (!(key in prev)) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  /**
   * NAW-FR-32: the message stands whether or not any agent answers it, and one
   * addressing nobody opens a discussion all the same — a draft is a thing to
   * think aloud about before it is a thing to ask about.
   */
  const open = useCallback(
    async (
      body: string,
      attachments: AttachmentInput[],
      fragmentTarget: FragmentTarget | null = null,
    ) => {
      const current = targetRef.current;
      if (!current) throw new Error("no discussion target");
      try {
        const created = await openDiscussion({
          target: current,
          fragmentTarget,
          body,
          attachments,
        });
        clearError(COMPOSER_KEY);
        mergeThread(created);
        dispatchFor(created, body);
        return created;
      } catch (e) {
        // NAW-FR-33: rendered inline in the composer, whose body and pending
        // strip are left intact so the refused attachment is removed and the
        // message posted without being retyped.
        setErrors((prev) => ({ ...prev, [COMPOSER_KEY]: errorText(e) }));
        throw e;
      }
    },
    [clearError, mergeThread, dispatchFor],
  );

  const reply = useCallback(
    async (
      threadId: string,
      body: string,
      quotes: CommentQuote[],
      attachments: AttachmentInput[],
    ) => {
      const current = targetRef.current;
      if (!current) return;
      try {
        const updated = await addCommentApi({
          ...locatorOf(current),
          discussionId: threadId,
          body,
          quotes,
          attachments,
        });
        clearError(threadId);
        mergeThread(updated);
        dispatchFor(updated, body);
      } catch (e) {
        setErrors((prev) => ({ ...prev, [threadId]: errorText(e) }));
        // A refusal because the discussion was locked since the surface last read
        // it must also re-render the card as locked, or the composer invites a
        // second attempt at something that cannot succeed.
        void listDiscussions(current)
          .then((loaded) => setThreads(Array.isArray(loaded) ? loaded : []))
          .catch(() => {});
        throw e;
      }
    },
    [clearError, mergeThread, dispatchFor],
  );

  const setLock = useCallback(
    async (threadId: string, locked: boolean) => {
      const current = targetRef.current;
      if (!current) return;
      try {
        const updated = await setDiscussionLock({
          ...locatorOf(current),
          discussionId: threadId,
          locked,
        });
        clearError(threadId);
        mergeThread(updated);
      } catch (e) {
        setErrors((prev) => ({ ...prev, [threadId]: errorText(e) }));
      }
    },
    [clearError, mergeThread],
  );

  const setResolved = useCallback(
    async (threadId: string, resolved: boolean) => {
      const current = targetRef.current;
      if (!current) return;
      try {
        const updated = await setDiscussionResolution({
          ...locatorOf(current),
          discussionId: threadId,
          resolved,
        });
        clearError(threadId);
        mergeThread(updated);
      } catch (e) {
        setErrors((prev) => ({ ...prev, [threadId]: errorText(e) }));
      }
    },
    [clearError, mergeThread],
  );

  const cancelTurn = useCallback(async (turnId: string) => {
    // Optimistic on both sides of the merge below: dropping it from this hook's
    // own set is not enough when the host supplied one, because the host still
    // holds it until the backend's terminal event lands and the merge would
    // simply put it back — leaving the card claiming an answer is coming after
    // the author said they no longer want it.
    setKnownTurns((prev) => prev.filter((t) => t.id !== turnId));
    setCancelledTurns((prev) =>
      prev.has(turnId) ? prev : new Set(prev).add(turnId),
    );
    await cancelAgentTurn(turnId).catch(() => {});
  }, []);

  const discussions = useMemo(
    () => threads.map((thread) => ({ thread, anchor: null })),
    [threads],
  );

  /**
   * The host's snapshot filtered by THIS surface's origin, so a rail's turns and
   * a discussion's never appear in one another (ACT-FR-22) — **merged with** what
   * this hook knows first-hand.
   *
   * The merge is what makes the shared path behave like the unshared one. The
   * host supplies the opening `list_agent_turns` snapshot, which is the whole
   * reason for sharing; but a turn this hook dispatched is known here the moment
   * `dispatch_agent_turn` resolves and only reaches the host when the backend's
   * registration event does, and there is no ordering between the two. Reading
   * the host alone leaves that window with no pending contribution on the card —
   * which is the entire feedback a person gets that their message was heard.
   *
   * Keyed by id so the same turn arriving from both sides is one contribution.
   */
  const effectiveTurns = useMemo(() => {
    if (!shared) return knownTurns;
    const merged = new Map<string, AgentTurn>();
    for (const turn of shared.allTurns) if (isMine(turn)) merged.set(turn.id, turn);
    for (const turn of knownTurns) merged.set(turn.id, turn);
    for (const id of cancelledTurns) merged.delete(id);
    return [...merged.values()];
  }, [shared, knownTurns, isMine, cancelledTurns]);
  turnsRef.current = effectiveTurns;
  // CTA-FR-ZOLW / CTA-FR-QTNB: only the turns still `running` render a pending
  // contribution. One awaiting a reply renders nothing — its question is already
  // in the thread as an ordinary comment, and that agent is waiting on the
  // author rather than composing anything.
  const pendingTurns = useMemo(
    () =>
      effectiveTurns.filter(
        (turn) =>
          turn.state === "running" &&
          threads.some((t) => t.id === turn.origin.discussionId),
      ),
    [effectiveTurns, threads],
  );

  const unresolved = useMemo(
    () => threads.filter((t) => !t.resolved).length,
    [threads],
  );

  return {
    target,
    discussions,
    agents: effectiveAgents as ProjectAgent[],
    pendingTurns,
    turnFailures,
    failedTurns: recoverable.failedTurns,
    imageNotices: notices.imageNotices,
    retryingTurnIds: recoverable.retryingTurnIds,
    retryTurn: recoverable.retryTurn,
    cancelTurn,
    identity,
    identityError,
    retryIdentity: loadIdentity,
    unresolved,
    errors,
    open,
    reply,
    setLock,
    setResolved,
  };
}
