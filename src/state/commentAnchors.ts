/**
 * Anchoring for the Editor's comment rail (`CMT-comments.md`).
 *
 * A thread is attached to a range of the artifact's **Markdown source** plus the
 * exact substring that range covered (CMT-FR-06). Text moves underneath it: the
 * author types above it, another writer rewrites the file, a merge lands. This
 * module is the whole story of keeping a card pointed at the right paragraph
 * through all of that, kept pure so every case is testable without a DOM:
 *
 * - {@link resolveAnchor} re-finds a stored anchor in freshly loaded content by
 *   searching for its quote *nearest its stored offsets* (CMT-FR-18), and reports
 *   a thread it cannot find as orphaned (CMT-FR-19).
 * - {@link shiftAnchor} tracks an anchor through a live edit, moving it when text
 *   is inserted or removed before it and orphaning it when the anchored text is
 *   itself destroyed (CMT-FR-20).
 * - {@link stackCards} turns anchor positions into the rail's card layout, each
 *   card aligned to its anchor and pushed down only as far as it must be to clear
 *   the one above (CMT-FR-27).
 */

import { discussionFragment } from "../types";
import type { FragmentRange, Discussion } from "../types";

/**
 * A thread as the rail holds it: the stored record, plus where it currently sits
 * in the buffer. `anchor` is null for an **orphaned** thread — one whose quote is
 * nowhere in the content (CMT-FR-19). Such a thread is never dropped; it renders
 * in the rail's orphaned section showing the quote it was attached to.
 */
export interface AnchoredThread {
  thread: Discussion;
  /** Where the thread sits now, or null when it is orphaned. */
  anchor: FragmentRange | null;
}

/**
 * CMT-FR-18: find `quote` in `content`, preferring the occurrence nearest
 * `near`.
 *
 * "Nearest" rather than "first" is what makes re-anchoring survive an artifact
 * that repeats a phrase. A thread on the third `## Steps` heading must come back
 * to the third one, not to the first — and after an edit above it, the third one
 * is simply the occurrence closest to where it used to be. Distance is measured
 * from the start offset, and a tie breaks toward the earlier occurrence so the
 * result never depends on scan direction.
 *
 * Returns the found start offset, or -1 when the quote appears nowhere.
 */
export function findNearest(content: string, quote: string, near: number): number {
  if (quote === "") return -1;
  let best = -1;
  let bestDistance = Infinity;
  let from = 0;
  for (;;) {
    const at = content.indexOf(quote, from);
    if (at === -1) break;
    const distance = Math.abs(at - near);
    if (distance < bestDistance) {
      best = at;
      bestDistance = distance;
    }
    // Overlapping occurrences count: a quote of `aa` in `aaa` genuinely has two
    // places it could be, and skipping by the quote's length would hide one.
    from = at + 1;
  }
  return best;
}

/**
 * CMT-FR-18: resolve one stored anchor against loaded content.
 *
 * The stored offsets are a *hint*, not the answer — the file may have been
 * rewritten by someone else since the thread was opened. The quote is the
 * identity; the offsets only decide which occurrence of it is meant. A quote that
 * appears nowhere yields null, which is what makes the thread orphaned rather
 * than silently re-pointed at unrelated text.
 */
export function resolveAnchor(
  content: string,
  stored: FragmentRange,
): FragmentRange | null {
  // Only the range is kept: a fragment target also carries its owner and path.
  // The overwhelmingly common case: nothing moved. Checked first so an unedited
  // artifact costs one slice rather than a scan of the whole document per thread.
  if (content.slice(stored.start, stored.end) === stored.quote) {
    return { start: stored.start, end: stored.end, quote: stored.quote };
  }
  const at = findNearest(content, stored.quote, stored.start);
  if (at === -1) return null;
  return { start: at, end: at + stored.quote.length, quote: stored.quote };
}

/**
 * CMT-FR-18: resolve every thread against the content an artifact just loaded.
 *
 * Threads keep the backend's order (anchor order, CMS-FR-23); the rail decides
 * where each one renders from whether its anchor came back null.
 */
export function resolveThreads(
  content: string,
  threads: readonly Discussion[],
): AnchoredThread[] {
  return threads.map((thread) => {
    // A thread that stores no anchor is a **discussion** (CMS-FR-53): there is
    // nothing to search for and nothing to orphan. It renders in the rail's
    // pinned Discussion section rather than among the aligned cards
    // (CMT-FR-53, CMT-FR-55).
    const stored = discussionFragment(thread);
    return {
      thread,
      anchor: stored === null ? null : resolveAnchor(content, stored),
    };
  });
}

/** A single contiguous edit: `[start, end)` of the old text became `insertedLength`. */
export interface BufferEdit {
  start: number;
  end: number;
  insertedLength: number;
}

/**
 * Derive the edit between two buffers as one contiguous replacement, by trimming
 * the common prefix and suffix.
 *
 * Typing, pasting, and deleting a selection are all exactly this shape, so a
 * single replacement describes real editing faithfully without the Editor having
 * to report its own operations. A Replace All that rewrites twelve scattered
 * occurrences is *not* this shape — the derived span covers everything from the
 * first change to the last — which is why {@link shiftAnchor} treats an anchor
 * overlapping the span as destroyed rather than guessing where it moved.
 *
 * Returns null when the two buffers are identical.
 */
export function diffEdit(before: string, after: string): BufferEdit | null {
  if (before === after) return null;
  let prefix = 0;
  const maxPrefix = Math.min(before.length, after.length);
  while (prefix < maxPrefix && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;
  const maxSuffix = Math.min(before.length - prefix, after.length - prefix);
  while (
    suffix < maxSuffix &&
    before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  return {
    start: prefix,
    end: before.length - suffix,
    insertedLength: after.length - prefix - suffix,
  };
}

/**
 * CMT-FR-20: move an anchor through one edit.
 *
 * Three cases, and the third is the one that matters:
 * - the edit is entirely **after** the anchor — nothing moves;
 * - the edit is entirely **before** it — the anchor slides by the length delta;
 * - the edit **touches** the anchored text — the thread is orphaned (null).
 *
 * The third case is deliberately strict. An edit that overlaps the anchor has
 * destroyed some of what the thread was about, and there is no honest range left
 * to point at; keeping a partially-overwritten anchor would leave the card
 * pointing at text nobody wrote. An insertion exactly *at* the anchor's start
 * counts as before it (the anchor slides, the new text is not part of the quote),
 * and one exactly at its end counts as after.
 */
export function shiftAnchor(
  anchor: FragmentRange,
  edit: BufferEdit,
): FragmentRange | null {
  const delta = edit.insertedLength - (edit.end - edit.start);
  // Entirely after the anchor.
  if (edit.start >= anchor.end) return anchor;
  // Entirely before it — including a pure insertion at its start.
  if (edit.end <= anchor.start) {
    return {
      start: anchor.start + delta,
      end: anchor.end + delta,
      quote: anchor.quote,
    };
  }
  return null;
}

/** CMT-FR-20: move every anchored thread through one edit. */
export function shiftThreads(
  anchored: readonly AnchoredThread[],
  edit: BufferEdit,
): AnchoredThread[] {
  return anchored.map((entry) =>
    entry.anchor === null
      ? entry
      : { thread: entry.thread, anchor: shiftAnchor(entry.anchor, edit) },
  );
}

/**
 * CMT-FR-21: the threads whose live anchor has drifted from the one the backend
 * holds, and so need persisting when the artifact is saved.
 *
 * An orphaned thread is deliberately absent: there is no range to record, and
 * writing one would replace a recoverable anchor (the quote is still stored, and
 * a later edit may bring the text back) with a meaningless one.
 */
export function driftedAnchors(
  anchored: readonly AnchoredThread[],
): { threadId: string; anchor: FragmentRange }[] {
  const out: { threadId: string; anchor: FragmentRange }[] = [];
  for (const entry of anchored) {
    if (entry.anchor === null) continue;
    // CMT-FR-55: a discussion is never reanchored, and asking the backend to
    // would be a typed `not_fragment_targeted` (CMS-FR-59).
    const stored = discussionFragment(entry.thread);
    if (stored === null) continue;
    if (
      stored.start === entry.anchor.start &&
      stored.end === entry.anchor.end &&
      stored.quote === entry.anchor.quote
    ) {
      continue;
    }
    out.push({ threadId: entry.thread.id, anchor: entry.anchor });
  }
  return out;
}

/** A card's placement in the rail: which thread, and its top offset in pixels. */
export interface StackedCard {
  threadId: string;
  top: number;
}

/**
 * CMT-FR-27: place each card at its anchor's line, pushing a card down only as
 * far as it must go to clear the one above.
 *
 * Cards are laid out in the order given (anchor order, CMS-FR-23) and each is
 * placed at `max(its own anchor top, the bottom of the previous card)`. A card is
 * therefore never *above* where its anchor is, never overlaps its neighbour, and
 * a run of closely-anchored threads packs tightly instead of every card after the
 * first being shoved to the bottom of the rail.
 *
 * `heights` is keyed by thread id; a card whose height is not yet measured falls
 * back to `defaultHeight`, so the first render (before any measurement) still
 * produces a non-overlapping layout.
 */
export function stackCards(
  cards: readonly { threadId: string; anchorTop: number }[],
  heights: Readonly<Record<string, number>>,
  defaultHeight: number,
  gap = 8,
): StackedCard[] {
  const out: StackedCard[] = [];
  let floor = 0;
  for (const card of cards) {
    const top = Math.max(card.anchorTop, floor);
    out.push({ threadId: card.threadId, top });
    floor = top + (heights[card.threadId] ?? defaultHeight) + gap;
  }
  return out;
}

/**
 * CMT-FR-29: the artifact's unresolved-thread count, orphaned threads included.
 *
 * Orphaned counts because an orphaned thread is exactly the one most at risk of
 * being forgotten: it has left the aligned column, and if it did not count the
 * control would read zero while unanswered objections sat in the rail.
 */
export function unresolvedCount(anchored: readonly AnchoredThread[]): number {
  return anchored.filter((entry) => !entry.thread.resolved).length;
}
