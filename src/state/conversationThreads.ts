/**
 * The discussions a surface is rendering, by identity
 * (`CVP-conversation-presentation.md` CVP-FR-36, CVP-FR-53).
 *
 * A conversation tab renders the whole discussion without an owning surface
 * behind it, so it needs the thread itself and not just its id. Two things fill
 * this cache and nothing else re-reads:
 *
 * - **A surface that already listed it publishes it.** A discussion revealed
 *   from a rail that is still open therefore costs no read at all (CVP-FR-53).
 * - **`"read comment thread"`**, for a tab with no owning surface to read it
 *   through — one whose artifact has since been deleted (CMS-FR-61).
 *
 * Thereafter every surface follows `"discussion changed"` and redraws
 * from its payload rather than re-reading (CVP-FR-36), which is what makes a new
 * comment, an agent's answer, a lock and a resolution all appear in the currently
 * active presentation immediately.
 *
 * Session-only, like everything else about a presentation (CVP-FR-48).
 */
import { useSyncExternalStore } from "react";
import { readDiscussion } from "../api";
import { logWarn } from "../logging";
import type { Discussion } from "../types";

let threads: ReadonlyMap<string, Discussion> = new Map();
/** Threads whose read is in flight or done, so a render never re-asks. */
const asked = new Set<string>();
/** Ids the backend answered `discussion_not_found` for, so it is asked once. */
const missing = new Set<string>();

const listeners = new Set<() => void>();
let version = 0;

function emit(): void {
  version += 1;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Fold a thread this application already holds into the cache.
 *
 * Called by every surface that lists conversations and by the shell's
 * `"discussion changed"` subscription, so a presentation opened from a
 * mounted rail has its conversation before it renders and issues no read.
 */
export function publishThread(thread: Discussion): void {
  const held = threads.get(thread.id);
  if (held === thread) return;
  threads = new Map(threads).set(thread.id, thread);
  asked.add(thread.id);
  missing.delete(thread.id);
  emit();
}

/** The same for a whole listing, in one notification. */
export function publishThreads(list: readonly Discussion[]): void {
  if (list.length === 0) return;
  const next = new Map(threads);
  for (const thread of list) {
    next.set(thread.id, thread);
    asked.add(thread.id);
    missing.delete(thread.id);
  }
  threads = next;
  emit();
}

export function threadById(threadId: string): Discussion | undefined {
  return threads.get(threadId);
}

/** Whether the backend has told us this conversation is gone (CMS-FR-30). */
export function threadIsMissing(threadId: string): boolean {
  return missing.has(threadId);
}

/**
 * CVP-FR-53: read a conversation the presentation holding it has no owning
 * surface to read it through.
 *
 * At most one read per conversation per session: a presentation whose owning
 * surface is mounted has already published the thread, so this never runs for it.
 */
export function ensureThreadLoaded(threadId: string): void {
  if (asked.has(threadId)) return;
  asked.add(threadId);
  void readDiscussion(threadId)
    .then((thread) => {
      publishThread(thread);
    })
    .catch((error: unknown) => {
      // Recorded rather than surfaced: the presentation renders its own
      // unavailable state, and a conversation that could not be read is
      // otherwise invisible to anyone debugging it.
      missing.add(threadId);
      logWarn(["frontend"], "could not read a conversation by id", {
        threadId,
        error: String(error),
      });
      emit();
    });
}

/**
 * Read one conversation again and publish what comes back.
 *
 * For the caller that must act on a comment the **backend has just appended**
 * and cannot wait for the `"discussion changed"` event to arrive: the event
 * and the command's own response travel the same bridge with no ordering
 * between them, so a caller reading the cache the instant a command resolves may
 * still be holding the conversation as it stood before. `ensureThreadLoaded`
 * cannot serve this — it reads at most once per session by design.
 *
 * Resolves to `undefined` where the read failed. The caller decides what that
 * means; nothing here surfaces it, and the record is left as it stood.
 */
export async function refreshThread(
  threadId: string,
): Promise<Discussion | undefined> {
  try {
    const thread = await readDiscussion(threadId);
    publishThread(thread);
    return thread;
  } catch (error: unknown) {
    logWarn(["frontend"], "could not re-read a conversation by id", {
      threadId,
      error: String(error),
    });
    return undefined;
  }
}

/** Every conversation the cache holds, for an owner that is ending (CMS-FR-39). */
export function heldThreads(): Discussion[] {
  return [...threads.values()];
}

/** Drop conversations whose owner is gone, so nothing renders or re-reads them. */
export function forgetThreads(ids: readonly string[]): void {
  if (ids.length === 0) return;
  const next = new Map(threads);
  for (const id of ids) {
    next.delete(id);
    asked.delete(id);
    missing.delete(id);
  }
  threads = next;
  emit();
}

/**
 * CVP-FR-49: torn down with every presentation on a project or worktree change,
 * a conversation of the outgoing content root being a thing to drop rather than
 * to keep.
 */
export function clearConversationThreads(): void {
  if (threads.size === 0 && asked.size === 0) return;
  threads = new Map();
  asked.clear();
  missing.clear();
  emit();
}

function snapshot(): number {
  return version;
}

/**
 * One conversation by id, loading it on first use when nothing has published it.
 *
 * The load is kicked off from the subscribe callback rather than from an effect,
 * so a caller gets it by subscribing at all — the same shape `draftProposals`
 * uses, and for the same reason: every consumer wants the record.
 */
export function useConversationThread(
  threadId: string | null,
): Discussion | undefined {
  useSyncExternalStore(
    (listener) => {
      const unsubscribe = subscribe(listener);
      if (threadId) ensureThreadLoaded(threadId);
      return unsubscribe;
    },
    snapshot,
    snapshot,
  );
  return threadId ? threads.get(threadId) : undefined;
}
