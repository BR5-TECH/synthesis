/**
 * What a viewer tab keeps while it is open: the Rich or Source choice of a
 * Document tab (DTV-FR-XMRL), and the page, zoom, and search text of a PDF
 * Viewer tab (PDV-FR-DXUX).
 *
 * The viewport mounts the active tab alone, so a viewer unmounts whenever the
 * author activates another tab. The state therefore lives here, outside the
 * component, keyed by document id — and the shell drops it when the tab leaves
 * the strip, so a new tab for the same document starts from the defaults. It is
 * memory only: nothing here is written to disk.
 */

export type DocumentViewMode = "rich" | "source";

export interface DocumentTextViewState {
  mode?: DocumentViewMode;
}

export interface PdfViewState {
  /** 1-based page number. */
  page?: number;
  /** An index into the zoom steps of the viewer. */
  zoomIndex?: number;
  search?: string;
}

const textStates = new Map<string, DocumentTextViewState>();
const pdfStates = new Map<string, PdfViewState>();

export function getTextViewState(documentId: string): DocumentTextViewState {
  return textStates.get(documentId) ?? {};
}

export function setTextViewState(
  documentId: string,
  patch: DocumentTextViewState,
): void {
  textStates.set(documentId, { ...getTextViewState(documentId), ...patch });
}

export function getPdfViewState(documentId: string): PdfViewState {
  return pdfStates.get(documentId) ?? {};
}

export function setPdfViewState(
  documentId: string,
  patch: PdfViewState,
): void {
  pdfStates.set(documentId, { ...getPdfViewState(documentId), ...patch });
}

/**
 * Forget the state of every document that has no tab left. Called by the shell
 * with the document ids its strip still holds.
 */
export function pruneViewState(openDocumentIds: ReadonlySet<string>): void {
  for (const id of [...textStates.keys()]) {
    if (!openDocumentIds.has(id)) textStates.delete(id);
  }
  for (const id of [...pdfStates.keys()]) {
    if (!openDocumentIds.has(id)) pdfStates.delete(id);
  }
}

/** Forget everything. Tests only. */
export function resetViewStateForTest(): void {
  textStates.clear();
  pdfStates.clear();
}
