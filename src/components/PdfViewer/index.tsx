/**
 * The PDF Viewer tab (`specifications/ui/PDV-pdf-viewer.md`).
 *
 * The tab is read-only. It reads the PDF bytes with `read_document_pdf` and
 * nothing else, opens them with PDF.js from memory, and follows the collection
 * through `useDocumentLoad`. The hook keeps the content on show while a new
 * revision loads, so the page, the zoom, and the search stay as they were
 * (PDV-FR-YOQS). The page, the zoom, and the search text belong to the tab and
 * are kept in memory only (PDV-FR-DXUX).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../../api";
import { useDocumentLoad } from "../../hooks/useDocumentLoad";
import { logError, logWarn } from "../../logging";
import { getPdfViewState, setPdfViewState } from "../../state/documentViewState";
import type { DocumentPdf } from "../../types";
import { PdfPage, type PageKey } from "./PdfPage";
import { PdfToolbar } from "./PdfToolbar";
import type { PdfHandle } from "./pdfDocument";
import type { PageMatch } from "./pdfHighlights";
import { countLabel, createPageTextStore } from "./pdfSearch";
import { usePdfDocument } from "./usePdfDocument";
import { usePdfSearch } from "./usePdfSearch";
import { DEFAULT_ZOOM_INDEX, ZOOM_STEPS, clampPage, scaleOf, validZoomIndex } from "./zoom";

const UNAVAILABLE_CAUSES =
  "The file may have been moved, deleted, or made unreadable, or its source may have been removed.";

function Message({ children, alert = false }: { children: string; alert?: boolean }) {
  return (
    <div className="pdf-viewer__state" role={alert ? "alert" : "status"}>
      {children}
    </div>
  );
}

function Ready({ documentId, handle }: { documentId: string; handle: PdfHandle }) {
  const pageCount = handle.doc.numPages;
  const [page, setPage] = useState(() =>
    clampPage(getPdfViewState(documentId).page ?? 1, pageCount),
  );
  const [zoomIndex, setZoomIndex] = useState(() =>
    validZoomIndex(getPdfViewState(documentId).zoomIndex),
  );
  const [search, setSearch] = useState(() => getPdfViewState(documentId).search ?? "");

  // PDV-FR-YOQS: the page stays as it was as far as the new page count allows.
  useEffect(() => {
    setPage((current) => clampPage(current, pageCount));
  }, [pageCount]);

  useEffect(() => {
    setPdfViewState(documentId, { page, zoomIndex, search });
  }, [documentId, page, zoomIndex, search]);

  // The text of each page is read once for the open PDF (PDV non-functional).
  const store = useMemo(
    () =>
      createPageTextStore(handle.doc, () =>
        logWarn(["frontend"], "pdf page text could not be read", { reason: "text" }),
      ),
    [handle],
  );

  const showPage = useCallback(
    (target: number) => setPage(clampPage(target, pageCount)),
    [pageCount],
  );
  const found = usePdfSearch(store, search, page, showPage);

  const pageMatches = useMemo<PageMatch[]>(
    () =>
      found.matches.flatMap((match, index) =>
        match.page === page ? [{ match, current: index === found.current }] : [],
      ),
    [found.matches, found.current, page],
  );

  const onKey = (key: PageKey) => {
    if (key === "next") showPage(page + 1);
    else if (key === "previous") showPage(page - 1);
    else if (key === "first") showPage(1);
    else showPage(pageCount);
  };

  const countText =
    search.trim() === "" || found.status !== "done"
      ? ""
      : countLabel(found.matches.length, found.current);

  return (
    <>
      <PdfToolbar
        page={page}
        pageCount={pageCount}
        onPage={showPage}
        zoomIndex={zoomIndex}
        onZoomIndex={(index) =>
          setZoomIndex(Math.min(Math.max(index, 0), ZOOM_STEPS.length - 1))
        }
        onResetZoom={() => setZoomIndex(DEFAULT_ZOOM_INDEX)}
        search={search}
        onSearch={setSearch}
        countText={countText}
        matchCount={found.matches.length}
        onNextMatch={found.next}
        onPreviousMatch={found.previous}
      />
      <PdfPage
        doc={handle.doc}
        store={store}
        page={page}
        scale={scaleOf(zoomIndex)}
        matches={pageMatches}
        reveal={found.reveal}
        onKey={onKey}
      />
      {/* PDV-FR-BEQL: the current page is announced when it changes. */}
      <p className="sr-only" role="status" aria-live="polite">
        Page {page} of {pageCount}
      </p>
    </>
  );
}

function Viewer({ documentId }: { documentId: string }) {
  // Nothing here carries a path, a name, the search text, or any page text.
  const onFailed = useCallback((_error: unknown, unavailable: boolean) => {
    if (unavailable) {
      logWarn(["frontend"], "pdf read failed", { reason: "unavailable" });
    } else {
      logError(["frontend"], "pdf read failed", { reason: "read" });
    }
  }, []);
  const { phase, data } = useDocumentLoad<DocumentPdf>(
    documentId,
    api.readDocumentPdf,
    { onFailed },
  );
  const open = usePdfDocument(data);

  let body;
  if (phase === "unavailable") {
    body = (
      <div className="pdf-viewer__state" role="status">
        <p className="pdf-viewer__state-title">This document is unavailable.</p>
        <p className="pdf-viewer__state-detail">{UNAVAILABLE_CAUSES}</p>
      </div>
    );
  } else if (phase === "failed" || open.failure === "invalid") {
    body = <Message alert>This PDF could not be displayed.</Message>;
  } else if (open.failure === "password") {
    body = (
      <Message alert>
        This PDF is protected by a password and cannot be displayed.
      </Message>
    );
  } else if (phase === "loading" || open.handle === null) {
    body = <Message>Loading PDF…</Message>;
  } else {
    body = <Ready documentId={documentId} handle={open.handle} />;
  }

  return (
    <section className="pdf-viewer" data-testid="pdf-viewer">
      {body}
    </section>
  );
}

export function PdfViewer({ documentId }: { documentId: string }) {
  // A new document id starts a new viewer with its own state.
  return <Viewer key={documentId} documentId={documentId} />;
}
