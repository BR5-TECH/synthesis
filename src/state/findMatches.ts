/**
 * EFR-FR-CLZF/EFR-FR-DDUX/EFR-FR-ENKY: the Editor's in-tab matching engine.
 *
 * Pure text-in / ranges-out, with no notion of an editing surface: the Editor
 * runs it over whichever surface is active (EFR-FR-DDUX) and over the WYSIWYG
 * frontmatter region separately, then concatenates the results into the one
 * ordinal the panel counts with.
 *
 * The three modes are the universal search input's (`SCH-search.md` SCH-FR-12),
 * and mean exactly what the backend's project-wide search means by them
 * (`../../specifications/core/SCC-search.md` SCC-FR-05), so a query behaves the
 * same whichever box it is typed into. This runs client-side over the in-memory
 * buffer rather than through `"start search"` because that operation walks the
 * project on disk and cannot see an unsaved edit (EFR-FR-DKQT).
 */
import type { SearchMode } from "../types";

/** A half-open `[start, end)` range of a match within the text it was found in. */
export interface FindMatch {
  start: number;
  end: number;
}

/**
 * The outcome of a match run. `valid: false` is reserved for a `regex` query
 * that does not compile (EFR-FR-ENKY) — every other empty outcome is a valid run
 * that simply found nothing, and the panel renders those two states differently.
 */
export type MatchResult =
  | { valid: true; matches: FindMatch[] }
  | { valid: false; matches: [] };

const NO_MATCHES: MatchResult = { valid: true, matches: [] };
const INVALID: MatchResult = { valid: false, matches: [] };

/** Characters that carry meaning to `RegExp` and must not in a literal query. */
const REGEX_META = /[.*+?^${}()|[\]\\]/g;

function escapeLiteral(query: string): string {
  return query.replace(REGEX_META, "\\$&");
}

/**
 * SCC-FR-05: `smart_case` ignores case while the query carries no uppercase
 * character and respects it as soon as one appears. Tested on the query as the
 * user typed it, so pasting `Widget` tightens the search without a mode change.
 */
export function smartCaseIsSensitive(query: string): boolean {
  return query !== query.toLowerCase();
}

/**
 * Compile a query into the global `RegExp` a run scans with, or `null` when the
 * query is a `regex` that does not compile (EFR-FR-ENKY). Literal modes escape the
 * query first, so `a.c` matches the three characters `a.c` and not `abc`
 * (SCC-FR-05).
 */
export function compileQuery(
  query: string,
  mode: SearchMode,
): RegExp | null {
  if (query === "") return null;
  switch (mode) {
    case "regex":
      // No `i` flag: a regular expression means what it says (SCC-FR-05), so a
      // query that wants to ignore case spells it out — `[Ww]idget`, or a
      // character class. JavaScript has no inline `(?i)` construct; such a
      // pattern does not compile and reads as an invalid one (EFR-FR-EPYP).
      //
      // `u` first, because without it `.` and a character class match a single
      // UTF-16 code unit — half of an astral character such as an emoji. A
      // match ending mid-pair would have Replace write a lone surrogate into
      // the buffer, corrupting the document. `u` also rejects a few patterns
      // that are legal without it (a redundant escape like `\-`), so a pattern
      // it refuses is retried unrestricted rather than reported invalid: a
      // query the user could previously run must not stop working.
      try {
        return new RegExp(query, "gu");
      } catch {
        try {
          return new RegExp(query, "g");
        } catch {
          return null;
        }
      }
    case "smart_case":
      return new RegExp(
        escapeLiteral(query),
        smartCaseIsSensitive(query) ? "g" : "gi",
      );
    case "literal_insensitive":
    default:
      return new RegExp(escapeLiteral(query), "gi");
  }
}

/**
 * Every match of `query` in `text`, in document order.
 *
 * An empty query matches nothing — the panel's zero state (EFR-FR-DXTV) — rather
 * than matching everywhere. Zero-length matches are skipped: a pattern like
 * `a*` or `^` "matches" at every position, which would make the counter
 * meaningless and give Replace nothing to rewrite, so they are not offered as
 * matches at all. Skipping them also settles the scan's termination, since a
 * zero-length match never advances `lastIndex` on its own.
 */
export function findMatches(
  text: string,
  query: string,
  mode: SearchMode,
): MatchResult {
  if (query === "") return NO_MATCHES;
  const re = compileQuery(query, mode);
  if (!re) return mode === "regex" ? INVALID : NO_MATCHES;

  const matches: FindMatch[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m[0].length === 0) {
      // Step past the empty match so the scan makes progress; it is not
      // recorded. `lastIndex` past the end ends the loop on the next exec.
      re.lastIndex += 1;
      continue;
    }
    matches.push({ start: m.index, end: m.index + m[0].length });
  }
  return { valid: true, matches };
}

/**
 * EFR-FR-FHFB: rewrite `ranges` in `text` with `replacement`, inserted **literally**
 * — no capture-group reference in it is substituted, in any mode. Ranges must be
 * in ascending order and non-overlapping, which is what `findMatches` returns.
 */
export function applyReplacements(
  text: string,
  ranges: readonly FindMatch[],
  replacement: string,
): string {
  if (ranges.length === 0) return text;
  let out = "";
  let cursor = 0;
  for (const r of ranges) {
    out += text.slice(cursor, r.start) + replacement;
    cursor = r.end;
  }
  return out + text.slice(cursor);
}

/**
 * EFR-FR-EVHJ: where the current match lands after the match set is recomputed.
 *
 * The current match keeps its place when it survives the recomputation — the
 * range is still a match — and otherwise becomes the nearest match after the
 * position it occupied, or the first match when none follows. `anchor` is that
 * position: the start offset the current match had before the change.
 */
export function reindexCurrent(
  matches: readonly FindMatch[],
  anchor: number | null,
): number {
  if (matches.length === 0) return 0;
  if (anchor === null) return 0;
  const survivor = matches.findIndex((m) => m.start === anchor);
  if (survivor !== -1) return survivor;
  const following = matches.findIndex((m) => m.start >= anchor);
  return following === -1 ? 0 : following;
}

/**
 * EFR-FR-DYMA: step the current match, wrapping at both ends — next from the last
 * match becomes the first, previous from the first becomes the last.
 */
export function stepIndex(
  index: number,
  total: number,
  delta: 1 | -1,
): number {
  if (total <= 0) return 0;
  return (((index + delta) % total) + total) % total;
}
