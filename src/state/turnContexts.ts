/**
 * DDS-FR-KXWQ / DDS-FR-TSPL: the context figures of the latest turn of each
 * discussion, as the turn events reported them.
 *
 * The session store keeps outstanding turns only, so a turn that has ended
 * leaves it. The context indicator still reads that turn's figures, so this
 * store keeps them per discussion id. It is memory only, like the turn record
 * it copies (AGC-FR-NUKP): a reload of the window starts it empty, and the
 * indicator then shows the estimate.
 */
import { useSyncExternalStore } from "react";

import type { AgentTurn, TurnContext } from "../types";

/** The figures of one turn, and which turn and agent they belong to. */
export interface HeldTurnContext {
  turnId: string;
  agentId: string;
  startedAt: string;
  context: TurnContext;
}

let held: ReadonlyMap<string, HeldTurnContext> = new Map();
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

function startedMs(startedAt: string): number {
  const ms = Date.parse(startedAt);
  return Number.isNaN(ms) ? -Infinity : ms;
}

/**
 * Keep the figures of a turn event. A turn that started before the turn held
 * for the same discussion does not replace it, because the indicator reads the
 * latest turn. An event without figures changes nothing.
 */
export function noteTurnContext(turn: AgentTurn): void {
  const context = turn.context;
  const key = turn.origin.discussionId;
  if (!context || !key) return;
  const prior = held.get(key);
  if (
    prior &&
    prior.turnId !== turn.id &&
    startedMs(prior.startedAt) > startedMs(turn.startedAt)
  ) {
    return;
  }
  if (prior && prior.turnId === turn.id && prior.context === context) return;
  const map = new Map(held);
  map.set(key, {
    turnId: turn.id,
    agentId: turn.agentId,
    startedAt: turn.startedAt,
    context,
  });
  held = map;
  emit();
}

/** The figures held for one discussion, or `null`. */
export function getTurnContext(discussionId: string): HeldTurnContext | null {
  return held.get(discussionId) ?? null;
}

/** CVP-FR-49: discard every held figure, on a project or worktree change. */
export function clearTurnContexts(): void {
  if (held.size === 0) return;
  held = new Map();
  emit();
}

/** Test seam: start each test with an empty store. */
export function resetTurnContextsForTest(): void {
  held = new Map();
}

/** The figures held for one discussion, in React. */
export function useTurnContext(discussionId: string | null): HeldTurnContext | null {
  const key = discussionId ?? "";
  return useSyncExternalStore(
    subscribe,
    () => (key === "" ? null : getTurnContext(key)),
    () => (key === "" ? null : getTurnContext(key)),
  );
}
