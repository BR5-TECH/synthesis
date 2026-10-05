/**
 * What one discussion holds that is neither the discussion nor its owner
 * (`CVP-conversation-presentation.md` CVP-FR-47, CVP-FR-48).
 *
 * A module-level store keyed by the discussion id, or by the discussion target
 * key for an opening composer that has no discussion id yet. It holds only what
 * the author has not sent and where the author is looking: the unsent composer
 * text, the quotes and the pending attachments, the scroll position, the unread
 * state, the outstanding agent turns that draw as pending placeholders, and the
 * inline failures.
 *
 * It is outside every component so that a change of owner surface, a tab that
 * closes and opens again, and a move between an owner layout and a conversation
 * tab all find the same state. It is memory only: nothing here is written to any
 * file or preference, and `clearAllDiscussionSessions` discards it on a project
 * or worktree change (CVP-FR-49).
 *
 * The answers to a question set are held by `./questionAnswerDrafts`, keyed by
 * the id of the set. That store is already module-level and already outlives
 * every surface, so this module does not copy it. It only discards it together
 * with the rest.
 */
import { useSyncExternalStore } from "react";

import type { PendingAttachment } from "../components/CommentAttachments";
import { logDebug } from "../logging";
import { forgetEveryDraft } from "./questionAnswerDrafts";
import { mergeKnownTurn } from "./activeAgents";
import type { AgentTurn, CommentQuote } from "../types";

export interface DiscussionSession {
  /** The unsent text of the composer. */
  body: string;
  quotes: readonly CommentQuote[];
  attachments: readonly PendingAttachment[];
  /** The scroll offset of the message list, restored when a surface mounts. */
  scrollTop: number;
  /** The list is at its foot, so a message that arrives is followed. */
  atTail: boolean;
  /** CVP-FR-TWRL: messages that arrived while the author was reading back. */
  unreadCount: number;
  /** The first of them, which the unread divider is drawn above. */
  firstUnreadId: string | null;
  /**
   * Every message id a surface of this discussion has rendered. A surface that
   * mounts later compares against it, so a message that arrived while no surface
   * was mounted still counts as unread. `null` until a surface has rendered once.
   */
  seenIds: readonly string[] | null;
  /** CTA-FR-ZOLW: the outstanding turns. Running ones draw as placeholders. */
  turns: readonly AgentTurn[];
  /** The list of turns was read from the backend once in this session. */
  turnsLoaded: boolean;
  /** CMT-FR-34: the typed error of the last refused operation. */
  error: string | undefined;
  /** CTA-FR-QTNB: the typed failure of the last turn that failed. */
  turnFailure: string | undefined;
  /** CVP-FR-42: focus last stood in the composer, so a reveal returns it there. */
  focusWasComposer: boolean;
  /**
   * CVP-FR-37: a failed contribution awaits Retry. Set by the runtime of a
   * mounted surface, and read by the Comments panel row (CMP-FR-28).
   */
  failedResponse: boolean;
}

const EMPTY: DiscussionSession = Object.freeze({
  body: "",
  quotes: Object.freeze([]) as readonly CommentQuote[],
  attachments: Object.freeze([]) as readonly PendingAttachment[],
  scrollTop: 0,
  atTail: true,
  unreadCount: 0,
  firstUnreadId: null,
  seenIds: null,
  turns: Object.freeze([]) as readonly AgentTurn[],
  turnsLoaded: false,
  error: undefined,
  turnFailure: undefined,
  focusWasComposer: false,
  failedResponse: false,
});

let sessions: ReadonlyMap<string, DiscussionSession> = new Map();
const listeners = new Set<() => void>();

function emit(): void {
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** One discussion's session, defaulted until something has been set on it. */
export function getDiscussionSession(key: string): DiscussionSession {
  return sessions.get(key) ?? EMPTY;
}

/**
 * Fold a change into one session. A change that alters nothing does not notify,
 * because a scroll event that moved nothing would otherwise re-render every
 * subscriber.
 */
export function patchDiscussionSession(
  key: string,
  change: Partial<DiscussionSession>,
): void {
  if (key === "") return;
  const held = getDiscussionSession(key);
  const next = { ...held, ...change };
  const same = (Object.keys(next) as (keyof DiscussionSession)[]).every(
    (field) => next[field] === held[field],
  );
  if (same) return;
  const map = new Map(sessions);
  map.set(key, next);
  sessions = map;
  emit();
}

export function setComposerBody(key: string, body: string): void {
  patchDiscussionSession(key, { body });
}

export function setComposerQuotes(key: string, quotes: readonly CommentQuote[]): void {
  patchDiscussionSession(key, { quotes: [...quotes] });
}

export function setComposerAttachments(
  key: string,
  attachments: readonly PendingAttachment[],
): void {
  patchDiscussionSession(key, { attachments: [...attachments] });
}

/** Discard what the composer holds, as a post that landed does. */
export function clearComposer(key: string): void {
  patchDiscussionSession(key, {
    body: "",
    quotes: EMPTY.quotes,
    attachments: EMPTY.attachments,
  });
}

export function setDiscussionError(key: string, error: string | undefined): void {
  patchDiscussionSession(key, { error });
}

export function setDiscussionTurnFailure(
  key: string,
  turnFailure: string | undefined,
): void {
  patchDiscussionSession(key, { turnFailure });
}

export function setFocusWasComposer(key: string, value: boolean): void {
  patchDiscussionSession(key, { focusWasComposer: value });
}

/**
 * CVP-FR-TWRL: the list scrolled. Reaching the foot clears the unread state,
 * because the messages under the divider have then been scrolled to.
 */
export function setDiscussionScroll(
  key: string,
  scrollTop: number,
  atTail: boolean,
): void {
  if (atTail) {
    patchDiscussionSession(key, {
      scrollTop,
      atTail: true,
      unreadCount: 0,
      firstUnreadId: null,
    });
    return;
  }
  patchDiscussionSession(key, { scrollTop, atTail: false });
}

/**
 * CVP-FR-TWRL: messages arrived while the author was reading back. The first of
 * them is kept across later arrivals, so the divider marks where the author
 * stopped reading rather than where the newest message begins.
 */
export function noteDiscussionArrivals(key: string, ids: readonly string[]): void {
  if (ids.length === 0) return;
  const held = getDiscussionSession(key);
  if (held.atTail) return;
  patchDiscussionSession(key, {
    unreadCount: held.unreadCount + ids.length,
    firstUnreadId: held.firstUnreadId ?? ids[0],
  });
}

/** CVP-FR-TWRL: activating the unread indicator clears the unread state. */
export function clearDiscussionUnread(key: string): void {
  patchDiscussionSession(key, { unreadCount: 0, firstUnreadId: null });
}

export function setDiscussionSeen(key: string, ids: readonly string[]): void {
  patchDiscussionSession(key, { seenIds: [...ids] });
}

/**
 * CVP-FR-HWTN: the ids of the turns that have ended or were cancelled in this
 * session. Turn ids are unique for the life of the application (AGC-FR-23), so
 * a `running` record for one of these ids is stale: a dispatch result or a read
 * that was in flight while the turn ended.
 */
let endedTurnIds: ReadonlySet<string> = new Set();

function noteTurnEnded(turnId: string): void {
  if (endedTurnIds.has(turnId)) return;
  endedTurnIds = new Set(endedTurnIds).add(turnId);
}

/** CVP-FR-HWTN: true when this turn has ended, so it never draws as pending again. */
export function hasTurnEnded(turnId: string): boolean {
  return endedTurnIds.has(turnId);
}

/** CVP-FR-HWTN: a running record for a turn that has already ended. */
function isStale(turn: AgentTurn): boolean {
  return turn.state === "running" && endedTurnIds.has(turn.id);
}

function logStale(key: string, turn: AgentTurn, source: string): void {
  logDebug(["frontend"], "a stale running turn record was ignored", {
    discussionId: key,
    turnId: turn.id,
    source,
  });
}

/**
 * CTA-FR-ZOLW: replace the outstanding turns of one discussion. `loaded` is
 * false where a read failed, so that a later mount tries again.
 */
export function setDiscussionTurns(
  key: string,
  turns: readonly AgentTurn[],
  loaded = true,
): void {
  // CVP-FR-HWTN: a read that was in flight while a turn ended reports it running.
  const live = turns.filter((t) => !isStale(t));
  patchDiscussionSession(key, { turns: live, turnsLoaded: loaded });
}

/**
 * CTA-FR-QXIG: fold one turn's record into the turns the discussion holds. The
 * turn keeps the position it already has, so two pending placeholders do not
 * swap when either agent begins a tool call.
 */
export function mergeDiscussionTurn(key: string, turn: AgentTurn): void {
  if (turn.state !== "running") noteTurnEnded(turn.id);
  else if (isStale(turn)) {
    logStale(key, turn, "event");
    return;
  }
  patchDiscussionSession(key, {
    turns: mergeKnownTurn(getDiscussionSession(key).turns, turn),
  });
}

/**
 * CVP-FR-HWTN: fold a turn event into the store whether or not a surface for
 * its discussion is mounted. The shell calls it for every event.
 */
export function followAgentTurn(turn: AgentTurn): void {
  if (turn.state !== "running") noteTurnEnded(turn.id);
  const key = turn.origin.discussionId;
  // Nothing to fold into a discussion this session never held.
  if (!key || !sessions.has(key)) return;
  mergeDiscussionTurn(key, turn);
}

/**
 * CTA-FR-ZOLW: an upsert for a turn the dispatch call just returned. CVP-FR-HWTN:
 * the call and the turn's events arrive in no fixed order, so a turn that has
 * already ended is not added again.
 */
export function upsertDiscussionTurn(key: string, turn: AgentTurn): void {
  if (isStale(turn)) {
    logStale(key, turn, "dispatch");
    return;
  }
  const held = getDiscussionSession(key).turns;
  patchDiscussionSession(key, {
    turns: [...held.filter((t) => t.id !== turn.id), turn],
  });
}

/** Optimistic cancel. CVP-FR-HWTN: the turn never draws as pending again. */
export function removeDiscussionTurn(key: string, turnId: string): void {
  noteTurnEnded(turnId);
  const held = getDiscussionSession(key).turns;
  if (!held.some((t) => t.id === turnId)) return;
  patchDiscussionSession(key, { turns: held.filter((t) => t.id !== turnId) });
}

/** CTA-FR-ZOLW: the turns still running, which draw as pending placeholders. */
export function pendingTurnsOf(turns: readonly AgentTurn[]): AgentTurn[] {
  return turns.filter((t) => t.state === "running");
}

/** CVP-FR-60: drop one session, as closing an opening composer does. */
export function clearDiscussionSession(key: string): void {
  if (!sessions.has(key)) return;
  const map = new Map(sessions);
  map.delete(key);
  sessions = map;
  emit();
}

/**
 * CVP-FR-49: discard every session, together with the question answers.
 *
 * Called on a project or worktree change. It changes nothing about any
 * discussion (CVP-FR-50).
 */
export function clearAllDiscussionSessions(): void {
  forgetEveryDraft();
  endedTurnIds = new Set();
  if (sessions.size === 0) return;
  sessions = new Map();
  emit();
}

/** One discussion's session, in React. */
export function useDiscussionSession(key: string): DiscussionSession {
  return useSyncExternalStore(
    subscribe,
    () => getDiscussionSession(key),
    () => getDiscussionSession(key),
  );
}
