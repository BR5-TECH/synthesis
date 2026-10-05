/**
 * Which words inside a replaced line actually differ.
 *
 * A whole-line tint answers "this line changed". It does not answer "what
 * changed in it", and on a long line with a one-word edit that is the question
 * the reader actually has. So a line that *replaced* another — both sides
 * present, both marked — is additionally segmented, and only the segments that
 * differ carry the stronger treatment.
 *
 * This applies to a replacement and nothing else. A line with no counterpart is
 * wholly new or wholly gone; every word of it differs, and marking each one
 * would just repeat the line's own marking at higher contrast.
 */
import { alignBy, commonExtent } from "./lineAlign";

// `Intl.Segmenter` is ES2022 and this project's `lib` is ES2020, so its type is
// declared here rather than by widening the whole project's library surface.
declare global {
  namespace Intl {
    interface SegmentData {
      segment: string;
    }
    interface Segments extends Iterable<SegmentData> {}
    class Segmenter {
      constructor(
        locales?: string | string[],
        options?: { granularity?: "grapheme" | "word" | "sentence" },
      );
      segment(input: string): Segments;
    }
  }
}

/** A run of text, and whether it is part of what changed. */
export interface Segment {
  text: string;
  changed: boolean;
}

/** A half-open `[start, end)` slice of a string. */
export type Range = [number, number];

/**
 * The same answer as character offsets rather than as runs.
 *
 * Rich rendering cannot use the runs directly: by the time a block is rendered
 * its text has been split into emphasis, code, and link spans, so the marking
 * has to be applied to whatever part of each span falls inside a changed range.
 */
export function changedRanges(segments: Segment[]): Range[] {
  const ranges: Range[] = [];
  let offset = 0;
  for (const segment of segments) {
    if (segment.changed) ranges.push([offset, offset + segment.text.length]);
    offset += segment.text.length;
  }
  return ranges;
}

/**
 * Split `text` — which begins at `offset` in the string the ranges describe —
 * into runs, each flagged with whether it falls inside a changed range.
 */
export function splitByRanges(
  text: string,
  offset: number,
  ranges: Range[],
): Segment[] {
  const out: Segment[] = [];
  let cursor = 0;
  const push = (slice: string, changed: boolean) => {
    if (!slice) return;
    const last = out[out.length - 1];
    if (last && last.changed === changed) last.text += slice;
    else out.push({ text: slice, changed });
  };

  for (const [start, end] of ranges) {
    const from = Math.max(start - offset, cursor);
    const to = Math.min(end - offset, text.length);
    if (to <= from) continue;
    push(text.slice(cursor, from), false);
    push(text.slice(from, to), true);
    cursor = to;
  }
  push(text.slice(cursor), false);
  return out;
}

/** Both sides of one replacement, segmented against each other. */
export interface WordDiff {
  old: Segment[];
  new: Segment[];
}

/**
 * Words, whitespace runs, and individual punctuation marks.
 *
 * Splitting on whitespace alone would call `getFoo(a)` and `getFoo(b)` wholly
 * different; splitting per character would produce confetti. Word-shaped runs
 * are the unit an author reads a code or prose edit in.
 *
 * `Intl.Segmenter` is what makes that true for text that is not English: it
 * keeps `café` and `👍` whole where a `[A-Za-z0-9_]` class would shred an
 * accented word into three tokens and cut an astral emoji in half — and half a
 * surrogate pair in its own DOM node renders as a replacement glyph, so that
 * one is text corruption rather than a highlighting nicety. It also finds word
 * boundaries in scripts that do not write spaces, so a Japanese edit marks the
 * word that changed rather than the whole line.
 */
const TOKEN = /[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu;

const SEGMENTER =
  typeof Intl !== "undefined" && "Segmenter" in Intl
    ? new Intl.Segmenter(undefined, { granularity: "word" })
    : null;

export function tokenize(line: string): string[] {
  // The fallback is Unicode-aware too; it just cannot find a boundary inside a
  // run of unspaced script.
  if (!SEGMENTER) return line.match(TOKEN) ?? [];
  return Array.from(SEGMENTER.segment(line), (part) => part.segment);
}

/**
 * Below this share of tokens in common, the two lines are not a rewrite of each
 * other but a coincidence of a few shared characters. Segmenting them produces
 * a line speckled with highlights that reads worse than no highlighting at all,
 * so the pair falls back to the whole-line marking it already carries.
 */
const MIN_SIMILARITY = 0.3;

/**
 * The most alignment work one line is worth.
 *
 * This runs once per replaced line, and a side-by-side view renders the whole
 * file — so an unbounded per-line cost becomes an unbounded per-file one, and
 * the tab stops painting. The budget is measured after the shared head and tail
 * are discounted, which is what an ordinary edit inside a long line costs
 * (almost nothing); it bites only on a pair that differs along its whole
 * length, where the segmentation was going to be declined as noise anyway.
 */
const WORD_CELL_BUDGET = 40_000;

/**
 * Segment a replacement. Returns `null` when the two lines have too little in
 * common to be read as one edit, or are too big to compare cheaply — the caller
 * then marks the lines whole, which is what they already carry.
 */
export function diffWords(oldLine: string, newLine: string): WordDiff | null {
  const oldTokens = tokenize(oldLine);
  const newTokens = tokenize(newLine);
  if (oldTokens.length === 0 || newTokens.length === 0) return null;

  const { head, tail } = commonExtent(oldTokens, newTokens);
  const midOld = oldTokens.length - head - tail;
  const midNew = newTokens.length - head - tail;
  if (midOld * midNew > WORD_CELL_BUDGET) return null;

  const pairs = alignBy(oldTokens, newTokens, (token) => token);

  // Whitespace matching is not evidence that two lines are related — every
  // line of an indented file shares its indentation with every other.
  const meaningful = (token: string) => token.trim().length > 0;
  const shared = pairs.filter(
    (pair) => !pair.changed && pair.old != null && meaningful(pair.old),
  ).length;
  // Measured against the *shorter* side: appending a paragraph to a line does
  // not make the line it was appended to any less recognisable, and dividing by
  // the longer side would decline exactly the append the marking is for.
  const total = Math.min(
    oldTokens.filter(meaningful).length,
    newTokens.filter(meaningful).length,
  );
  if (total > 0 && shared / total < MIN_SIMILARITY) return null;

  const oldSegments: Segment[] = [];
  const newSegments: Segment[] = [];
  const push = (into: Segment[], text: string, changed: boolean) => {
    const last = into[into.length - 1];
    if (last && last.changed === changed) last.text += text;
    else into.push({ text, changed });
  };

  // A space between two changed words matches itself, so it would otherwise
  // split one edit into two highlights with a gap down the middle. Absorbing it
  // makes "three four" read as the single replacement it is.
  const absorbed = pairs.map((pair, index) => {
    if (pair.changed) return pair;
    const text = pair.old ?? pair.new ?? "";
    if (text.trim().length > 0) return pair;
    const before = pairs[index - 1];
    const after = pairs[index + 1];
    return before?.changed && after?.changed ? { ...pair, changed: true } : pair;
  });

  for (const pair of absorbed) {
    if (pair.old != null) push(oldSegments, pair.old, pair.changed);
    if (pair.new != null) push(newSegments, pair.new, pair.changed);
  }

  // Every token differing is the "wholly rewritten" case again, reached from
  // the other direction: the segmentation would tint the entire line and say
  // nothing the line's own marking did not.
  if (newSegments.every((segment) => segment.changed)) return null;

  return { old: oldSegments, new: newSegments };
}
