/**
 * EFR-FR-DSKI: highlighting the find panel's matches inside the WYSIWYG surface.
 *
 * The panel matches over *text* (EFR-FR-DDUX) while ProseMirror addresses its
 * document by node position, so this module owns both halves of that bridge: a
 * flat text projection of the document with an index back to positions, and a
 * decoration plugin the Editor pushes ranges into.
 *
 * Kept out of `Editor.tsx` because the mapping is the part most worth testing on
 * its own — an off-by-one here highlights the wrong words and, worse, makes
 * Replace rewrite the wrong span (EFR-FR-EYLX).
 */
// Re-exported by `@tiptap/react`, which is the direct dependency; `@tiptap/core`
// is only a transitive one and importing it directly would rely on hoisting.
import { Extension } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

/** One run of text in the document, and where it starts in document positions. */
export interface TextSegment {
  /** Offset of this run within the flat projection. */
  offset: number;
  /** ProseMirror position of the run's first character. */
  from: number;
  length: number;
}

export interface DocText {
  text: string;
  segments: TextSegment[];
}

/** A resolved match, in ProseMirror document positions. */
export interface PosRange {
  from: number;
  to: number;
}

/**
 * Project the document to the text the panel searches, with an index back to
 * positions.
 *
 * Textblocks are separated by a newline so a query cannot match across a
 * paragraph boundary — without it, a paragraph ending "foo" followed by one
 * starting "bar" would contain "oba". The separator occupies an offset but no
 * document position, which is what makes a match that spans one unresolvable
 * (see `rangeToPositions`) rather than silently mis-mapped.
 */
export function docText(doc: PMNode): DocText {
  const segments: TextSegment[] = [];
  let text = "";
  doc.descendants((node, pos) => {
    if (node.isTextblock) {
      if (text.length > 0) text += "\n";
      return true;
    }
    if (node.isText && node.text) {
      segments.push({ offset: text.length, from: pos, length: node.text.length });
      text += node.text;
    }
    return true;
  });
  return { text, segments };
}

/**
 * The document position of a text offset, or null when it names a separator.
 *
 * Binary search rather than a scan: this runs twice per match on every
 * recompute, and a recompute happens on every keystroke — a linear walk would
 * make the cost `segments × matches` per character typed, which is exactly what
 * the responsiveness the Editor promises on a several-thousand-line document
 * cannot afford. Segments are produced in ascending offset order and do not
 * overlap, which is what makes the search valid.
 */
export function offsetToPos(index: DocText, offset: number): number | null {
  const segments = index.segments;
  let lo = 0;
  let hi = segments.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const s = segments[mid];
    if (offset < s.offset) hi = mid - 1;
    else if (offset >= s.offset + s.length) lo = mid + 1;
    else return s.from + (offset - s.offset);
  }
  return null;
}

/**
 * Resolve a half-open text range to document positions, or null when it cannot
 * be represented as one contiguous span.
 *
 * The length check is what rejects a match crossing a block separator or a
 * non-text node: those make the projection's offsets and the document's
 * positions advance at different rates, so a range whose two ends are both
 * resolvable can still describe a different span than the one that matched.
 * Such a match is dropped rather than highlighted somewhere it does not belong.
 */
export function rangeToPositions(
  index: DocText,
  start: number,
  end: number,
): PosRange | null {
  if (end <= start) return null;
  const from = offsetToPos(index, start);
  const lastChar = offsetToPos(index, end - 1);
  if (from === null || lastChar === null) return null;
  const to = lastChar + 1;
  if (to - from !== end - start) return null;
  return { from, to };
}

/** What the Editor pushes into the plugin: the ranges, and which one is current. */
export interface FindHighlightPayload {
  ranges: PosRange[];
  /** Index into `ranges` of the current match, or -1 when there is none. */
  current: number;
}

export const findHighlightKey = new PluginKey<FindHighlightPayload>(
  "synthesisFindHighlight",
);

export const MATCH_CLASS = "find-match";
export const CURRENT_MATCH_CLASS = "find-match--current";

function buildDecorations(
  doc: PMNode,
  payload: FindHighlightPayload,
): DecorationSet {
  const size = doc.content.size;
  const decos = payload.ranges
    // A range left over from a document that has since changed can point past
    // the end; ProseMirror throws on an out-of-bounds decoration, which would
    // take the whole editing surface down.
    .filter((r) => r.from >= 0 && r.to <= size && r.to > r.from)
    .map((r, i) =>
      Decoration.inline(r.from, r.to, {
        class:
          i === payload.current
            ? `${MATCH_CLASS} ${CURRENT_MATCH_CLASS}`
            : MATCH_CLASS,
      }),
    );
  return DecorationSet.create(doc, decos);
}

const EMPTY: FindHighlightPayload = { ranges: [], current: -1 };

/**
 * EFR-FR-DSKI: decorates the current match set inside the WYSIWYG body.
 *
 * Presentation only — it adds no node and changes no byte, so it occupies no
 * position in the artifact's undo history (EDT-FR-23). The Editor pushes a new
 * payload as a transaction meta whenever the matches or the current index
 * change; between pushes, ranges are mapped through document changes so a
 * keystroke does not leave the highlights visibly lagging before the recompute
 * lands (EFR-FR-EVHJ).
 */
export const FindHighlight = Extension.create({
  name: "findHighlight",

  addProseMirrorPlugins() {
    return [
      new Plugin<FindHighlightPayload>({
        key: findHighlightKey,
        state: {
          init: () => EMPTY,
          apply(tr, value) {
            const pushed = tr.getMeta(findHighlightKey) as
              | FindHighlightPayload
              | undefined;
            if (pushed) return pushed;
            if (!tr.docChanged) return value;
            return {
              ranges: value.ranges.map((r) => ({
                from: tr.mapping.map(r.from),
                to: tr.mapping.map(r.to),
              })),
              current: value.current,
            };
          },
        },
        props: {
          decorations(state) {
            const payload = findHighlightKey.getState(state);
            if (!payload || payload.ranges.length === 0) return null;
            return buildDecorations(state.doc, payload);
          },
        },
      }),
    ];
  },
});
