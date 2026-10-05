/**
 * Text of the pages of one open PDF, and the search over it (PDV-FR-XMRL,
 * PDV-FR-BPXG, PDV-FR-TIFB).
 *
 * The text of a page is read from PDF.js once for the open PDF. The text layer
 * of the page and the search both use that one read. This file has no DOM code.
 */
import type { PdfDocument } from "./pdfDocument";

/** The parts of a PDF.js text item that the viewer reads. */
export interface TextItemLike {
  str?: string;
  hasEOL?: boolean;
}

/** The parts of a PDF.js text content that the viewer reads. */
export interface TextContentLike {
  items: TextItemLike[];
  styles?: unknown;
}

/** The text of one page, joined for search. */
export interface PageText {
  /** The text of the items. An item that ends a line adds a space. */
  text: string;
  /** The same text in lower case. It has the same length as `text`. */
  lower: string;
  /** Start offset in `text` of each text item that has a string. */
  starts: number[];
  /** Length of the string of each text item that has a string. */
  lengths: number[];
}

/** What the viewer keeps for one page: the content and its joined text. */
export interface PageEntry {
  content: TextContentLike;
  text: PageText;
}

/** One match. `start` and `end` are offsets in the text of `page`. */
export interface PdfMatch {
  /** 1-based page number. */
  page: number;
  start: number;
  end: number;
}

/** One slice of one text item. `item` indexes the text divs of the layer. */
export interface MatchSegment {
  item: number;
  from: number;
  to: number;
}

/**
 * Lower-case a string without a change of length. A letter whose lower case has
 * another length stays as it is, so offsets stay valid.
 */
export function foldCase(value: string): string {
  const lower = value.toLowerCase();
  if (lower.length === value.length) return lower;
  let out = "";
  for (const unit of value) {
    const folded = unit.toLowerCase();
    out += folded.length === unit.length ? folded : unit;
  }
  return out;
}

export const EMPTY_PAGE_TEXT: PageText = {
  text: "",
  lower: "",
  starts: [],
  lengths: [],
};

/** Join the strings of a text content. */
export function buildPageText(content: TextContentLike): PageText {
  let text = "";
  const starts: number[] = [];
  const lengths: number[] = [];
  for (const item of content.items) {
    if (typeof item.str !== "string") continue;
    starts.push(text.length);
    lengths.push(item.str.length);
    text += item.str;
    if (item.hasEOL) text += " ";
  }
  return { text, lower: foldCase(text), starts, lengths };
}

/** The query that is searched for. Spaces at its ends do not count. */
export function normalizeQuery(query: string): string {
  return foldCase(query.trim());
}

/** Every match of the query on one page. Matches do not overlap. */
export function findPageMatches(
  page: number,
  text: PageText,
  query: string,
): PdfMatch[] {
  const needle = normalizeQuery(query);
  if (needle === "") return [];
  const found: PdfMatch[] = [];
  let at = text.lower.indexOf(needle);
  while (at !== -1) {
    found.push({ page, start: at, end: at + needle.length });
    at = text.lower.indexOf(needle, at + needle.length);
  }
  return found;
}

/** The index of the match to start with: the first one at or after the page. */
export function firstMatchFrom(matches: PdfMatch[], page: number): number {
  const at = matches.findIndex((match) => match.page >= page);
  return at === -1 ? 0 : at;
}

/** Map a match to the slices of the text items that it covers. */
export function matchSegments(text: PageText, match: PdfMatch): MatchSegment[] {
  const out: MatchSegment[] = [];
  for (let item = 0; item < text.starts.length; item += 1) {
    const start = text.starts[item];
    const end = start + text.lengths[item];
    if (start >= match.end) break;
    if (end <= match.start) continue;
    const from = Math.max(match.start, start) - start;
    const to = Math.min(match.end, end) - start;
    if (to > from) out.push({ item, from, to });
  }
  return out;
}

/** The count text of the search: `3 of 12`, or `No matches`. */
export function countLabel(total: number, current: number): string {
  return total === 0 ? "No matches" : `${current + 1} of ${total}`;
}

/** Reads the text of the pages of one PDF, one time for each page. */
export interface PageTextStore {
  readonly pageCount: number;
  /** The content and the joined text of a page. It never rejects. */
  entry(page: number): Promise<PageEntry>;
  /** Called with the reason when a page could not be read. */
  onFailure?: (page: number) => void;
}

export function createPageTextStore(
  doc: PdfDocument,
  onFailure?: (page: number) => void,
): PageTextStore {
  const cache = new Map<number, Promise<PageEntry>>();
  const store: PageTextStore = {
    pageCount: doc.numPages,
    onFailure,
    entry(page) {
      let found = cache.get(page);
      if (found === undefined) {
        found = doc
          .getPage(page)
          .then((proxy) => proxy.getTextContent())
          .then((content) => {
            const like = content as unknown as TextContentLike;
            return { content: like, text: buildPageText(like) };
          })
          .catch(() => {
            store.onFailure?.(page);
            return {
              content: { items: [] },
              text: EMPTY_PAGE_TEXT,
            } as PageEntry;
          });
        cache.set(page, found);
      }
      return found;
    },
  };
  return store;
}
