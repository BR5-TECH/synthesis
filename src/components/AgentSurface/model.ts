/**
 * The local data models of the open surfaces (`ASF-agent-surfaces.md`
 * ASF-FR-ENRG, ASF-FR-JLUJ).
 *
 * One model per surface id, held in memory for as long as the window runs, so
 * a panel that moves to another owner surface keeps what the author typed. A
 * relaunch starts each model again from the record.
 */
import { useCallback, useSyncExternalStore } from "react";


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
  cards.clear();
  notify();
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
