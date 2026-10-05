/**
 * Highlights of the search matches in the text layer of a page (PDV-FR-BPXG).
 *
 * A highlight wraps a slice of the text of a text div in a `mark` element. The
 * text of the layer stays the same, so a copy of the selection gives the page
 * text (PDV-FR-KDVB).
 */
import type { MatchSegment, PageText, PdfMatch } from "./pdfSearch";
import { matchSegments } from "./pdfSearch";

export const MARK_CLASS = "pdf-match";

/** A match on the displayed page, and whether it is the current match. */
export interface PageMatch {
  match: PdfMatch;
  current: boolean;
}

/** Remove every highlight under the container and join the text again. */
export function clearHighlights(container: HTMLElement): void {
  const marks = Array.from(container.querySelectorAll(`mark.${MARK_CLASS}`));
  const parents = new Set<Node>();
  for (const mark of marks) {
    const parent = mark.parentNode;
    if (parent === null) continue;
    parent.replaceChild(
      mark.ownerDocument.createTextNode(mark.textContent ?? ""),
      mark,
    );
    parents.add(parent);
  }
  for (const parent of parents) parent.normalize();
}

interface Slice extends MatchSegment {
  current: boolean;
}

/** Split the text of one div into plain text and marks. */
function markDiv(div: HTMLElement, slices: Slice[]): HTMLElement | null {
  if (div.childNodes.length !== 1) return null;
  const only = div.firstChild;
  if (only === null || only.nodeType !== Node.TEXT_NODE) return null;
  const text = only.textContent ?? "";
  const doc = div.ownerDocument;
  const fragment = doc.createDocumentFragment();
  let first: HTMLElement | null = null;
  let at = 0;
  for (const slice of slices.sort((a, b) => a.from - b.from)) {
    const from = Math.max(slice.from, at);
    const to = Math.min(slice.to, text.length);
    if (to <= from) continue;
    if (from > at) fragment.append(doc.createTextNode(text.slice(at, from)));
    const mark = doc.createElement("mark");
    mark.className = MARK_CLASS;
    if (slice.current) {
      mark.setAttribute("data-current", "true");
      first ??= mark;
    }
    mark.textContent = text.slice(from, to);
    fragment.append(mark);
    at = to;
  }
  if (at === 0) return null;
  if (at < text.length) fragment.append(doc.createTextNode(text.slice(at)));
  div.replaceChildren(fragment);
  return first;
}

/**
 * Highlight the matches of the displayed page. The old highlights go first.
 * Returns the first mark of the current match, or null when it is not here.
 */
export function applyHighlights(
  container: HTMLElement,
  textDivs: HTMLElement[],
  text: PageText,
  matches: PageMatch[],
): HTMLElement | null {
  clearHighlights(container);
  const byItem = new Map<number, Slice[]>();
  for (const { match, current } of matches) {
    for (const segment of matchSegments(text, match)) {
      const list = byItem.get(segment.item) ?? [];
      list.push({ ...segment, current });
      byItem.set(segment.item, list);
    }
  }
  let current: HTMLElement | null = null;
  for (const [item, slices] of byItem) {
    const div = textDivs[item];
    if (!div) continue;
    const mark = markDiv(div, slices);
    if (mark !== null && current === null) current = mark;
  }
  return current;
}
