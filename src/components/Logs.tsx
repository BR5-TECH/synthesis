import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { browseForSavePath, exportLogs, queryLogs } from "../api";
import { onLogRecordsAppended } from "../events";
import { LOG_DOMAINS, LOG_LEVELS } from "../types";
import type {
  LogBufferState,
  LogCursor,
  LogDomain,
  LogFilter,
  LogLevel,
  LogPage,
  LogRecord,
} from "../types";

/**
 * The Logs bottom-panel surface (`../../specifications/ui/LOG-logs.md`).
 *
 * The session's diagnostic records, narrowed by level, by domain, and by text
 * until only the interesting ones remain. **The panel evaluates no filter and
 * matches no text itself** (LOG-FR-06): every act of narrowing is a
 * `queryLogs` call, so what renders is exactly what the backend returned. That
 * is the whole shape of this component — it holds filter values, a page of
 * records, and view state, and asks the backend for everything else.
 *
 * It is mounted only while Logs is the bottom panel's active surface and that
 * panel is visible, which is what implements LOG-FR-11: while the user is
 * looking at Runs, Git, or History — or at nothing — this issues no query and
 * performs no redraw however many records arrive.
 */

/** How many records one page holds (LOG-FR-15). */
export const PAGE_SIZE = 200;

/**
 * The most rows kept mounted at once. Paging back repeatedly would otherwise
 * grow the DOM without bound; the oldest-loaded end is trimmed instead, which is
 * what keeps scroll cost independent of how long the session has been running
 * (LOG non-functional requirements).
 */
export const MAX_LOADED = 1_000;

/** How long the search box waits after a keystroke before querying. */
export const SEARCH_DEBOUNCE_MS = 150;

const DEFAULT_FILTER: LogFilter = {
  // LOG-FR-07: a panel opened for the first time in a session hides nothing.
  minLevel: "DEBUG",
  domains: [...LOG_DOMAINS],
  query: "",
  queryIsRegex: false,
};

/**
 * LOG-FR-08: filter values live for the session, not for the mount.
 *
 * Module scope rather than component state because the panel is *unmounted*
 * whenever the bottom panel switches to another surface or hides — component
 * state would reset every time the user glanced at Git, which is precisely what
 * LOG-FR-08 forbids. Nothing here is persisted: a relaunched application starts
 * a fresh module and therefore the defaults of LOG-FR-07.
 */
let sessionFilter: LogFilter = { ...DEFAULT_FILTER };

/** Reset the session-scoped filter. For tests. */
export function resetLogFilterForTest(): void {
  sessionFilter = { ...DEFAULT_FILTER };
}

/**
 * A record's timestamp in the viewer's local zone at millisecond precision
 * (LOG-FR-02). Exported for its own test — the stored stamp is UTC, and the
 * conversion is the part a reader depends on to correlate a record with
 * something they remember happening.
 */
export function formatLocalTime(ts: string): string {
  const at = new Date(ts);
  if (Number.isNaN(at.getTime())) return ts;
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  return `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(
    at.getSeconds(),
  )}.${pad(at.getMilliseconds(), 3)}`;
}

/** The domain column's text: the record's domains, in the panel's fixed order. */
function domainLabel(domains: LogDomain[]): string {
  return LOG_DOMAINS.filter((d) => domains.includes(d)).join("·");
}

/** Whether two filters would produce the same match set. */
function sameFilter(a: LogFilter, b: LogFilter): boolean {
  return (
    a.minLevel === b.minLevel &&
    a.queryIsRegex === b.queryIsRegex &&
    (a.query ?? "") === (b.query ?? "") &&
    a.domains.length === b.domains.length &&
    a.domains.every((d) => b.domains.includes(d))
  );
}

/** A filter's identity, for detecting that a page belongs to a different one. */
function filterKey(f: LogFilter): string {
  return JSON.stringify([f.minLevel, [...f.domains].sort(), f.query ?? "", f.queryIsRegex]);
}

interface Loaded {
  records: LogRecord[];
  generation: number;
  matchedTotal: number;
  bufferTotal: number;
  droppedTotal: number;
  /**
   * Which filter produced these rows. A single "last request wins" counter is
   * not enough on its own: a filter change and an append event can be in flight
   * together, and merging the new filter's delta into the old filter's rows
   * would render records the current filter excludes — precisely what LOG-FR-06
   * forbids.
   */
  key: string;
  /**
   * Set when a backward page came back empty: there is nothing older to load.
   * Without it every scroll gesture at the top re-issues the same exhausted
   * `before` query forever.
   */
  atOldest: boolean;
}

const EMPTY: Loaded = {
  records: [],
  generation: 0,
  matchedTotal: 0,
  bufferTotal: 0,
  droppedTotal: 0,
  key: "",
  atOldest: false,
};

/** What a backend that answered with something other than a page is read as. */
const EMPTY_PAGE: LogPage = {
  records: [],
  generation: 0,
  matchedTotal: 0,
  bufferTotal: 0,
  droppedTotal: 0,
  highestSequence: null,
};

export function Logs() {
  // Held in module scope (LOG-FR-08); mirrored into state so edits re-render.
  const [filter, setFilterState] = useState<LogFilter>(sessionFilter);
  const [loaded, setLoaded] = useState<Loaded>(EMPTY);
  const [queryError, setQueryError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [following, setFollowing] = useState(true);
  const [exportNote, setExportNote] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  const listRef = useRef<HTMLDivElement>(null);
  // The live filter, readable from callbacks that must not re-subscribe on
  // every keystroke.
  const filterRef = useRef(filter);
  filterRef.current = filter;
  // Written by `load` alone, never re-assigned during render: `load` reads it
  // back immediately when it pages forward to catch up, and a render-time
  // assignment could clobber what it had just written with the pre-commit value.
  const loadedRef = useRef(loaded);
  /** The row a pending prepend must keep under the pointer (LOG-FR-15). */
  const pendingAnchor = useRef<{ sequence: number; offsetTop: number } | null>(null);
  const followingRef = useRef(following);
  followingRef.current = following;
  // Discards a page whose request was superseded by a later one, so a slow
  // query never renders over a fresh result.
  const requestSeq = useRef(0);

  const setFilter = useCallback((patch: Partial<LogFilter>) => {
    sessionFilter = { ...sessionFilter, ...patch };
    setFilterState(sessionFilter);
    // The note describes an export of a set the filter has just changed, so it
    // stops being true the moment the filter moves.
    setExportNote(null);
  }, []);

  /**
   * Ask for a page and adopt it.
   *
   * LOG-FR-09: a pattern the backend rejects leaves the last matching list
   * rendered and reports the error beside the search box, rather than emptying
   * the panel while the user is still typing.
   */
  const load = useCallback(
    async (
      cursor: LogCursor | null,
      mode: "replace" | "append" | "prepend",
    ): Promise<LogPage | null> => {
      const token = ++requestSeq.current;
      const active = filterRef.current;
      const key = filterKey(active);
      // LOG-FR-15: a prepend inserts rows ABOVE what the user is reading, so
      // the scroll offset has to be re-anchored or the content they were
      // looking at is pushed off-screen — and, because `scrollTop` stays at 0,
      // the next scroll event requests yet another page.
      //
      // The anchor is a specific row's position, not the list's height. Height
      // cannot serve: at the MAX_LOADED cap a prepend adds rows at the head and
      // drops as many from the tail, so the height delta is ~0 while the
      // content has moved by a full page. Consumed in a layout effect, because
      // the measurement is only meaningful after React has committed the rows.
      const el = listRef.current;
      if (mode === "prepend" && el) {
        const first = loadedRef.current.records[0];
        const node = first
          ? el.querySelector<HTMLElement>(`[data-sequence="${first.sequence}"]`)
          : null;
        pendingAnchor.current =
          first && node ? { sequence: first.sequence, offsetTop: node.offsetTop } : null;
      }
      try {
        const answered = await queryLogs(active, cursor, PAGE_SIZE);
        if (token !== requestSeq.current) return null;
        // A backend that answered with something other than a page — a stub
        // that has not implemented the command, a harness serving `undefined` —
        // leaves the panel empty rather than crashing it on mount. Everything
        // below reads `page` as a `LogPage`, and this is the one place the
        // process boundary can hand us something else.
        const page: LogPage =
          answered && Array.isArray(answered.records) ? answered : EMPTY_PAGE;
        setQueryError(null);
        // Computed from the ref rather than inside the updater: a state updater
        // must be pure, and React may invoke it more than once per commit.
        const current = loadedRef.current;
        // LOG-FR-13: a page from a different generation describes a buffer that
        // has been cleared. A page from a different filter describes a different
        // match set. Either way, what is held cannot be merged into.
        const stale =
          mode !== "replace" &&
          (page.generation !== current.generation || key !== current.key);
        const merging = mode !== "replace" && !stale;
        let records: LogRecord[];
        if (mode === "prepend" && merging) {
          records = [...page.records, ...current.records].slice(0, MAX_LOADED);
        } else if (mode === "append" && merging) {
          const merged = [...current.records, ...page.records];
          records = merged.slice(Math.max(0, merged.length - MAX_LOADED));
        } else {
          records = page.records;
        }
        const next: Loaded = {
          records,
          generation: page.generation,
          matchedTotal: page.matchedTotal,
          bufferTotal: page.bufferTotal,
          droppedTotal: page.droppedTotal,
          key,
          // A prepend that came back empty means there is nothing older; any
          // other outcome re-opens the possibility.
          atOldest:
            mode === "prepend" && merging
              ? page.records.length === 0
              : mode === "replace"
                ? false
                : current.atOldest,
        };
        loadedRef.current = next;
        setLoaded(next);
        if (stale) {
          // The rows described a buffer or a match set that no longer exists,
          // so a selection or an expansion keyed to them names nothing.
          setExpanded(new Set());
          setSelected(new Set());
        }
        if (mode !== "prepend" || page.records.length === 0) {
          pendingAnchor.current = null;
        }
        return page;
      } catch (e) {
        if (token !== requestSeq.current) return null;
        setQueryError(typeof e === "string" ? e : String(e));
        return null;
      } finally {
        if (token === requestSeq.current) setReady(true);
      }
    },
    [],
  );

  /**
   * LOG-FR-12: on becoming visible — which for this component is on mount — take
   * the newest page under the current filter rather than replaying anything.
   * Re-runs on every filter change, debounced for the text query alone so a
   * keystroke does not issue a query per character.
   */
  useEffect(() => {
    let cancelled = false;
    const run = () => {
      if (!cancelled) void load(null, "replace");
    };
    const t = setTimeout(run, filter.query ? SEARCH_DEBOUNCE_MS : 0);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
    // Every field is named so a change to any of them re-queries.
  }, [
    load,
    filter.minLevel,
    filter.query,
    filter.queryIsRegex,
    filter.domains.join(","),
  ]);

  /**
   * LOG-FR-11 / LOG-FR-13: follow the append event for as long as this is
   * mounted. The payload carries no records (LGC-FR-12), so every arrival is a
   * prompt to re-query rather than something to render.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onLogRecordsAppended((state: LogBufferState) => {
      if (cancelled) return;
      const current = loadedRef.current;
      if (state.generation !== current.generation) {
        // The buffer this panel was rendering has been cleared: discard the
        // rows, the selection, and the expansion state, and start again.
        setExpanded(new Set());
        setSelected(new Set());
        void load(null, "replace");
        return;
      }
      // LOG-FR-15: ask only for the delta — but keep asking until the panel
      // actually holds the newest record. The backend coalesces a burst of a
      // thousand appends into a couple of events, so a single page-sized delta
      // per event silently leaves the tail unfetched while the Follow pin still
      // claims to be at the end.
      void (async () => {
        for (let guard = 0; guard < 50; guard += 1) {
          const held = loadedRef.current;
          const newest = held.records[held.records.length - 1];
          const page = await load(
            newest ? { after: newest.sequence } : null,
            newest ? "append" : "replace",
          );
          if (!page) return;
          const held2 = loadedRef.current.records;
          const newestHeld = held2.length > 0 ? held2[held2.length - 1].sequence : -1;
          const caughtUp =
            page.highestSequence === null || newestHeld >= page.highestSequence;
          if (caughtUp || page.records.length === 0) return;
        }
      })();
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // No backend event channel (or no Tauri runtime): the panel still
        // renders whatever the initial query returned.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  /**
   * LOG-FR-15: put the anchor row back where it was, now that the prepended
   * rows are committed and laid out.
   *
   * `useLayoutEffect` rather than `useEffect` so the correction lands in the
   * same frame as the insertion — an effect would let the browser paint the
   * jumped position first, which is the flicker the anchoring exists to avoid.
   */
  useLayoutEffect(() => {
    const anchor = pendingAnchor.current;
    if (!anchor) return;
    pendingAnchor.current = null;
    const el = listRef.current;
    if (!el) return;
    const node = el.querySelector<HTMLElement>(`[data-sequence="${anchor.sequence}"]`);
    if (!node) return;
    const moved = node.offsetTop - anchor.offsetTop;
    if (moved !== 0) el.scrollTop += moved;
  }, [loaded.records]);

  // LOG-FR-10: while the pin holds, the newest record stays in view.
  useEffect(() => {
    if (!following) return;
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [loaded.records, following]);

  /**
   * LOG-FR-10: scrolling up releases the pin; returning to the foot resumes it.
   * LOG-FR-15: reaching the top of what is loaded asks for the preceding page.
   */
  const onScroll = useCallback(() => {
    const el = listRef.current;
    if (!el) return;
    const atFoot = el.scrollHeight - el.scrollTop - el.clientHeight <= 4;
    if (atFoot !== followingRef.current) setFollowing(atFoot);
    if (el.scrollTop <= 0) {
      const held = loadedRef.current;
      const oldest = held.records[0];
      // `atOldest` is what stops a buffer whose head has been evicted from
      // re-issuing the same exhausted `before` query on every scroll gesture.
      if (oldest && !held.atOldest) void load({ before: oldest.sequence }, "prepend");
    }
  }, [load]);

  /**
   * LOG-FR-04 / LOG-FR-16. A plain click activates the row — it expands to its
   * JSON and becomes the selection, which is the common case: look at a record,
   * then copy it. A modified click builds a multi-row selection *without*
   * expanding, so selecting fifty rows to copy does not unfold fifty blocks of
   * JSON.
   */
  const onRowClick = useCallback(
    (record: LogRecord, event: React.MouseEvent) => {
      const additive = event.metaKey || event.ctrlKey || event.shiftKey;
      if (additive) {
        setSelected((current) => {
          const next = new Set(current);
          if (next.has(record.sequence)) next.delete(record.sequence);
          else next.add(record.sequence);
          return next;
        });
        return;
      }
      setSelected(new Set([record.sequence]));
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(record.sequence)) next.delete(record.sequence);
        else next.add(record.sequence);
        return next;
      });
    },
    [],
  );

  const selectedRecords = useMemo(
    () => loaded.records.filter((r) => selected.has(r.sequence)),
    [loaded.records, selected],
  );

  /**
   * LOG-FR-16: the selected records as JSON, one complete record per line, in
   * list order — the full records rather than the rendered row text, so what is
   * pasted into a defect report carries every field the columns did not show.
   */
  const copySelected = useCallback(() => {
    if (selectedRecords.length === 0) return;
    const text = selectedRecords.map((r) => JSON.stringify(r)).join("\n");
    void navigator.clipboard?.writeText?.(text)?.catch?.(() => {
      // A clipboard the platform refused is not worth an error surface here.
    });
  }, [selectedRecords]);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
        if (selectedRecords.length === 0) return;
        // A user who has highlighted text inside an expanded record's JSON
        // means the browser's own copy, not the panel's. Overriding it there
        // would make the one place with text worth selecting the one place
        // selection does not work.
        const highlighted = window.getSelection?.()?.toString() ?? "";
        if (highlighted.length > 0) return;
        event.preventDefault();
        copySelected();
      }
    },
    [copySelected, selectedRecords.length],
  );

  /**
   * LOG-FR-17: export the filter's entire match set, not the pages loaded or
   * the rows on screen. Cancelling the dialog invokes nothing.
   */
  const onExport = useCallback(async () => {
    setExportNote(null);
    try {
      const chosen = await browseForSavePath("synthesis-logs.jsonl");
      if (chosen === "cancelled" || !chosen || typeof chosen !== "object") return;
      const written = await exportLogs(filterRef.current, chosen.selected.path);
      setExportNote(`Exported ${written} record${written === 1 ? "" : "s"}.`);
    } catch (e) {
      setExportNote(`Export failed: ${typeof e === "string" ? e : String(e)}`);
    }
  }, []);

  const toggleDomain = useCallback(
    (domain: LogDomain) => {
      const current = filterRef.current.domains;
      setFilter({
        domains: current.includes(domain)
          ? current.filter((d) => d !== domain)
          : [...current, domain],
      });
    },
    [setFilter],
  );

  const filtering = !sameFilter(filter, DEFAULT_FILTER);
  // LOG-FR-18: an empty buffer and a list narrowed to nothing are different
  // states and read differently. `bufferTotal` disregards the filter, which is
  // exactly what tells the two apart.
  // A failed first query leaves `bufferTotal` at zero while saying nothing
  // about the buffer, so the empty-buffer block would claim a session emitted
  // nothing when the truth is that nobody managed to ask.
  const bufferEmpty = ready && !queryError && loaded.bufferTotal === 0;
  const narrowedToNothing =
    ready && loaded.bufferTotal > 0 && loaded.records.length === 0;

  return (
    <div className="logs" onKeyDown={onKeyDown} tabIndex={-1}>
      {/* LOG-FR-05: two control rows, pinned above the list, in this order. */}
      <div className="logs__controls">
        <div className="logs__row">
          <label className="logs__label" htmlFor="logs-level">
            Level
          </label>
          <select
            id="logs-level"
            className="logs__select"
            value={filter.minLevel}
            onChange={(e) => setFilter({ minLevel: e.target.value as LogLevel })}
          >
            {LOG_LEVELS.map((level) => (
              <option key={level} value={level}>
                {level}
              </option>
            ))}
          </select>
          <span className="logs__label">Domain</span>
          {LOG_DOMAINS.map((domain) => (
            <label key={domain} className="logs__check">
              <input
                type="checkbox"
                checked={filter.domains.includes(domain)}
                onChange={() => toggleDomain(domain)}
              />
              {domain}
            </label>
          ))}
          <span className="spacer" />
          <button
            className="btn btn--ghost btn--sm"
            data-active={following}
            aria-pressed={following}
            onClick={() => setFollowing((f) => !f)}
            title="Follow the newest record"
          >
            Follow
          </button>
          <button
            className="btn btn--ghost btn--sm"
            disabled={selectedRecords.length === 0}
            onClick={copySelected}
          >
            Copy
          </button>
          <button className="btn btn--ghost btn--sm" onClick={() => void onExport()}>
            Export
          </button>
        </div>
        <div className="logs__row">
          <input
            className="input input--sm logs__search"
            placeholder="Filter logs…"
            value={filter.query ?? ""}
            onChange={(e) => setFilter({ query: e.target.value })}
          />
          <label className="logs__check" title="Match as a regular expression">
            <input
              type="checkbox"
              checked={filter.queryIsRegex}
              onChange={(e) => setFilter({ queryIsRegex: e.target.checked })}
            />
            .*
          </label>
          {/* LOG-FR-09: reported beside the box, with the last matching list
              left rendered rather than blanked. */}
          {queryError && (
            <span className="logs__error" role="status">
              {queryError}
            </span>
          )}
          <span className="spacer" />
          {exportNote && <span className="t-meta">{exportNote}</span>}
          <span className="t-meta" data-testid="logs-counts">
            {loaded.matchedTotal} of {loaded.bufferTotal}
          </span>
        </div>
      </div>

      <div
        className="logs__list"
        ref={listRef}
        onScroll={onScroll}
        data-testid="logs-list"
      >
        {/* LOG-FR-14: at the head of the list, so it scrolls away with the
            oldest record it describes. */}
        {loaded.droppedTotal > 0 && (
          <div className="logs__dropped">
            {loaded.droppedTotal.toLocaleString()} older records were dropped
          </div>
        )}

        {bufferEmpty && (
          <div className="logs__empty">
            <div className="logs__empty-line">No logs yet</div>
            <p className="logs__empty-body">
              This session has not emitted a diagnostic record. Records appear
              here as the application works.
            </p>
          </div>
        )}

        {narrowedToNothing && (
          <div className="logs__message">
            No records match {filtering ? "these filters" : "this view"}.
          </div>
        )}

        {loaded.records.map((record) => {
          const open = expanded.has(record.sequence);
          return (
            <div
              key={record.sequence}
              className="logs__entry"
              data-sequence={record.sequence}
              data-level={record.level}
              data-selected={selected.has(record.sequence)}
              data-testid="logs-row"
            >
              <div
                className="logs__line"
                onClick={(e) => onRowClick(record, e)}
                role="button"
                aria-expanded={open}
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onRowClick(record, e as unknown as React.MouseEvent);
                  }
                }}
              >
                <span className="logs__ts">{formatLocalTime(record.ts)}</span>
                {/* LOG-FR-03: the printed label, so the level reads without
                    relying on colour. */}
                <span className="logs__lvl" data-level={record.level}>
                  {record.level}
                </span>
                <span className="logs__domains" title={domainLabel(record.domains)}>
                  {domainLabel(record.domains)}
                </span>
                {/* LOG-FR-22: rendered as stored — no case transform anywhere on
                    this path, so a path in a message reads as it does on disk. */}
                <span className="logs__msg">{record.message}</span>
              </div>
              {open && (
                <pre className="logs__json" data-testid="logs-json">
                  {JSON.stringify(record, null, 2)}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
