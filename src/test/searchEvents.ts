/**
 * Test scaffolding for the search event bus (`SCC-search.md` contract surface).
 *
 * Search is the first surface whose results arrive on the **event bus** rather
 * than as a command's return value, so a test that only stubs `invoke` sees an
 * overlay that never fills. This gives tests a `listen` they can drive: they
 * emit `"search results"` batches and the `"search ended"` terminal event
 * exactly as the backend would, including out of order and before the
 * dispatching `invoke` has resolved.
 */
import { SEARCH_ENDED, SEARCH_RESULTS } from "../events";
import type {
  SearchEndReason,
  SearchGroup,
  SearchHit,
  SearchMatchKind,
} from "../types";

type Handler = (event: { payload: unknown }) => void;

export interface EventBus {
  /** Drop-in for `@tauri-apps/api/event`'s `listen`. */
  listen: (name: string, handler: Handler) => Promise<() => void>;
  /** Deliver a payload to every subscriber of `name`. */
  emit: (name: string, payload: unknown) => void;
  /** How many subscribers `name` currently has. */
  listenerCount: (name: string) => number;
  /** Forget every subscription (between tests). */
  reset: () => void;
}

export function createEventBus(): EventBus {
  const handlers = new Map<string, Set<Handler>>();
  return {
    listen: async (name, handler) => {
      const set = handlers.get(name) ?? new Set<Handler>();
      set.add(handler);
      handlers.set(name, set);
      return () => {
        set.delete(handler);
      };
    },
    emit: (name, payload) => {
      // Copied before iterating: a handler that unsubscribes on delivery (the
      // terminal event is exactly that case) must not mutate the set mid-loop.
      [...(handlers.get(name) ?? [])].forEach((h) => h({ payload }));
    },
    listenerCount: (name) => handlers.get(name)?.size ?? 0,
    reset: () => handlers.clear(),
  };
}

/** Stream a batch of hits for `searchId`, as `"search results"` would. */
export function emitResults(
  bus: EventBus,
  searchId: string,
  hits: SearchHit[],
): void {
  bus.emit(SEARCH_RESULTS, { searchId, hits });
}

/** End `searchId`, as `"search ended"` would (SCC-FR-13: exactly once). */
export function emitEnded(
  bus: EventBus,
  searchId: string,
  reason: SearchEndReason = "completed",
): void {
  bus.emit(SEARCH_ENDED, { searchId, reason });
}

let nextOrdinal = 0;

/** Reset the ordinal counter so each test's hits start from a known base. */
export function resetHitOrdinals(): void {
  nextOrdinal = 0;
}

/**
 * A `SearchHit` with sensible defaults — a content match in the Files group, at
 * the next ordinal. Every field the contract carries is overridable, so a test
 * says only what it is actually about.
 */
export function hit(overrides: Partial<SearchHit> = {}): SearchHit {
  const ordinal = overrides.ordinal ?? nextOrdinal++;
  const path = overrides.path ?? `src/file-${ordinal}.rs`;
  const group: SearchGroup = overrides.group ?? "file";
  const matchKind: SearchMatchKind = overrides.matchKind ?? "content";
  return {
    id: overrides.id ?? path,
    name: overrides.name ?? (path.split("/").pop() as string),
    path,
    ordinal,
    group,
    matchKind,
    ...(matchKind === "content"
      ? { line: overrides.line ?? 1, snippet: overrides.snippet ?? "a matching line" }
      : {}),
    ...overrides,
  };
}
