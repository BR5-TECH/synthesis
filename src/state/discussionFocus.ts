/**
 * The one-instance rule for discussion surfaces
 * (`CVP-conversation-presentation.md` CVP-FR-02, CVP-FR-05, CVP-FR-42).
 *
 * A discussion has at most one mounted surface. Every `DiscussionSurface`
 * registers here with the id it shows, the owner layout it sits in, and a
 * function that moves focus into it. The first surface to register for an id is
 * the **active** one. A second surface for the same id waits in a queue and
 * renders nothing until the active one goes, and then it takes over. So an owner
 * change that mounts the new surface before the old one unmounts never shows the
 * discussion twice.
 *
 * `focusDiscussion` is what every reveal route calls first. It focuses the
 * surface that already shows the discussion, or answers `"closed"`. Then the
 * caller opens the owner surface or the conversation tab, and holds a pending
 * focus request for the surface to consume when it mounts.
 *
 * Session memory only. Nothing here is persisted.
 */
import { useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

import { logDebug } from "../logging";

/** What a reveal puts focus on inside a surface. */
export type FocusTarget = "discussion" | "composer";

/** The result of {@link focusDiscussion}. */
export type FocusOutcome = "focused" | "closed";

interface Entry {
  owner: string;
  focus: (target: FocusTarget) => void;
}

/** What `registerSurface` returns. */
export interface SurfaceRegistration {
  /** Whether this registration is the one surface showing its discussion. */
  isActive: () => boolean;
  /** Remove the registration. The next waiting surface of the id takes over. */
  release: () => void;
}

const queues = new Map<string, Entry[]>();
const pending = new Map<string, FocusTarget>();
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

/**
 * Register a mounted surface of a discussion.
 *
 * The first registration of an id is active. Later ones wait. A surface that is
 * not active must not render the discussion.
 */
export function registerSurface(
  discussionId: string,
  owner: string,
  focus: (target: FocusTarget) => void,
): SurfaceRegistration {
  const entry: Entry = { owner, focus };
  const queue = queues.get(discussionId) ?? [];
  queue.push(entry);
  queues.set(discussionId, queue);
  if (queue.length > 1) {
    logDebug(["frontend"], "a second surface of one discussion is waiting", {
      discussionId,
      owner,
      active: queue[0].owner,
    });
  }
  emit();
  return {
    isActive: () => queues.get(discussionId)?.[0] === entry,
    release: () => {
      const held = queues.get(discussionId);
      if (!held) return;
      const at = held.indexOf(entry);
      if (at === -1) return;
      held.splice(at, 1);
      if (held.length === 0) queues.delete(discussionId);
      emit();
    },
  };
}

/** The owner layout of the surface showing this discussion, or `null` when closed. */
export function surfaceOwner(discussionId: string): string | null {
  return queues.get(discussionId)?.[0]?.owner ?? null;
}

/** How many surfaces are registered for an id, waiting ones included. */
export function surfaceCount(discussionId: string): number {
  return queues.get(discussionId)?.length ?? 0;
}

/**
 * CVP-FR-06 / CVP-FR-54: focus the surface that already shows the discussion.
 *
 * Returns `"closed"` when no surface is mounted for it, which tells the caller
 * to open the owner surface or the conversation tab.
 */
export function focusDiscussion(
  discussionId: string,
  target: FocusTarget = "discussion",
): FocusOutcome {
  const active = queues.get(discussionId)?.[0];
  if (!active) return "closed";
  active.focus(target);
  return "focused";
}

/**
 * CVP-FR-42: hold a focus request for a surface that mounts later than the
 * route runs. The surface consumes it when it becomes active.
 */
export function requestPendingFocus(
  discussionId: string,
  target: FocusTarget = "discussion",
): void {
  pending.set(discussionId, target);
}

/** The held focus request of a discussion, taken. */
export function consumePendingFocus(discussionId: string): FocusTarget | null {
  const held = pending.get(discussionId) ?? null;
  pending.delete(discussionId);
  return held;
}

/**
 * CVP-FR-49: forget every registration and every held request, as a project or
 * worktree change does.
 */
export function resetDiscussionFocus(): void {
  queues.clear();
  pending.clear();
  emit();
}

/**
 * Register a surface for as long as it is mounted and say whether it is the
 * active one.
 *
 * Registered in a layout effect, so the first paint already knows whether the
 * surface may render. A surface that returns `false` renders nothing.
 */
export function useSurfaceSlot(
  discussionId: string,
  owner: string,
  focus: (target: FocusTarget) => void,
): boolean {
  const focusRef = useRef(focus);
  focusRef.current = focus;
  const [registration, setRegistration] = useState<SurfaceRegistration | null>(null);

  useLayoutEffect(() => {
    const held = registerSurface(discussionId, owner, (target) =>
      focusRef.current(target),
    );
    setRegistration(held);
    return () => {
      held.release();
      setRegistration(null);
    };
  }, [discussionId, owner]);

  return useSyncExternalStore(
    subscribe,
    () => registration?.isActive() ?? false,
    () => false,
  );
}
