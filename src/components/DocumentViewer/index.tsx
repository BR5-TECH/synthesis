/**
 * The viewer tab for a Markdown or text document (DTV-FR-MICG .. DTV-FR-BPXG).
 *
 * The tab is read-only. It reads the text with `read_document` and nothing
 * else, and it follows the collection through `useDocumentLoad`. The hook keeps
 * the old text on show while a new revision loads. The scroll container stays
 * mounted in that time, so the scroll position and the Rich or Source choice
 * stay as they were (DTV-FR-SAIG).
 */
import { useCallback, useRef, useState } from "react";
import * as api from "../../api";
import { useDocumentLoad } from "../../hooks/useDocumentLoad";
import { logError, logInfo, logWarn } from "../../logging";
import {
  getTextViewState,
  setTextViewState,
} from "../../state/documentViewState";
import type { DocumentViewMode } from "../../state/documentViewState";
import type { DocumentText } from "../../types";
import { DocumentMarkdown } from "./DocumentMarkdown";
import { ViewSwitch } from "./ViewSwitch";

const UNAVAILABLE_CAUSES =
  "The file may have been moved, deleted, or made unreadable, it may not be valid UTF-8 text, or its source may have been removed.";

function isTextFormat(data: DocumentText): boolean {
  return data.format === "markdown" || data.format === "text";
}

function Viewer({ documentId }: { documentId: string }) {
  const [mode, setMode] = useState<DocumentViewMode>(
    () => getTextViewState(documentId).mode ?? "rich",
  );
  const regionRef = useRef<HTMLDivElement | null>(null);

  // Nothing here carries a path, a name, or any part of the text.
  const onLoaded = useCallback((data: DocumentText) => {
    if (!isTextFormat(data)) {
      logError(["frontend"], "document read failed", { reason: "read" });
      return;
    }
    logInfo(["frontend"], "document loaded", {
      format: data.format,
      bytes: new TextEncoder().encode(data.text).length,
    });
  }, []);
  const onFailed = useCallback((_error: unknown, unavailable: boolean) => {
    if (unavailable) {
      logWarn(["frontend"], "document read failed", { reason: "unavailable" });
    } else {
      logError(["frontend"], "document read failed", { reason: "read" });
    }
  }, []);

  const { phase, data } = useDocumentLoad<DocumentText>(
    documentId,
    api.readDocument,
    { onLoaded, onFailed },
  );

  const choose = (next: DocumentViewMode) => {
    setMode(next);
    setTextViewState(documentId, { mode: next });
    if (regionRef.current) regionRef.current.scrollTop = 0;
  };

  let body;
  if (phase === "loading") {
    body = (
      <div className="document-viewer__state" role="status">
        Loading document…
      </div>
    );
  } else if (phase === "unavailable") {
    body = (
      <div className="document-viewer__state" role="status">
        <p className="document-viewer__state-title">
          This document is unavailable.
        </p>
        <p className="document-viewer__state-detail">{UNAVAILABLE_CAUSES}</p>
      </div>
    );
  } else if (phase === "failed" || data === null || !isTextFormat(data)) {
    body = (
      <div className="document-viewer__state" role="alert">
        The document could not be read.
      </div>
    );
  } else {
    const markdown = data.format === "markdown";
    const rich = markdown && mode === "rich";
    body = (
      <>
        {markdown && (
          <div className="document-viewer__bar">
            <ViewSwitch mode={mode} onChange={choose} />
          </div>
        )}
        <div
          ref={regionRef}
          className="document-viewer__text"
          role="region"
          aria-label="Document text"
          tabIndex={0}
        >
          {rich ? (
            <div className="doc document-viewer__rich">
              <DocumentMarkdown text={data.text} />
            </div>
          ) : (
            <pre className="document-viewer__source">{data.text}</pre>
          )}
        </div>
      </>
    );
  }

  return (
    <section className="document-viewer" data-testid="document-text-viewer">
      {body}
    </section>
  );
}

export function DocumentTextViewer({ documentId }: { documentId: string }) {
  // A new document id starts a new viewer with its own state.
  return <Viewer key={documentId} documentId={documentId} />;
}
