/**
 * The question sets the discussions on screen are holding
 * (`../../specifications/ui/DQA-discussion-question-answering.md` DQA-FR-JWEF).
 *
 * A discussion is rendered in five presentations and the author moves it between
 * them (CVP-FR-47), so which of them is mounted decides nothing about whether a
 * set is known. Two things fill this cache and nothing else re-reads:
 *
 * - **`"read discussion question set"`**, once per discussion, when a surface
 *   first mounts it.
 * - **`"discussion question set changed"`**, thereafter — which is how a set
 *   recorded while the author is reading appears without a reopen, and how one
 *   submitted in another presentation goes.
 *
 * Session-only, like every other thing a presentation holds (CVP-FR-48). The set
 * itself is durable and the backend owns it; this is the copy on screen.
 *
 * A set that arrives **reconciles** the unsent answer draft against it rather
 * than replacing it (DQA-FR-DYFR), so a re-read while the author is part-way
 * through costs them nothing.
 */
import { useSyncExternalStore } from "react";
import { readDiscussionQuestionSet } from "../api";
import { logWarn } from "../logging";
import type { PendingQuestionSet } from "../types";
import { forgetSet, reconcile } from "./questionAnswerDrafts";

/**
 * thread id -> the set it holds, or `null` where it holds none.
 *
 * `null` and absent are different states: `null` is an answer the backend gave,
 * and absent is a discussion nobody has asked about yet. A block that treated
 * them alike would render nothing while a set was still being read, then appear
 * — which is the flicker the once-per-thread guard below exists to avoid.
 */
let sets: ReadonlyMap<string, PendingQuestionSet | null> = new Map();
/** Threads whose read is in flight or done, so a render never re-asks. */
const asked = new Set<string>();

const listeners = new Set<() => void>();
let version = 0;

function emit(): void {
  version += 1;
  for (const listener of listeners) listener();
}

/** A value that changes when anything here does, which is what the hook watches. */
function snapshot(): number {
  return version;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * CMS-FR-BQEN: fold in the set a discussion now holds, or that it holds none.
 *
 * Called by the shell's `"discussion question set changed"` subscription and by
 * the read below, so every presentation of one discussion redraws from one copy.
 */
export function publishSet(threadId: string, set: PendingQuestionSet | null): void {
  const held = sets.get(threadId);
  if (held === set) return;
  // DQA-FR-HCTM: the discussion stopped holding that set, by submission or by
  // any other route, so what the author entered against it goes with it.
  if (held && (!set || set.setId !== held.setId)) forgetSet(held.setId);
  // DQA-FR-DYFR: the set is redrawn from what came back and every draft entry
  // whose position it still records is kept against that position.
  if (set) reconcile(set.setId, set.questions.map((question) => question.position));
  sets = new Map(sets).set(threadId, set);
  asked.add(threadId);
  emit();
}

/** The set this discussion holds, `null` where it holds none, `undefined` where nobody has asked. */
export function questionSetOf(threadId: string): PendingQuestionSet | null | undefined {
  return sets.get(threadId);
}

/**
 * DQA-FR-JWEF: read the set a discussion holds, at most once per discussion per
 * session.
 *
 * Everything after that arrives on the event, so a surface that mounts the same
 * discussion in a second presentation issues no read of its own.
 */
export function ensureQuestionSetLoaded(threadId: string): void {
  if (asked.has(threadId)) return;
  asked.add(threadId);
  void readDiscussionQuestionSet(threadId)
    .then((set) => {
      publishSet(threadId, set);
    })
    .catch((error: unknown) => {
      // Recorded rather than surfaced. A discussion whose set could not be read
      // renders as one holding none, which is the safe reading: the composer
      // stays available and the author is never blocked by a failed read.
      logWarn(["frontend"], "could not read a discussion question set", {
        threadId,
        error: String(error),
      });
      publishSet(threadId, null);
    });
}

/**
 * DQA-FR-NRZB, DQA-FR-JWEF: the set this discussion holds, read once and
 * followed thereafter.
 *
 * Returns `null` while the read is outstanding as well as when the discussion
 * holds none, so a block renders nothing until there is something to render.
 */
export function useQuestionSet(threadId: string | null): PendingQuestionSet | null {
  // The read is issued on subscription rather than during render, exactly as
  // `useConversationThread` issues its own: a render is not the place to start
  // one, and this is the moment the component is actually watching.
  useSyncExternalStore(
    (listener) => {
      const unsubscribe = subscribe(listener);
      if (threadId) ensureQuestionSetLoaded(threadId);
      return unsubscribe;
    },
    snapshot,
    snapshot,
  );
  return (threadId ? sets.get(threadId) : null) ?? null;
}

/**
 * CVP-FR-49: torn down with every presentation on a project or worktree change,
 * a discussion of the outgoing content root being a thing to drop rather than to
 * keep.
 */
export function clearQuestionSets(): void {
  if (sets.size === 0 && asked.size === 0) return;
  for (const set of sets.values()) if (set) forgetSet(set.setId);
  sets = new Map();
  asked.clear();
  emit();
}
