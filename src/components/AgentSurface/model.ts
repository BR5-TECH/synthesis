/**
 * The local data models of the open surfaces (`ASF-agent-surfaces.md`
 * ASF-FR-ENRG, ASF-FR-JLUJ).
 *
 * One model per surface id, held in memory for as long as the window runs, so
 * a panel that moves to another owner surface keeps what the author typed. A
 * relaunch starts each model again from the record.
 */
import { useCallback, useSyncExternalStore } from "react";

import { setAt } from "./pointer";

const models = new Map<string, Record<string, unknown>>();
/** ASF-FR-MLKG: the selections of the cards, per surface and card id. */
const cards = new Map<string, unknown>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** For tests: forget every local model. */
export function resetSurfaceModelsForTest(): void {
  models.clear();
  cards.clear();
  notify();
}

/** The local model of `surfaceId`, started from `initial` on first use. */
export function useSurfaceModel(
  surfaceId: string,
  initial: Record<string, unknown>,
): [Record<string, unknown>, (pointer: string, value: unknown) => void] {
  const model = useSyncExternalStore(
    subscribe,
    () => models.get(surfaceId) ?? initial,
    () => models.get(surfaceId) ?? initial,
  );
  const write = useCallback(
    (pointer: string, value: unknown) => {
      const current = models.get(surfaceId) ?? initial;
      models.set(surfaceId, setAt(current, pointer, value));
      notify();
    },
    [surfaceId, initial],
  );
  return [model, write];
}


/** ASF-FR-MLKG: a card's selection, kept as long as the local model is. */
export function useCardState<T>(
  surfaceId: string,
  cardId: string,
  initial: T,
): [T, (next: T) => void] {
  const key = `${surfaceId}\u0000${cardId}`;
  const value = useSyncExternalStore(
    subscribe,
    () => (cards.has(key) ? (cards.get(key) as T) : initial),
    () => (cards.has(key) ? (cards.get(key) as T) : initial),
  );
  const set = useCallback(
    (next: T) => {
      cards.set(key, next);
      notify();
    },
    [key],
  );
  return [value, set];
}
