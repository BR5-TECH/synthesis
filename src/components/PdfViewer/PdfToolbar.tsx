/**
 * The toolbar of the PDF viewer: page navigation, zoom, and text search
 * (PDV-FR-SAIG, PDV-FR-HQTN, PDV-FR-XMRL, PDV-FR-BPXG, PDV-FR-QNVD).
 *
 * Every control is a native button or field, so it is reached with the Tab key
 * and operated with the keyboard, and each one has an accessible name. The
 * viewer offers no control to change, save, print, or export the PDF
 * (PDV-FR-INXM).
 */
import { useEffect, useState, type KeyboardEvent } from "react";
import {
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ChevronUpIcon,
  MinusIcon,
  PlusIcon,
} from "./icons";
import { ZOOM_STEPS, clampPage } from "./zoom";

export interface PdfToolbarProps {
  /** 1-based number of the displayed page. */
  page: number;
  pageCount: number;
  onPage: (page: number) => void;
  zoomIndex: number;
  onZoomIndex: (index: number) => void;
  onResetZoom: () => void;
  search: string;
  onSearch: (text: string) => void;
  /** The count text of the search: `3 of 12`, `No matches`, or empty. */
  countText: string;
  matchCount: number;
  onNextMatch: () => void;
  onPreviousMatch: () => void;
}

export function PdfToolbar({
  page,
  pageCount,
  onPage,
  zoomIndex,
  onZoomIndex,
  onResetZoom,
  search,
  onSearch,
  countText,
  matchCount,
  onNextMatch,
  onPreviousMatch,
}: PdfToolbarProps) {
  const [draft, setDraft] = useState(String(page));
  useEffect(() => setDraft(String(page)), [page]);

  const commitPage = () => {
    const wanted = Number.parseInt(draft, 10);
    if (Number.isNaN(wanted)) {
      setDraft(String(page));
      return;
    }
    // PDV-FR-SAIG: Enter shows that page, clamped to the page range.
    const target = clampPage(wanted, pageCount);
    setDraft(String(target));
    onPage(target);
  };

  const onSearchKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (event.shiftKey) onPreviousMatch();
    else onNextMatch();
  };

  return (
    <div className="pdf-toolbar" role="toolbar" aria-label="PDF viewer">
      <div className="pdf-toolbar__group" role="group" aria-label="Page">
        <button
          type="button"
          className="pdf-btn"
          aria-label="Previous page"
          title="Previous page"
          disabled={page <= 1}
          onClick={() => onPage(page - 1)}
        >
          <ChevronLeftIcon />
        </button>
        <input
          className="pdf-page-field"
          type="text"
          inputMode="numeric"
          aria-label="Page number"
          value={draft}
          size={Math.max(2, String(pageCount).length)}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => setDraft(String(page))}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            commitPage();
          }}
        />
        <span className="pdf-page-count">of {pageCount}</span>
        <button
          type="button"
          className="pdf-btn"
          aria-label="Next page"
          title="Next page"
          disabled={page >= pageCount}
          onClick={() => onPage(page + 1)}
        >
          <ChevronRightIcon />
        </button>
      </div>

      <div className="pdf-toolbar__group" role="group" aria-label="Zoom">
        <button
          type="button"
          className="pdf-btn"
          aria-label="Zoom out"
          title="Zoom out"
          disabled={zoomIndex <= 0}
          onClick={() => onZoomIndex(zoomIndex - 1)}
        >
          <MinusIcon />
        </button>
        <span className="pdf-zoom-label">
          {ZOOM_STEPS[zoomIndex]}%
        </span>
        <button
          type="button"
          className="pdf-btn"
          aria-label="Zoom in"
          title="Zoom in"
          disabled={zoomIndex >= ZOOM_STEPS.length - 1}
          onClick={() => onZoomIndex(zoomIndex + 1)}
        >
          <PlusIcon />
        </button>
        <button
          type="button"
          className="pdf-btn pdf-btn--text"
          aria-label="Reset zoom"
          title="Reset zoom"
          onClick={onResetZoom}
        >
          Reset
        </button>
      </div>

      <div className="pdf-toolbar__group pdf-toolbar__search" role="group" aria-label="Search">
        <input
          className="pdf-search-field"
          type="search"
          aria-label="Search text"
          placeholder="Search…"
          value={search}
          onChange={(event) => onSearch(event.target.value)}
          onKeyDown={onSearchKey}
        />
        <span
          className="pdf-search-count"
          role="status"
          aria-live="polite"
        >
          {countText}
        </span>
        <button
          type="button"
          className="pdf-btn"
          aria-label="Previous match"
          title="Previous match"
          disabled={matchCount === 0}
          onClick={onPreviousMatch}
        >
          <ChevronUpIcon />
        </button>
        <button
          type="button"
          className="pdf-btn"
          aria-label="Next match"
          title="Next match"
          disabled={matchCount === 0}
          onClick={onNextMatch}
        >
          <ChevronDownIcon />
        </button>
      </div>
    </div>
  );
}
