/**
 * Opens the PDF bytes of a viewer with PDF.js and owns the open document
 * (PDV-FR-GPLH, PDV-FR-YLWC, PDV-FR-YOQS, PDV-FR-INXM, PDV-FR-KJWR).
 *
 * When new bytes arrive, the old document stays on show until the new one is
 * open. The old document is released once the new one has taken its place.
 */
import { useEffect, useState } from "react";
import { logError, logInfo, logWarn } from "../../logging";
import type { DocumentPdf } from "../../types";
import {
  decodeBase64,
  isPasswordError,
  loadPdfjs,
  releaseHandle,
  startOpen,
  type PdfHandle,
  type PdfLoadingTask,
} from "./pdfDocument";

export type PdfOpenFailure = "password" | "invalid";

export interface PdfOpenState {
  handle: PdfHandle | null;
  failure: PdfOpenFailure | null;
}

const NONE: PdfOpenState = { handle: null, failure: null };

export function usePdfDocument(data: DocumentPdf | null): PdfOpenState {
  const [state, setState] = useState<PdfOpenState>(NONE);

  useEffect(() => {
    if (data === null) {
      setState(NONE);
      return;
    }
    let cancelled = false;
    let adopted = false;
    let task: PdfLoadingTask | null = null;
    void (async () => {
      let bytes: Uint8Array;
      try {
        const pdfjs = await loadPdfjs();
        if (cancelled) return;
        bytes = decodeBase64(data.bytes_base64);
        task = startOpen(pdfjs, bytes);
      } catch {
        if (cancelled) return;
        logError(["frontend"], "pdf could not be prepared", { reason: "invalid" });
        setState({ handle: null, failure: "invalid" });
        return;
      }
      const length = bytes.length;
      try {
        const doc = await task.promise;
        if (cancelled) return;
        adopted = true;
        logInfo(["frontend"], "pdf opened", {
          pages: doc.numPages,
          bytes: length,
        });
        setState({
          handle: { doc, task, revision: data.revision, bytes: length },
          failure: null,
        });
      } catch (error) {
        if (cancelled) return;
        if (isPasswordError(error)) {
          logWarn(["frontend"], "pdf needs a password", { reason: "password" });
          setState({ handle: null, failure: "password" });
        } else {
          logError(["frontend"], "pdf could not be opened", {
            reason: "invalid",
          });
          setState({ handle: null, failure: "invalid" });
        }
      }
    })();
    return () => {
      cancelled = true;
      if (!adopted && task !== null) {
        void Promise.resolve(task.destroy()).catch(() => {});
      }
    };
  }, [data]);

  const current = state.handle;
  useEffect(() => {
    if (current === null) return;
    return () => releaseHandle(current);
  }, [current]);

  return state;
}
