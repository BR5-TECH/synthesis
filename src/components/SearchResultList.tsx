/**
 * The grouped, collapsible result list shared by both presentation levels of
 * `SCH-search.md`: the overlay (SCH-FR-02) and the full results tab (SCH-FR-08).
 *
 * Both render the same grouping (SCH-FR-03), the same one-entry-per-file result
 * shape (SCH-FR-17), the same collapse affordance (SCH-FR-05), and the same
 * in-progress / empty / error states (SCH-FR-19 / SCH-FR-21). The page adds
 * filters of its own (SCH-FR-10); the overlay does not — that is the only
 * difference, so it lives at the call sites rather than here.
 */
import { useState } from "react";
import { groupHits } from "../searchGroups";
import type { SearchHit } from "../types";

interface SearchResultListProps {
  hits: SearchHit[];
  /** A search is dispatched and has not yet ended (SCH-FR-19). */
  running: boolean;
  /** `"search ended"` has arrived for this search (SCH-FR-19). */
  ended: boolean;
  /** SCH-FR-21: an uncompilable query, rendered in place of the groups. */
  error: string | null;
  /** Whether a query has been entered at all. */
  hasQuery: boolean;
  onActivate: (hit: SearchHit) => void;
  /**
   * Render a count beside each group header. The full results page does
   * (SCH-FR-10: "group counts update accordingly" as its filters narrow the
   * set); the overlay deliberately does not — SCH-FR-17 says it shows no match
   * count, because a count over a *capped* sweep would be a number the user
   * could reasonably read as "how many matches exist", which it is not.
   */
  showCounts?: boolean;
}

export function SearchResultList({
  hits,
  running,
  ended,
  error,
  hasQuery,
  onActivate,
  showCounts = false,
}: SearchResultListProps) {
  // SCH-FR-05: each group is collapsible. Collapsing is an interaction *inside*
  // the overlay and never dismisses it (SCH-FR-07 / SCH-FR-05).
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const toggle = (key: string) =>
    setCollapsed((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  // SCH-FR-21: the error replaces the result groups, and the query and active
  // mode are left untouched so the pattern can be corrected in place.
  if (error) {
    return (
      <div className="search-state search-state--error" role="alert">
        {error === "invalid query"
          ? "Invalid regular expression — correct the pattern to search."
          : error}
      </div>
    );
  }

  if (!hasQuery) {
    return (
      <div className="search-state search-state--idle">
        Type to search this project.
      </div>
    );
  }

  const groups = groupHits(hits);

  // SCH-FR-19: an empty state ONLY after the search has ended with no hits.
  // While it is still running and has produced nothing yet, saying "no results"
  // would be a claim the search has not yet earned.
  if (groups.length === 0) {
    return ended ? (
      <div className="search-state search-state--empty">No results.</div>
    ) : (
      <div className="search-state search-state--running" aria-busy="true">
        Searching…
      </div>
    );
  }

  return (
    <div className="search-results" data-running={running}>
      {groups.map((group) => {
        const groupCollapsed = collapsed.has(group.key);
        return (
          <div key={group.key} className="search-group">
            <GroupHeader
              title={group.title}
              collapsed={groupCollapsed}
              onToggle={() => toggle(group.key)}
              count={showCounts ? group.hits.length : undefined}
            />
            {!groupCollapsed &&
              (group.subgroups
                ? group.subgroups.map((sub) => {
                    const subCollapsed = collapsed.has(sub.key);
                    return (
                      <div key={sub.key} className="search-subgroup">
                        <GroupHeader
                          title={sub.title}
                          collapsed={subCollapsed}
                          onToggle={() => toggle(sub.key)}
                          nested
                          count={showCounts ? sub.hits.length : undefined}
                        />
                        {!subCollapsed &&
                          sub.hits.map((hit) => (
                            <ResultRow
                              key={hit.id}
                              hit={hit}
                              onActivate={onActivate}
                            />
                          ))}
                      </div>
                    );
                  })
                : group.hits.map((hit) => (
                    <ResultRow key={hit.id} hit={hit} onActivate={onActivate} />
                  )))}
          </div>
        );
      })}
    </div>
  );
}

function GroupHeader({
  title,
  collapsed,
  onToggle,
  nested = false,
  count,
}: {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
  nested?: boolean;
  count?: number;
}) {
  return (
    <div
      className={nested ? "search-group__head search-group__head--nested" : "search-group__head"}
      role="button"
      tabIndex={0}
      aria-expanded={!collapsed}
      style={{ cursor: "pointer" }}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle();
        }
      }}
    >
      <span aria-hidden="true">{collapsed ? "▸" : "▾"}</span>
      {title}
      {count !== undefined && <span className="search-group__count">{count}</span>}
    </div>
  );
}

/**
 * SCH-FR-17: one entry per matching file — its name, its project-relative path,
 * and, for a content match, the line number and text of its first match. A
 * result that matched only on its path shows no snippet line, and no result
 * carries a match count of its own (a file is one hit however many times the
 * query occurs in it).
 */
function ResultRow({
  hit,
  onActivate,
}: {
  hit: SearchHit;
  onActivate: (hit: SearchHit) => void;
}) {
  return (
    <div className="search-result-entry">
      <div
        className="search-result"
        role="button"
        tabIndex={0}
        onClick={() => onActivate(hit)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onActivate(hit);
          }
        }}
      >
        <span className="search-result__name">{hit.name}</span>
        <span className="search-result__meta">{hit.path}</span>
      </div>
      {hit.matchKind === "content" && hit.line != null && (
        <div className="search-result__snippet">
          <span className="search-result__line">{hit.line}</span>
          <span className="search-result__snippet-text">{hit.snippet}</span>
        </div>
      )}
    </div>
  );
}
