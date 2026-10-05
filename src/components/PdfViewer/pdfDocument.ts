/**
 * Loads PDF.js and opens a PDF from bytes in memory (PDV-FR-GPLH, PDV-FR-CEIV).
 *
 * The legacy build of PDF.js runs in older web views. The library loads only
 * when a viewer mounts. The worker is a bundled asset of the application, and
 * the viewer gives PDF.js bytes only: no URL and no path.
 */
import type * as Pdfjs from "pdfjs-dist";

export type PdfjsModule = typeof Pdfjs;
export type PdfDocument = Pdfjs.PDFDocumentProxy;
export type PdfLoadingTask = Pdfjs.PDFDocumentLoadingTask;
export type PdfPageProxy = Pdfjs.PDFPageProxy;

let loading: Promise<PdfjsModule> | null = null;

/** Load PDF.js once and point it at the bundled worker. */
export function loadPdfjs(): Promise<PdfjsModule> {
  if (loading === null) {
    loading = (async () => {
      const [library, worker] = await Promise.all([
        import("pdfjs-dist/legacy/build/pdf.mjs"),
        import("pdfjs-dist/legacy/build/pdf.worker.min.mjs?url"),
      ]);
      library.GlobalWorkerOptions.workerSrc = worker.default;
      return library as PdfjsModule;
    })();
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

/** Decode base64 text to bytes. The loop keeps large files safe. */
export function decodeBase64(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** An open PDF, with the loading task that PDF.js made for it. */
export interface PdfHandle {
  doc: PdfDocument;
  task: PdfLoadingTask;
  revision: string;
  bytes: number;
}

/** Start to open bytes. PDF.js never asks for a password here. */
export function startOpen(
  pdfjs: PdfjsModule,
  data: Uint8Array,
): PdfLoadingTask {
  return pdfjs.getDocument({
    data,
    disableRange: true,
    disableStream: true,
    disableAutoFetch: true,
  });
}

/**
 * Release the PDF.js document. Destroying the loading task destroys the worker
 * side of the document too. Never throws.
 */
export function releaseHandle(handle: PdfHandle): void {
  try {
    void Promise.resolve(handle.task.destroy()).catch(() => {});
  } catch {
    // The task is gone already.
  }
}

/** True for the error that PDF.js raises for a PDF that needs a password. */
export function isPasswordError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    (error as { name?: unknown }).name === "PasswordException"
  );
}
