/**
 * What one selected scope of the graduation log window holds, and how it grows
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-LUQI,
 * GLW-FR-QDWA, GLW-FR-RQEV, GLW-FR-NFLN).
 *
 * The window holds no source of truth (GLW-FR-THAX). Every record it shows came
 * back through the one read it invokes, every read is bounded to one page
 * (GLW-FR-ZPUH), and an answer for a scope the reader has since left is
 * discarded rather than merged (GLW-FR-NFLN).
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { readGraduationLogs } from "../../api";
import { onGraduationLogRecordsAppended } from "../../events";
import { logDebug, logWarn } from "../../logging";
import { scopeKey, scopeOf, type LogScopeEntry } from "../../state/graduation/logScopes";
import { acceptPage, type PageScope } from "./pages";
import type {
  GraduationLogCursor,
  GraduationLogFailure,
  GraduationLogPage,
  GraduationLogPageEntry,
  GraduationLogReadStatus,
  GraduationLogSearchResult,
  GraduationLogStream,
} from "../../types";

/** GLW-FR-ZPUH: one page, never a whole scope. */
export const PAGE_LIMIT = 200;

export interface LogScopeState {
  entries: GraduationLogPageEntry[];
  status: GraduationLogReadStatus | null;
  search: GraduationLogSearchResult;
  matchedTotal: number;
  failure: GraduationLogFailure | null;
  olderCursor: GraduationLogCursor | null;
  oldestReached: boolean;
  latestSequence: number;
  /** GLW-FR-NMOD: a read is in flight and nothing is yet held. */
  loading: boolean;
  /** GLW-FR-UZAB: its own indication, and none of the five states. */
  loadingOlder: boolean;
  olderFailed: boolean;
  /** GLW-FR-OJXH: the read failed and the window says so rather than blanking. */
  readError: string | null;
}

const EMPTY: LogScopeState = {
  entries: [],
  status: null,
  search: "not_requested",
  matchedTotal: 0,
  failure: null,
  olderCursor: null,
  oldestReached: true,
  latestSequence: 0,
  loading: true,
  loadingOlder: false,
  olderFailed: false,
  readError: null,
};

/**
 * GLW-FR-RQEV: pages merge in ascending sequence order and never duplicate.
 *
 * A record the window already holds is not held a second time however the pages
 * overlap, so an older page that touches the newest one costs nothing.
 */
export function merge(
  held: GraduationLogPageEntry[],
  arriving: GraduationLogPageEntry[],
): GraduationLogPageEntry[] {
  const sequenceOf = (entry: GraduationLogPageEntry) =>
    typeof entry.record.sequence === "number" ? entry.record.sequence : 0;
  const bySequence = new Map<number, GraduationLogPageEntry>();
  for (const entry of held) bySequence.set(sequenceOf(entry), entry);
  for (const entry of arriving) bySequence.set(sequenceOf(entry), entry);
  return [...bySequence.values()].sort((a, b) => sequenceOf(a) - sequenceOf(b));
}

/**
 * GLW-FR-VRTC: what a page says about the scope, unless a newer page already
 * said it. A page behind the newest one merged adds its records and replaces
 * neither the status, the count, nor the newest sequence.
 */
function describe(
  held: LogScopeState,
  page: GraduationLogPage,
): Pick<
  LogScopeState,
  "status" | "search" | "matchedTotal" | "failure" | "latestSequence"
> {
  if (held.status !== null && page.latestSequence < held.latestSequence) {
    return {
      status: held.status,
      search: held.search,
      matchedTotal: held.matchedTotal,
      failure: held.failure,
      latestSequence: held.latestSequence,
    };
  }
  return {
    status: page.status,
    search: page.search,
    matchedTotal: page.matchedTotal,
    failure: page.failure ?? null,
    latestSequence: page.latestSequence,
  };
}

function newestSequence(entries: GraduationLogPageEntry[]): number {
  const last = entries[entries.length - 1];
  return last && typeof last.record.sequence === "number"
    ? last.record.sequence
    : 0;
}

export interface LogScopeRequest {
  runId: string;
  phaseId: string;
  entry: LogScopeEntry | null;
  stream: GraduationLogStream;
  /** GLW-FR-IMKM: applied to the selected stream and scope alone. */
  query: string;
}

export interface LogScope {
  state: LogScopeState;
  /** GLW-FR-QDWA: ask for the page before the oldest record held. */
  loadOlder: () => void;
  /** GLW-FR-OJXH: read the newest page again after a failure. */
  reload: () => void;
}

export function useLogScope(request: LogScopeRequest): LogScope {
  const { runId, phaseId, entry, stream, query } = request;
  const [state, setState] = useState<LogScopeState>(EMPTY);
  const key = `${scopeKey(runId, phaseId, entry, stream)} ${query}`;
  const keyRef = useRef(key);
  keyRef.current = key;
  // The entry's **key** is what a scope change is decided from, never the
  // object: a reload of the run's listing hands the surface an equal entry in a
  // new object, and re-reading on that would throw away the page and the
  // reader's place in it for no change at all.
  const entryRef = useRef(entry);
  entryRef.current = entry;
  const entryKey = entry?.key ?? null;
  const scopeRef = useRef<PageScope>({ runId, phaseId, entry, stream });
  scopeRef.current = { runId, phaseId, entry, stream };
  // GLW-FR-GZWN: the newest page is in flight, and an append that arrived in
  // the meantime waits for it, so no record falls between the two reads.
  const firstInFlight = useRef(false);
  const appendWaiting = useRef(false);
  const readAfter = useRef<(() => void) | null>(null);
  // The newest sequence held, kept beside the state so an arriving record can
  // be asked for without reading state inside an updater.
  const newestRef = useRef(0);
  // GLW-FR-QDWA: at most one older-page request in flight at a time.
  const olderInFlight = useRef(false);
  const [attempt, setAttempt] = useState(0);

  const apply = useCallback((forKey: string, page: GraduationLogPage) => {
    // GLW-FR-NFLN: an answer for a scope no longer selected is discarded.
    if (keyRef.current !== forKey) return;
    setState((held) => {
      // The newest page is what a scope opens on, and an append that arrived
      // while it was in flight is merged into it rather than dropped.
      const merged = merge(held.entries, page.entries);
      newestRef.current = newestSequence(merged);
      return {
        entries: merged,
        ...describe(held, page),
        olderCursor:
          held.entries.length === 0
            ? (page.olderCursor ?? null)
            : held.olderCursor,
        oldestReached:
          held.entries.length === 0 ? page.oldestReached : held.oldestReached,
        loading: false,
        loadingOlder: false,
        olderFailed: false,
        readError: null,
      };
    });
  }, []);

  // GLW-FR-LUQI: the newest page whenever a scope is selected.
  useEffect(() => {
    const selected = entryRef.current;
    if (!selected) {
      setState({ ...EMPTY, loading: false, status: "empty" });
      return;
    }
    const forKey = key;
    setState({ ...EMPTY, loading: true });
    newestRef.current = 0;
    olderInFlight.current = false;
    firstInFlight.current = true;
    appendWaiting.current = false;
    let cancelled = false;
    void readGraduationLogs({
      runId,
      phaseId,
      pass: scopeOf(selected),
      stream,
      limit: PAGE_LIMIT,
      query: query.trim() === "" ? null : query,
    })
      .then((page) => {
        if (cancelled) return;
        firstInFlight.current = false;
        const accepted = acceptPage(page, scopeRef.current);
        if (!accepted) {
          logWarn(["frontend"], "the run log page answered for another scope", {
            runId,
            phaseId,
            stream,
          });
          if (keyRef.current === forKey) {
            setState({
              ...EMPTY,
              loading: false,
              readError: "The log answered for another run, stage, or stream.",
            });
          }
          return;
        }
        apply(forKey, accepted);
        // GLW-FR-GZWN: an append that arrived while this page was in flight.
        if (appendWaiting.current) {
          appendWaiting.current = false;
          // The state updater has not run yet, so the cursor is set here.
          newestRef.current = Math.max(
            newestRef.current,
            newestSequence(accepted.entries),
          );
          readAfter.current?.();
        }
      })
      .catch((error: unknown) => {
        if (cancelled || keyRef.current !== forKey) return;
        firstInFlight.current = false;
        logWarn(["frontend"], "the run log page could not be read", {
          runId,
          phaseId,
          stream,
        });
        setState({
          ...EMPTY,
          loading: false,
          readError: String(error),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [runId, phaseId, entryKey, stream, query, key, apply, attempt]);

  // GLW-FR-LUQI: told that this run and stream grew, the window asks for the
  // records after the newest it holds. It renders nothing from the payload.
  useEffect(() => {
    if (!entryKey) return;
    const forKey = key;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    const read = () => {
      const selected = entryRef.current;
      if (!selected || cancelled || keyRef.current !== forKey) return;
      void readGraduationLogs({
        runId,
        phaseId,
        pass: scopeOf(selected),
        stream,
        cursor: {
          runId,
          stream,
          direction: "after",
          sequence: newestRef.current,
        },
        limit: PAGE_LIMIT,
        query: query.trim() === "" ? null : query,
      })
        .then((answer) => {
          if (cancelled || keyRef.current !== forKey) return;
          // GLW-FR-VRTC: another run, phase, scope, or stream is not shown.
          const page = acceptPage(answer, scopeRef.current);
          if (!page) {
            logWarn(["frontend"], "a run log append answered for another scope", {
              runId,
              phaseId,
              stream,
            });
            return;
          }
          setState((current) => {
            const merged = merge(current.entries, page.entries);
            newestRef.current = newestSequence(merged);
            return {
              ...current,
              entries: merged,
              ...describe(current, page),
              // GLW-FR-QDWA: an append never says the beginning is on screen.
              oldestReached:
                current.entries.length === 0
                  ? page.oldestReached
                  : current.oldestReached,
              olderCursor:
                current.entries.length === 0
                  ? (page.olderCursor ?? null)
                  : current.olderCursor,
              loading: false,
            };
          });
        })
        .catch(() => {
          logDebug(["frontend"], "a run log append could not be read back", {
            runId,
            stream,
          });
        });
    };
    readAfter.current = read;
    void onGraduationLogRecordsAppended((payload) => {
      if (cancelled) return;
      if (payload.runId !== runId || payload.stream !== stream) return;
      if (keyRef.current !== forKey) return;
      // GLW-FR-GZWN: the newest page has not arrived, so read after it lands.
      if (firstInFlight.current) {
        appendWaiting.current = true;
        return;
      }
      read();
    }).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      if (readAfter.current === read) readAfter.current = null;
      unlisten?.();
    };
  }, [runId, phaseId, entryKey, stream, query, key]);

  const loadOlder = useCallback(() => {
    const forKey = keyRef.current;
    const selected = entryRef.current;
    if (olderInFlight.current || !selected) return;
    const cursor = state.olderCursor;
    if (!cursor || state.oldestReached) return;
    olderInFlight.current = true;
    setState((held) => ({ ...held, loadingOlder: true, olderFailed: false }));
    void readGraduationLogs({
      runId,
      phaseId,
      pass: scopeOf(selected),
      stream,
      cursor,
      limit: PAGE_LIMIT,
      query: query.trim() === "" ? null : query,
    })
      .then((answer) => {
        // GLW-FR-NFLN: an answer for a scope the reader has left is discarded,
        // and it does not release the guard the scope they are on now holds.
        if (keyRef.current !== forKey) return;
        olderInFlight.current = false;
        const page = acceptPage(answer, scopeRef.current);
        if (!page) {
          // GLW-FR-VRTC: what is held stays, and the window offers to ask again.
          setState((held) => ({ ...held, loadingOlder: false, olderFailed: true }));
          return;
        }
        setState((held) => ({
          ...held,
          entries: merge(page.entries, held.entries),
          olderCursor: page.olderCursor ?? null,
          oldestReached: page.oldestReached,
          matchedTotal: page.matchedTotal,
          loadingOlder: false,
          olderFailed: false,
        }));
      })
      .catch(() => {
        if (keyRef.current !== forKey) return;
        olderInFlight.current = false;
        // GLW-FR-UZAB: what is held stays on screen, and the window offers to
        // ask again rather than emptying itself.
        setState((held) => ({ ...held, loadingOlder: false, olderFailed: true }));
      });
  }, [phaseId, query, runId, state.oldestReached, state.olderCursor, stream]);

  const reload = useCallback(() => setAttempt((at) => at + 1), []);

  return { state, loadOlder, reload };
}
