/**
 * The polite live-region message of the discussion routes
 * (`CVP-conversation-presentation.md` CVP-FR-43).
 *
 * A reveal names the discussion and the surface it landed on. The shell draws
 * the latest message in one live region. Session memory only.
 */
import { useSyncExternalStore } from "react";

export interface DiscussionAnnouncement {
  seq: number;
  message: string;
}

let current: DiscussionAnnouncement = { seq: 0, message: "" };
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Say something. A repeated message still reads, because the sequence moves. */
export function announceDiscussion(message: string): void {
  current = { seq: current.seq + 1, message };
  listeners.forEach((l) => l());
}

/** The latest announcement, in React. */
export function useDiscussionAnnouncement(): DiscussionAnnouncement {
  return useSyncExternalStore(
    subscribe,
    () => current,
    () => current,
  );
}
