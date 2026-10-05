/**
 * The text search of one open PDF (PDV-FR-XMRL, PDV-FR-BPXG, PDV-FR-TIFB,
 * PDV-FR-YOQS).
 *
 * The hook reads the text of every page through the page text store, which
 * reads each page one time. Then it finds the matches and keeps the current
 * one. A new query, and a new document, start the search again. The search
 * starts at the first match at or after the displayed page.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  findPageMatches,
  firstMatchFrom,
  normalizeQuery,
  type PageTextStore,
  type PdfMatch,
} from "./pdfSearch";

export type SearchStatus = "idle" | "searching" | "done";

export interface PdfSearch {
  status: SearchStatus;
  matches: PdfMatch[];
  /** Index of the current match in `matches`. */
  current: number;
  /** Changes each time the viewer must scroll the current match into view. */
  reveal: number;
  next: () => void;
  previous: () => void;
}

interface SearchResult {
  status: SearchStatus;
  matches: PdfMatch[];
  current: number;
  reveal: number;
}

const IDLE: SearchResult = {
  status: "idle",
  matches: [],
  current: 0,
  reveal: 0,
};

export function usePdfSearch(
  store: PageTextStore,
  query: string,
  page: number,
  showPage: (page: number) => void,
): PdfSearch {
  const [result, setResult] = useState<SearchResult>(IDLE);
  const pageRef = useRef(page);
  pageRef.current = page;
  const showRef = useRef(showPage);
  showRef.current = showPage;
  const resultRef = useRef(result);
  resultRef.current = result;
  const needle = normalizeQuery(query);

  useEffect(() => {
    if (needle === "") {
      setResult((old) => ({ ...IDLE, reveal: old.reveal }));
      return;
    }
    let cancelled = false;
    setResult((old) => ({ ...IDLE, status: "searching", reveal: old.reveal }));
    void (async () => {
      const found: PdfMatch[] = [];
      for (let p = 1; p <= store.pageCount; p += 1) {
        const entry = await store.entry(p);
        if (cancelled) return;
        found.push(...findPageMatches(p, entry.text, needle));
      }
      const start = firstMatchFrom(found, pageRef.current);
      setResult((old) => ({
        status: "done",
        matches: found,
        current: start,
        reveal: old.reveal + 1,
      }));
      if (found.length > 0 && found[start].page !== pageRef.current) {
        showRef.current(found[start].page);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [store, needle]);

  const move = useCallback((step: number) => {
    const now = resultRef.current;
    const total = now.matches.length;
    if (total === 0) return;
    const at = (now.current + step + total) % total;
    setResult({ ...now, current: at, reveal: now.reveal + 1 });
    if (now.matches[at].page !== pageRef.current) {
      showRef.current(now.matches[at].page);
    }
  }, []);

  const next = useCallback(() => move(1), [move]);
  const previous = useCallback(() => move(-1), [move]);

  return { ...result, next, previous };
}
