import type { Editor as TiptapEditor } from "@tiptap/react";
import { findNearest } from "../../state/commentAnchors";

/**
 * Roughly where a DOM selection sits in the rich body's flat text — the hint the
 * source lookup searches near (CMT-FR-06).
 *
 * Derived from the editor's own selection rather than the DOM range, because
 * ProseMirror positions are what the flat projection is indexed by. Falls back to
 * 0 when there is no editor or the selection cannot be located, which degrades
 * the search to "first occurrence" rather than failing it.
 */
export function bodyOffsetOfSelection(
  editor: TiptapEditor | null,
  _selection: Selection | null,
): number {
  if (!editor || editor.isDestroyed) return 0;
  const { from } = editor.state.selection;
  // The flat projection joins textblocks with a newline, so a position's offset
  // in it is the text before it — which `textBetween` over the same separator
  // reproduces exactly.
  try {
    return editor.state.doc.textBetween(0, from, "\n", "\n").length;
  } catch {
    return 0;
  }
}

/**
 * CMT-FR-06: the range of the Markdown **source** that a rich-body selection
 * covers, or null when it covers none.
 *
 * A selection inside a single block appears in the source verbatim, so it is
 * found directly. One that spans blocks does not: the rendered text carries
 * neither the `##` of a heading nor the `- ` of a list item, and browsers join
 * blocks with whitespace the source spells differently. Such a selection is
 * located by its two ends instead — its first rendered line and its last — and
 * the range between them is read back out of the source with the syntax
 * included. Refusing these outright would turn away most of the selections
 * anyone actually makes in a structured document.
 */
export function sourceRange(
  source: string,
  text: string,
  hint: number,
): { start: number; end: number } | null {
  const direct = findNearest(source, text, hint);
  if (direct !== -1) return { start: direct, end: direct + text.length };

  // Blank entries are the block separators themselves, which is exactly the
  // part that differs between the two representations.
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
  if (lines.length === 0) return null;

  const start = findNearest(source, lines[0], hint);
  if (start === -1) return null;
  const last = lines[lines.length - 1];
  // Searched forward from the start rather than nearest to it: the end of a
  // selection is after its beginning, and a phrase repeated inside the passage
  // must not pull the range shut.
  const tail = source.indexOf(last, start);
  // Its last line lying nowhere after its first means this is not one passage
  // of the source at all. Anchoring to the first line alone would silently give
  // the author a thread on a smaller, different passage than the one they
  // selected, which is the outcome CMT-FR-06 refuses.
  if (tail === -1) return null;
  return { start, end: tail + last.length };
}

/** The passage a thread is about to be opened over (CMT-FR-05). */
export interface CommentSelection {
  start: number;
  end: number;
  quote: string;
}

/** That passage plus where its affordance sits on the editing surface. */
export type PendingSelection = CommentSelection & { top: number; left: number };

/**
 * Roughly how wide the affordance is, used only to keep it from overhanging the
 * right edge of a narrow surface. Being a few pixels out just shifts the button
 * slightly; nothing depends on it being exact.
 */
export const AFFORDANCE_WIDTH = 28;
/** The gap between the end of the selection and the affordance below it. */
const AFFORDANCE_GAP = 6;

/**
 * CMT-FR-05: where the affordance goes — immediately past the end of the
 * selection rather than at a fixed corner of the tab, so the control that acts
 * on a passage sits with that passage.
 *
 * Returned in the scroll container's own content coordinates (scroll offset
 * folded in), which is the space an absolutely positioned child of it lives in.
 * That way the button travels with the text as the body scrolls instead of
 * hovering over unrelated prose.
 */
export function selectionPoint(
  sel: Selection | null,
  container: HTMLElement | null,
): { top: number; left: number } {
  const zero = { top: 0, left: 0 };
  if (!sel || !container || sel.rangeCount === 0) return zero;
  const range = sel.getRangeAt(sel.rangeCount - 1);
  // The *last* client rect, not the bounding box: a selection spanning several
  // lines should put the button at the end of the passage, not floating beside
  // the widest line of it.
  const rects = range.getClientRects?.();
  const rect =
    rects && rects.length > 0
      ? rects[rects.length - 1]
      : range.getBoundingClientRect?.();
  return pointForRect(rect ?? null, container);
}

/**
 * CMT-FR-05 on the source surface: where the affordance goes for a selection in
 * a `<textarea>`.
 *
 * A textarea's selection is offsets rather than a DOM range — `getSelection()`
 * reaches inside a replaced element nowhere — so the point is measured on the
 * layer behind it instead (ESH-FR-YTNN), which reproduces the same characters at
 * the same metrics in the same box. A collapsed range at the same offset there
 * therefore lands where the caret is, without the textarea being asked anything
 * it cannot answer.
 */
export function offsetPoint(
  layer: HTMLElement | null,
  offset: number,
  container: HTMLElement | null,
): { top: number; left: number } {
  const zero = { top: 0, left: 0 };
  if (!layer || !container) return zero;
  const doc = layer.ownerDocument;
  const walker = doc.createTreeWalker(layer, NodeFilter.SHOW_TEXT);
  let remaining = offset;
  let node = walker.nextNode();
  while (node) {
    const length = node.textContent?.length ?? 0;
    if (remaining <= length) {
      const range = doc.createRange();
      range.setStart(node, remaining);
      range.collapse(true);
      return pointForRect(range.getBoundingClientRect?.() ?? null, container);
    }
    remaining -= length;
    node = walker.nextNode();
  }
  return zero;
}

/** The shared arithmetic: a client rect in the container's content coordinates. */
function pointForRect(
  rect: DOMRect | null,
  container: HTMLElement,
): { top: number; left: number } {
  const zero = { top: 0, left: 0 };
  if (!rect) return zero;
  const box = container.getBoundingClientRect();
  // Only clamp against a width that has actually been laid out; a container
  // reporting zero has no known right edge to overhang.
  const maxLeft =
    container.clientWidth > 0
      ? Math.max(0, container.clientWidth - AFFORDANCE_WIDTH)
      : Number.POSITIVE_INFINITY;
  return {
    top: Math.max(0, rect.bottom - box.top + container.scrollTop + AFFORDANCE_GAP),
    left: Math.min(
      Math.max(0, rect.left - box.left + container.scrollLeft),
      maxLeft,
    ),
  };
}

