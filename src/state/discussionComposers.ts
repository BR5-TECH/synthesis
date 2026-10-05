/**
 * What the Discuss composer holds before it is posted, per item
 * (`ACT-action-control.md` ACT-FR-14).
 *
 * The composer's unposted text and its pending attachments belong to the **item**
 * rather than to the tab: dismissing the composer and reopening it returns what
 * was typed, and so does closing the tab and opening the item again. So the store
 * is module-level and keyed by the item, exactly as `draftSessions` holds the
 * same two things for a draft — which is the store a New Artifact tab keeps
 * using, this one serving every other tab.
 *
 * Held only in memory and never persisted: nothing here is part of any file, and
 * none of it survives the application (ACT-FR-14).
 */
import { useCallback, useSyncExternalStore } from "react";
import type { AttachmentInput, DiscussionTarget } from "../types";
import { discussionTargetKey } from "../types";

/** One queued attachment, with the display name its chip carries. */
export interface PendingComposerAttachment {
  input: AttachmentInput;
  name: string;
}

interface ComposerSession {
  body: string;
  attachments: PendingComposerAttachment[];
}

const sessions = new Map<string, ComposerSession>();
const listeners = new Set<() => void>();

/**
 * A version counter rather than the map itself, because `useSyncExternalStore`
 * compares snapshots by identity and a mutable map is always identical to
 * itself. Bumping it is what tells React something in here changed.
 */
let version = 0;

function emit(): void {
  version += 1;
  for (const listener of listeners) listener();
}

function ensure(key: string): ComposerSession {
  let session = sessions.get(key);
  if (!session) {
    session = { body: "", attachments: [] };
    sessions.set(key, session);
  }
  return session;
}

/** The empty session every un-typed-in item reads as, shared so it is stable. */
const EMPTY: ComposerSession = { body: "", attachments: [] };

export function getComposer(key: string): ComposerSession {
  return sessions.get(key) ?? EMPTY;
}

export function setComposerBody(key: string, body: string): void {
  const session = ensure(key);
  if (session.body === body) return;
  session.body = body;
  emit();
}

export function setComposerAttachments(
  key: string,
  attachments: PendingComposerAttachment[],
): void {
  ensure(key).attachments = attachments;
  emit();
}

/**
 * Discard everything held for the open project.
 *
 * ACT-FR-14: none of this is persisted, and it does not outlive the project it
 * was typed in — on the same terms the Editor's retained edit state does not
 * (per `EDT-editor.md` EDT-FR-28).
 */
export function clearAllComposers(): void {
  if (sessions.size === 0) return;
  sessions.clear();
  emit();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * The composer state for one item, and the two setters that change it.
 *
 * `target` may be undefined while a tab has nothing bound to it yet; the reads
 * then serve the empty session and the writes are no-ops, so a caller never has
 * to guard.
 */
export function useDiscussionComposer(target: DiscussionTarget | undefined): {
  body: string;
  attachments: readonly PendingComposerAttachment[];
  setBody: (body: string) => void;
  setAttachments: (attachments: PendingComposerAttachment[]) => void;
  /** Reads through the store rather than off the snapshot, so two files dropped
   *  in one gesture do not each start from the same stale list. */
  current: () => PendingComposerAttachment[];
} {
  const key = target ? discussionTargetKey(target) : null;
  useSyncExternalStore(
    subscribe,
    () => version,
    () => version,
  );
  const session = key ? getComposer(key) : EMPTY;

  const setBody = useCallback(
    (body: string) => {
      if (key) setComposerBody(key, body);
    },
    [key],
  );
  const setAttachments = useCallback(
    (attachments: PendingComposerAttachment[]) => {
      if (key) setComposerAttachments(key, attachments);
    },
    [key],
  );
  const current = useCallback(
    () => (key ? getComposer(key).attachments : []),
    [key],
  );

  return {
    body: session.body,
    attachments: session.attachments,
    setBody,
    setAttachments,
    current,
  };
}
