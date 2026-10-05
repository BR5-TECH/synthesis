/**
 * The full results page — a main-viewport tab over the *complete* result set
 * (`SCH-search.md` SCH-FR-08).
 *
 * It dispatches its **own** search over the same query and mode with
 * `scope = "full"` (SCH-FR-18) rather than inheriting the overlay's capped
 * result set, which is what lets the overlay be dismissed while this keeps
 * streaming (SCH-FR-20). Its search is cancelled only when the tab closes — the
 * hook's unmount does that.
 *
 * SCH-FR-11: navigating from a result preserves the tab, so returning to it
 * shows the same query, mode and result set without re-dispatching. That falls
 * out of the tab staying mounted: React keeps the component alive while another
 * tab is active, so the hook is not re-run.
 */
import { useEffect, useState } from "react";
import { SearchResultList } from "./SearchResultList";
import { useSearch } from "../hooks/useSearch";
import { QUERY_MODES } from "../hooks/useSearchQueryMode";
import { SearchSessionStore } from "../state/searchSessions";
import { ARTIFACT_TYPES, type TypeLens } from "../artifactTypes";
import type { SearchHit, SearchMode } from "../types";

export interface SearchResultsTarget {
  query: string;
  mode: SearchMode;
}

interface SearchResultsProps {
  /** The tab's id — the key its finished result set is recorded under. */
  tabId: string;
  target: SearchResultsTarget;
  onActivate: (hit: SearchHit) => void;
  /**
   * SCH-FR-11: the shell-owned store a finished result set outlives this mount
   * in. Required rather than defaulted to a module-level singleton: a store
   * shared process-wide would be keyed by tab id across every project the
   * session ever opens, so a reused tab id would resume another project's
   * results.
   */
  sessions: SearchSessionStore;
}

export function SearchResults({
  tabId,
  target,
  onActivate,
  sessions,
}: SearchResultsProps) {
  /**
   * SCH-FR-11: a finished result set is resumed rather than
   * re-searched. Read once, at mount: a record that appears later belongs to
   * this same mount's own write-through and must not restart anything.
   */
  const [resumed] = useState(() => sessions.get(tabId) ?? null);

  // SCH-FR-18: `full`, and with no typing pause — the query is fixed at open,
  // so there is no typing in progress to wait out. Disabled entirely when a
  // finished result set was resumed, so returning to the tab dispatches nothing.
  const live = useSearch({
    query: target.query,
    mode: target.mode,
    scope: "full",
    enabled: resumed === null,
    debounceMs: 0,
    // SCH-FR-20: this sweep must survive the overlay. At most one search runs at
    // a time (SCC-FR-12), so a keystroke in the overlay supersedes it — and
    // retrying is what makes "survives" true rather than "is silently cut short".
    retryOnSuperseded: true,
  });

  // Record the result set once the sweep terminates **completely**, so the next
  // mount resumes it.
  //
  // Only `completed` and `capped` answer the question that was asked; a
  // `cancelled` or `superseded` sweep stopped early, and recording its partial
  // result set as final would leave the tab permanently showing a truncated
  // answer it would never re-run. Those re-dispatch on the next mount instead.
  // An error is terminal in its own right and is resumed, so the tab does not
  // silently re-run a pattern that cannot compile (SCH-FR-21).
  const finalReason = live.reason === "completed" || live.reason === "capped";
  useEffect(() => {
    if (resumed !== null || !live.ended) return;
    if (!finalReason && live.error === null) return;
    sessions.save(tabId, {
      hits: live.hits,
      reason: live.reason,
      error: live.error,
    });
  }, [
    resumed,
    live.ended,
    live.hits,
    live.reason,
    live.error,
    finalReason,
    sessions,
    tabId,
  ]);

  const hits = resumed ? resumed.hits : live.hits;
  const error = resumed ? resumed.error : live.error;
  const ended = resumed ? true : live.ended;
  const running = resumed ? false : live.running;

  /**
   * SCH-FR-10: the full results page exposes filters over the result set that
   * the overlay does not. The artifact-type lens is the one v1 can answer from
   * what a hit actually carries; owner, author, role, workstream, playbook,
   * source, target, tags, lifecycle status and custom fields describe metadata
   * no v1 surface stores yet, so they are not offered rather than offered inert.
   *
   * It opens showing **everything the search found** — not the Library's "All
   * artifacts" default (LIB-FR-12). The filter narrows a result set the user has
   * already asked for by typing a query; opening it pre-narrowed would silently
   * hide the Files group and every entity match, and SCH-FR-08 has the tab
   * render the same grouping the overlay does.
   */
  const [lens, setLens] = useState<TypeLens>("files");
  const visible = hits.filter((hit) => {
    if (lens === "files") return true;
    if (lens === "artifacts") return hit.group === "artifact";
    return hit.subtype === lens;
  });

  const modeLabel =
    QUERY_MODES.find((m) => m.value === target.mode)?.title ?? target.mode;

  return (
    <div className="search-page">
      <div className="search-page__head">
        <div className="search-page__query">
          <span className="search-page__query-text">{target.query}</span>
          <span className="search-page__mode">{modeLabel}</span>
        </div>
        {/* SCH-FR-10: the filter the overlay deliberately does not offer. */}
        <label className="search-page__filter">
          Type
          <select
            className="select select--sm"
            aria-label="Filter by artifact type"
            value={lens}
            onChange={(e) => setLens(e.target.value as TypeLens)}
          >
            <option value="files">All Results</option>
            <option value="artifacts">Artifacts Only</option>
            {ARTIFACT_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="search-page__body">
        <SearchResultList
          hits={visible}
          running={running}
          ended={ended}
          error={error}
          hasQuery={target.query.trim().length > 0}
          onActivate={onActivate}
          // SCH-FR-10: the page's counts move as its filters narrow the set.
          showCounts
        />
      </div>
    </div>
  );
}
