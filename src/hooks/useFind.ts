/**
 * `EFR-editor-find-replace.md`: the find panel's match set over the Editor's active
 * editing surface.
 *
 * The Editor hands in the surfaces that are active right now — the raw-text
 * source alone, or the WYSIWYG frontmatter region followed by the rich body
 * (EFR-FR-DDUX) — and gets back one flat, document-ordered match list with a
 * single current index, which is what the panel's `n/total` counts (EFR-FR-DXTV).
 * Matching runs over the text the Editor supplies, so it always describes the
 * in-memory buffer rather than the bytes on disk.
 *
 * The current index lives here rather than on the edit session: EFR-FR-GIPZ
 * retains the panel's *form, query, replacement and mode*, and the position
 * within the match set is derived from the content, which is re-seeded whenever
 * the artifact is reopened.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import {
  findMatches,
  reindexCurrent,
  stepIndex,
  type FindMatch,
} from "../state/findMatches";
import type { SearchMode } from "../types";

/** Which editing surface a match was found in. */
export type FindSurfaceId = "source" | "frontmatter" | "body";

/** One searchable surface, in the order matches are counted. */
export interface FindTarget {
  id: FindSurfaceId;
  text: string;
}

/** A match, tagged with the surface whose text its offsets index into. */
export interface SurfaceMatch extends FindMatch {
  surface: FindSurfaceId;
}

export interface FindEngine {
  /** Every match across the active surfaces, in document order. */
  matches: SurfaceMatch[];
  /** Index of the current match, or -1 when there is none. */
  current: number;
  /** The current match itself, for convenience. */
  currentMatch: SurfaceMatch | null;
  /** EFR-FR-ENKY: false when a `regex` query does not compile. */
  valid: boolean;
  /** EFR-FR-EGQB: step the current match, wrapping at both ends. */
  step: (delta: 1 | -1) => void;
  /** Move the current match to a specific ordinal (used after a replacement). */
  setCurrent: (index: number) => void;
  /**
   * Place the anchor the next recompute reindexes from (EFR-FR-EVHJ), overriding
   * where the current match sits now. A Replace uses it to land past the text it
   * just inserted, so a replacement that itself contains the query advances
   * instead of matching what it just wrote (EFR-FR-EXHA).
   */
  setAnchor: (surface: FindSurfaceId | null, start: number) => void;
  /** The matches belonging to one surface, in that surface's own offsets. */
  matchesIn: (surface: FindSurfaceId) => FindMatch[];
}

/**
 * Compute the match set for `query`/`mode` across `targets`.
 *
 * `enabled` is false while the panel is closed or the tab is blocked by a modal
 * (EFR-FR-HLGY), which yields an empty set rather than a stale one — nothing is
 * highlighted and the replace actions have nothing to act on.
 */
export function useFind(
  targets: readonly FindTarget[],
  query: string,
  mode: SearchMode,
  enabled: boolean,
  /**
   * Optional gate on whether a match is usable by the surface that holds it.
   *
   * The WYSIWYG body needs it: a match in its flat text projection may not
   * correspond to one contiguous span of the document (it can straddle a block
   * separator or an inline node), and such a match can be neither highlighted
   * nor rewritten. Dropping it here rather than downstream is what keeps the
   * counter, the navigation, the decorations, and Replace describing one and the
   * same set — counting a match nothing else can act on would report `1/3` while
   * highlighting two and replacing two.
   */
  usable?: (match: SurfaceMatch) => boolean,
): FindEngine {
  // Serialised so the memo below is keyed on the surfaces' *content*: the
  // Editor rebuilds the array on every render, so an identity comparison would
  // recompute constantly, and a length comparison would miss every edit.
  const key = useMemo(
    () => JSON.stringify(targets.map((t) => [t.id, t.text])),
    [targets],
  );

  const { matches, valid } = useMemo(() => {
    if (!enabled || query === "") {
      return { matches: [] as SurfaceMatch[], valid: true };
    }
    const all: SurfaceMatch[] = [];
    let ok = true;
    for (const target of targets) {
      const result = findMatches(target.text, query, mode);
      if (!result.valid) ok = false;
      for (const m of result.matches) {
        const match: SurfaceMatch = { ...m, surface: target.id };
        if (!usable || usable(match)) all.push(match);
      }
    }
    return { matches: all, valid: ok };
    // `key` stands in for `targets`; listing both would recompute on identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, query, mode, enabled, usable]);

  const [current, setCurrentIndex] = useState(0);

  /**
   * EFR-FR-EVHJ: where the current match lands after a recompute.
   *
   * `start` is the offset the current match occupied and `ordinal` its position
   * among that surface's matches, alongside how many that surface then had. The
   * offset alone is not enough: an edit *above* the current match shifts every
   * offset below it, and matching on the offset would then walk the current
   * match backwards to whichever occurrence moved into the vacated position.
   */
  const anchorRef = useRef<{
    surface: FindSurfaceId;
    start: number;
    ordinal: number;
    total: number;
  } | null>(null);
  const matchesRef = useRef(matches);
  matchesRef.current = matches;

  useEffect(() => {
    const anchor = anchorRef.current;
    if (matches.length === 0 || !anchor) {
      setCurrentIndex(0);
      return;
    }
    // Reindex within the anchor's own surface, so the current match does not
    // jump between the frontmatter region and the body when only one of them
    // changed, then translate back to the flat ordinal.
    const sameSurface = matches.filter((m) => m.surface === anchor.surface);
    if (sameSurface.length === 0) {
      // The surface the current match lived in has no matches left — its own
      // offsets are meaningless against the ones that remain, which index a
      // different surface's text entirely. The first match is the answer.
      setCurrentIndex(0);
      return;
    }
    const survivor = sameSurface.findIndex((m) => m.start === anchor.start);
    // An unchanged count with the occupied offset gone means the whole set
    // shifted under an edit elsewhere; the same ordinal is still the same
    // occurrence. Otherwise fall back to the nearest following match.
    const local =
      survivor !== -1
        ? survivor
        : sameSurface.length === anchor.total
          ? Math.min(anchor.ordinal, sameSurface.length - 1)
          : reindexCurrent(sameSurface, anchor.start);
    setCurrentIndex(matches.indexOf(sameSurface[local]));
  }, [matches]);

  // Remember where the current match sits, for the next recompute.
  useEffect(() => {
    const m = matches[current];
    if (!m) {
      anchorRef.current = null;
      return;
    }
    const sameSurface = matches.filter((x) => x.surface === m.surface);
    anchorRef.current = {
      surface: m.surface,
      start: m.start,
      ordinal: sameSurface.indexOf(m),
      total: sameSurface.length,
    };
  }, [matches, current]);

  const setCurrent = (index: number) => {
    const total = matchesRef.current.length;
    if (total === 0) return;
    setCurrentIndex(((index % total) + total) % total);
  };

  return {
    matches,
    current: matches.length === 0 ? -1 : Math.min(current, matches.length - 1),
    currentMatch: matches[Math.min(current, matches.length - 1)] ?? null,
    valid,
    step: (delta) => setCurrentIndex(stepIndex(current, matches.length, delta)),
    setCurrent,
    setAnchor: (surface, start) => {
      // A deliberate relocation, so it carries no ordinal to fall back on: the
      // offset is the whole intent (land past what was just written).
      anchorRef.current = surface
        ? { surface, start, ordinal: -1, total: -1 }
        : null;
    },
    matchesIn: (surface) =>
      matches
        .filter((m) => m.surface === surface)
        .map(({ start, end }) => ({ start, end })),
  };
}
