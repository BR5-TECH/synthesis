/**
 * The toast stack of the notification facility (`NTF-notifications.md`
 * NTF-FR-KTRQ through NTF-FR-OPCD).
 *
 * A toast is the in-window channel of a raise: it tells the author who is
 * working in the main window that something needs them now. The store holds what
 * is showing and what waits, and nothing else. It records nothing (NTF-FR-22):
 * a toast that has left is gone, and the store reaches no backend operation.
 *
 * It is also the one place where a surface reports the result of an action of
 * the author (NTF-FR-TSRL). The facility (`notifications.ts`) writes the toasts
 * of a raise. A surface writes the toast of a result through `showToast`, from a
 * small module of its own (for example `pullToast.ts`). The dismissal and click
 * paths of the stack component remove the toast they act on.
 */
import { useSyncExternalStore } from "react";

/** NTF-FR-KTRQ: the level a raising surface sets on a raise. */
export type NotificationLevel = "Info" | "Warn" | "Error";

/** NTF-FR-HZNF: at most this many toasts show at one time. */
export const MAX_VISIBLE_TOASTS = 3;

/** NTF-FR-YJAE: how long an Info toast stays, in milliseconds. */
export const INFO_TOAST_MS = 5000;

export interface ToastEntry {
  /** The key of the raise. A later toast with this key replaces this one. */
  key: string;
  level: NotificationLevel;
  title: string;
  /** May be empty. A toast with an empty body shows no body line. */
  body: string;
  /** Where a click lands the author. Null for a toast that only dismisses. */
  address: string | null;
  /**
   * NTF-FR-XNCG: one link of a toast with no address — a label and an action
   * of the surface that shows it. Activating it leaves the toast in the stack.
   */
  link?: ToastLink | null;
  /**
   * Counts the replacements of this toast. A change restarts the Info timer
   * (NTF-FR-HZNF).
   */
  revision: number;
}

export type ToastInput = Omit<ToastEntry, "revision">;

/** NTF-FR-XNCG: the link a toast with no address may carry. */
export interface ToastLink {
  label: string;
  onActivate: () => void;
}

/** The visible toasts in arrival order: the oldest first. */
let visible: ToastEntry[] = [];
/** The toasts that wait for a slot, in arrival order. */
let queue: ToastEntry[] = [];
/** The visible toasts as the stack draws them: the newest first. */
let snapshot: readonly ToastEntry[] = [];

const subscribers = new Set<() => void>();

function publish(): void {
  snapshot = [...visible].reverse();
  for (const notify of subscribers) notify();
}

function promote(): void {
  while (visible.length < MAX_VISIBLE_TOASTS && queue.length > 0) {
    visible.push(queue.shift() as ToastEntry);
  }
}

/**
 * NTF-FR-HZNF, NTF-FR-SXTI: show a toast.
 *
 * A key that is showing replaces that toast in place and restarts its timer. A
 * key that waits takes the new content in its queue place. Any other toast shows
 * at once when a slot is free, and waits in the queue otherwise. A `priority`
 * toast waits at the front of the queue, so the next free slot is its slot.
 */
export function showToast(
  input: ToastInput,
  options: { priority?: boolean } = {},
): void {
  const shown = visible.findIndex((t) => t.key === input.key);
  if (shown !== -1) {
    visible[shown] = { ...input, revision: visible[shown].revision + 1 };
  } else {
    const waiting = queue.findIndex((t) => t.key === input.key);
    if (waiting !== -1) {
      queue[waiting] = { ...input, revision: queue[waiting].revision + 1 };
    } else if (visible.length < MAX_VISIBLE_TOASTS) {
      visible.push({ ...input, revision: 0 });
    } else if (options.priority) {
      // A toast that answers an action of the author waits first in the queue.
      queue.unshift({ ...input, revision: 0 });
    } else {
      queue.push({ ...input, revision: 0 });
    }
  }
  publish();
}

function removeWhere(drop: (toast: ToastEntry) => boolean): void {
  const before = visible.length + queue.length;
  visible = visible.filter((t) => !drop(t));
  queue = queue.filter((t) => !drop(t));
  if (visible.length + queue.length === before) return;
  promote();
  publish();
}

/** NTF-FR-QEHM, NTF-FR-ZSIK: remove one toast by its key. */
export function dismissToast(key: string): void {
  removeWhere((t) => t.key === key);
}

/** NTF-FR-14: the author reached what the toasts address. */
export function removeToastsForAddress(address: string): void {
  removeWhere((t) => t.address === address);
}

/** NTF-FR-38: the surface retracted the key. */
export function removeToastsForKey(key: string): void {
  removeWhere((t) => t.key === key);
}

/** NTF-FR-15: the content root changed, so no toast outlives it. */
export function clearToasts(): void {
  removeWhere(() => true);
}

/**
 * NTF-FR-WTFB: toasts about a project that is opening, held until its content
 * root is open. The opening removes every toast (NTF-FR-15), so a toast shown
 * before it would go at once.
 */
let pendingForRoot: { projectKey: string; input: ToastInput }[] = [];

/** NTF-FR-WTFB: show `input` when the content root of `projectKey` is open. */
export function showToastWhenRootOpens(projectKey: string, input: ToastInput): void {
  pendingForRoot = [
    ...pendingForRoot.filter((p) => p.input.key !== input.key),
    { projectKey, input },
  ];
}

/** NTF-FR-WTFB: the content root of `projectKey` is open. Show what waits for it. */
export function releaseToastsForRoot(projectKey: string): void {
  const due = pendingForRoot.filter((p) => p.projectKey === projectKey);
  if (due.length === 0) return;
  pendingForRoot = pendingForRoot.filter((p) => p.projectKey !== projectKey);
  for (const { input } of due) showToast(input);
}

/** Tests only: the toasts that wait for a slot. */
export function queuedToasts(): readonly ToastEntry[] {
  return queue;
}

/** Tests only: the visible toasts, newest first. */
export function visibleToasts(): readonly ToastEntry[] {
  return snapshot;
}

/** Tests only: start from an empty stack. */
export function resetToasts(): void {
  visible = [];
  queue = [];
  pendingForRoot = [];
  publish();
}

function subscribe(notify: () => void): () => void {
  subscribers.add(notify);
  return () => {
    subscribers.delete(notify);
  };
}

/** The visible toasts, newest first, for the stack component. */
export function useToasts(): readonly ToastEntry[] {
  return useSyncExternalStore(subscribe, () => snapshot);
}
