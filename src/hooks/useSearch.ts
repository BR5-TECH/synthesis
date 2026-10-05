/**
 * The universal search bar's dispatch and accumulation (`SCH-search.md`).
 *
 * Owns the four things the UI side of search is responsible for: the typing
 * debounce that decides *when* a search is dispatched (SCH-FR-15), accumulation
 * of the streamed hits and their ordering (SCH-FR-16), the in-progress /
 * empty / error states the overlay renders (SCH-FR-19 / SCH-FR-21), and
 * cancellation (SCH-FR-20).
 *
 * Both presentation levels use it: the overlay mounts it with
 * `scope: "capped"`, the Search results tab with `scope: "full"` (SCH-FR-18).
 * They are two independent searches over the same query — dismissing the
 * overlay cancels only the overlay's.
 *
 * Event batches are **buffered by search id** rather than assumed to arrive
 * after the dispatch resolves. The backend streams on a channel that is already
 * live when `start_search` is invoked, so a search that ends before its own
 * `invoke` promise settles — an empty project, a cap hit on the first file —
 * would otherwise deliver its whole result set into a hook that did not yet know
 * the id to accept it under, and the overlay would sit on an in-progress state
 * for a search that had already finished.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { onSearchEnded, onSearchResults } from "../events";
import type {
  SearchEndReason,
  SearchHit,
  SearchMode,
  SearchScope,
} from "../types";

/**
 * SCH non-functional requirement: short enough to feel immediate on a
 * deliberate query, long enough that ordinary typing speed dispatches once.
 */
export const TYPING_PAUSE_MS = 200;

/**
 * How long a consumer that retries a superseded search waits before
 * re-dispatching (see `retryOnSuperseded`). Long enough that a user still
 * typing in the overlay wins the next round rather than the two trading
 * supersessions keystroke for keystroke; short enough to be invisible once the
 * typing stops.
 */
export const SUPERSEDE_RETRY_MS = 300;

/** The typed error `start_search` rejects an uncompilable pattern with. */
const INVALID_QUERY = "invalid query";

export interface SearchState {
  /** Accumulated hits, ascending by `ordinal` (SCH-FR-16). */
  hits: SearchHit[];
  /** A search has been dispatched and has not yet ended (SCH-FR-19). */
  running: boolean;
  /** `"search ended"` arrived for the current search (SCH-FR-19). */
  ended: boolean;
  /** Why it ended, or null while it runs / before anything was dispatched. */
  reason: SearchEndReason | null;
  /** SCH-FR-21: the query did not compile; rendered in place of the groups. */
  error: string | null;
  /** SCH-FR-20: stop the search in flight and clear the result set. */
  cancel: () => void;
}

export interface UseSearchOptions {
  query: string;
  mode: SearchMode;
  /** SCH-FR-18: `capped` for the overlay, `full` for the Search results tab. */
  scope: SearchScope;
  /**
   * Whether this consumer wants a search at all. The overlay passes its open
   * state, so closing it cancels (SCH-FR-20); the tab passes `true` for its
   * whole life, so its sweep survives the overlay closing.
   */
  enabled?: boolean;
  /**
   * SCH-FR-15's typing pause. The Search results tab passes 0: its query is
   * fixed at open, so there is no typing to wait out.
   */
  debounceMs?: number;
  /**
   * Re-dispatch instead of ending when this search is **superseded**.
   *
   * At most one search runs at a time (SCC-FR-12), so a keystroke in the
   * overlay supersedes the Search results tab's full sweep — while SCH-FR-20
   * requires that sweep to survive the overlay entirely. Retrying is how the
   * tab honours both: it is superseded, waits for the overlay to settle, and
   * runs again, converging on the complete answer SCH-FR-11 then preserves.
   *
   * The overlay leaves this off: its search being superseded means the user
   * typed again, and that newer search is the one they want.
   */
  retryOnSuperseded?: boolean;
}

/** Whether the query text is worth dispatching at all (SCH-FR-15). */
function isDispatchable(query: string): boolean {
  return query.trim().length > 0;
}

/**
 * How many foreign search ids may sit in the pre-dispatch buffers at once.
 *
 * Only the *next* dispatch's id is ever drained from them, so without a bound a
 * long-lived consumer — the Search results tab, which dispatches once and then
 * listens for the rest of its life — would accumulate every batch of every
 * other search the session ever runs. Two is generous: the buffer exists for a
 * search whose events beat its own `invoke` home, and there is at most one such
 * search at a time.
 */
const MAX_BUFFERED_SEARCHES = 2;

/** Drop the oldest buffered ids once past the bound (insertion-ordered Maps). */
function pruneBuffers(
  hits: Map<string, SearchHit[]>,
  ends: Map<string, SearchEndReason>,
): void {
  while (hits.size > MAX_BUFFERED_SEARCHES) {
    const oldest = hits.keys().next().value as string;
    hits.delete(oldest);
    ends.delete(oldest);
  }
  while (ends.size > MAX_BUFFERED_SEARCHES) {
    ends.delete(ends.keys().next().value as string);
  }
}

export function useSearch({
  query,
  mode,
  scope,
  enabled = true,
  debounceMs = TYPING_PAUSE_MS,
  retryOnSuperseded = false,
}: UseSearchOptions): SearchState {
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [running, setRunning] = useState(false);
  const [ended, setEnded] = useState(false);
  const [reason, setReason] = useState<SearchEndReason | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** The id of the search whose events this hook is currently accepting. */
  const currentId = useRef<string | null>(null);
  /**
   * Batches and terminations that arrived before their dispatch resolved. Keyed
   * by search id and drained the moment the id becomes known; cleared on every
   * new dispatch, so a superseded search's late batches are dropped rather than
   * accumulating for the life of the session.
   */
  const buffered = useRef(new Map<string, SearchHit[]>());
  const bufferedEnd = useRef(new Map<string, SearchEndReason>());
  /**
   * Increments on every dispatch. An `invoke` that resolves after a newer
   * dispatch has already started compares its token and declines to install
   * itself, so a slow round-trip cannot make the UI adopt a stale search's id.
   */
  const dispatchToken = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /**
   * Set on unmount. A dispatch whose `invoke` is still in flight when the
   * consumer goes away has no id to cancel yet, so the id is cancelled the
   * moment it arrives instead — otherwise closing a Search results tab within
   * the round-trip window leaves a full sweep running with nobody listening.
   */
  const unmounted = useRef(false);

  /**
   * SCH-FR-16: hits render in ascending `ordinal` and an already-visible result
   * never changes position. Batches arrive unordered (consumers finish files in
   * whatever order they finish them, SCC-FR-10), so the merge inserts each hit
   * at its ordinal rather than appending — which is what makes the list grow
   * *downward under a stable reading position* rather than reshuffling.
   */
  const absorb = useCallback((incoming: SearchHit[]) => {
    if (incoming.length === 0) return;
    setHits((previous) => {
      const seen = new Set(previous.map((h) => h.id));
      // SCC-FR-07: at most one hit per file. A duplicate id can only be a
      // redelivered batch, and admitting it would double a rendered row.
      const fresh = incoming.filter((h) => !seen.has(h.id));
      if (fresh.length === 0) return previous;
      return [...previous, ...fresh].sort((a, b) => a.ordinal - b.ordinal);
    });
  }, []);

  /** Bumped when a superseded search is owed a retry; drives the effect below. */
  const [retryNonce, setRetryNonce] = useState(0);

  const finish = useCallback(
    (why: SearchEndReason) => {
      // A superseded search this consumer means to retry has not ended as far
      // as the surface is concerned: leaving `running` true is what keeps the
      // page showing "Searching…" instead of flashing a truncated result set —
      // or "No results" — for an answer that is about to be asked again.
      if (why === "superseded" && retryOnSuperseded) {
        setHits([]);
        setRetryNonce((n) => n + 1);
        return;
      }
      setRunning(false);
      setEnded(true);
      setReason(why);
    },
    [retryOnSuperseded],
  );

  // One subscription per mount, for the life of the hook. Batches for an id this
  // hook has not adopted yet are buffered rather than dropped.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void onSearchResults((payload) => {
      if (payload.searchId === currentId.current) {
        absorb(payload.hits);
        return;
      }
      const pending = buffered.current.get(payload.searchId) ?? [];
      buffered.current.set(payload.searchId, [...pending, ...payload.hits]);
      pruneBuffers(buffered.current, bufferedEnd.current);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    void onSearchEnded((payload) => {
      if (payload.searchId === currentId.current) {
        finish(payload.reason);
        return;
      }
      bufferedEnd.current.set(payload.searchId, payload.reason);
      pruneBuffers(buffered.current, bufferedEnd.current);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [absorb, finish]);

  /**
   * SCH-FR-20: stop the search in flight and return to an empty, un-ended state.
   * Cancelling is best-effort — an id the backend has already retired is a no-op
   * there (SCC-FR-14), so there is nothing to recover from.
   */
  const cancel = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    dispatchToken.current += 1;
    const inFlight = currentId.current;
    currentId.current = null;
    buffered.current.clear();
    bufferedEnd.current.clear();
    if (inFlight) void api.cancelSearch(inFlight).catch(() => {});
    setHits([]);
    setRunning(false);
    setEnded(false);
    setReason(null);
    setError(null);
  }, []);

  /**
   * The dispatch itself. Clears the previous result set before the round-trip so
   * the overlay never shows one query's hits under another's text, and adopts
   * whatever the buffer already holds for the returned id.
   */
  const dispatch = useCallback(
    (text: string, activeMode: SearchMode) => {
      const token = ++dispatchToken.current;
      currentId.current = null;
      buffered.current.clear();
      bufferedEnd.current.clear();
      setHits([]);
      setRunning(true);
      setEnded(false);
      setReason(null);
      setError(null);
      // SCC-FR-12: starting supersedes anything still running, so the previous
      // search needs no explicit cancel here.
      void api
        .startSearch(text, activeMode, scope)
        .then((id) => {
          // This dispatch was abandoned while its round-trip was in flight —
          // the consumer unmounted, the overlay was dismissed, or the query was
          // edited down to empty. At the moment of abandonment there was no id
          // yet to cancel, so it is cancelled here instead; otherwise the sweep
          // runs to completion with nobody listening (SCH-FR-15 / SCH-FR-20).
          //
          // A stale token can also mean a *newer* dispatch replaced this one,
          // which the backend already superseded (SCC-FR-12). Cancelling an id
          // that is no longer the running search is a no-op there (SCC-FR-14),
          // so one branch safely covers both.
          if (unmounted.current || token !== dispatchToken.current) {
            void api.cancelSearch(id).catch(() => {});
            return;
          }
          currentId.current = id;
          // Anything that landed before the id was known.
          const early = buffered.current.get(id);
          if (early) {
            buffered.current.delete(id);
            absorb(early);
          }
          const earlyEnd = bufferedEnd.current.get(id);
          if (earlyEnd) {
            bufferedEnd.current.delete(id);
            finish(earlyEnd);
          }
        })
        .catch((e) => {
          if (token !== dispatchToken.current) return;
          // SCH-FR-21: an uncompilable regular expression is rendered in place
          // of the result groups, with the query and mode left intact so the
          // user can correct the pattern where they typed it. Nothing was
          // started, so there is no search to end (SCC-FR-06).
          const message = String(e);
          setError(message.includes(INVALID_QUERY) ? INVALID_QUERY : message);
          setRunning(false);
          setEnded(true);
          currentId.current = null;
        });
    },
    [absorb, finish, scope],
  );

  /**
   * SCH-FR-15 / SCH-FR-14: a keystroke restarts the typing pause; a mode change
   * dispatches immediately, because it is a deliberate click rather than a
   * keystroke in progress.
   */
  const previous = useRef<{ query: string; mode: SearchMode } | null>(null);
  useEffect(() => {
    if (!enabled) return;
    const modeChanged = previous.current !== null && previous.current.mode !== mode;
    const queryChanged =
      previous.current === null || previous.current.query !== query;
    previous.current = { query, mode };
    if (!queryChanged && !modeChanged) return;

    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }

    // SCH-FR-15: a query edited down to empty dispatches nothing, cancels any
    // search in flight, and leaves the overlay empty.
    if (!isDispatchable(query)) {
      cancel();
      previous.current = { query, mode };
      return;
    }

    const wait = modeChanged && !queryChanged ? 0 : debounceMs;
    if (wait === 0) {
      dispatch(query, mode);
      return;
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      dispatch(query, mode);
    }, wait);
  }, [query, mode, enabled, debounceMs, dispatch, cancel]);

  /**
   * The retry a superseded search is owed. Keyed on the nonce alone and reading
   * the query through a ref: keying it on `query`/`mode` too would re-arm the
   * timer on every keystroke, which is the opposite of waiting for the typing
   * to settle.
   */
  const latest = useRef({ query, mode, enabled });
  latest.current = { query, mode, enabled };
  useEffect(() => {
    if (retryNonce === 0) return;
    const timeout = setTimeout(() => {
      const now = latest.current;
      if (!now.enabled || !isDispatchable(now.query)) return;
      dispatch(now.query, now.mode);
    }, SUPERSEDE_RETRY_MS);
    return () => clearTimeout(timeout);
  }, [retryNonce, dispatch]);

  /**
   * SCH-FR-20: the consumer went away — the overlay was dismissed, or the
   * Search results tab closed. Either way its search is cancelled, and the
   * other's is untouched because each mounts its own hook.
   */
  useEffect(() => {
    if (enabled) return;
    previous.current = null;
    cancel();
  }, [enabled, cancel]);

  useEffect(() => {
    // Guards against React re-running this effect (Strict Mode mounts twice):
    // the flag must describe the CURRENT mount, not a previous one's teardown.
    unmounted.current = false;
    return () => {
      unmounted.current = true;
      if (timer.current !== null) clearTimeout(timer.current);
      const inFlight = currentId.current;
      currentId.current = null;
      if (inFlight) void api.cancelSearch(inFlight).catch(() => {});
    };
  }, []);

  return { hits, running, ended, reason, error, cancel };
}
