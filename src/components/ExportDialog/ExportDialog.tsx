/**
 * The Export dialog (`EXD-export-dialog.md`).
 *
 * The author exports a saved DOC artifact to a `.pdf` or a `.docx` file here.
 * The conversion is the backend's (`DXP-document-export.md`); the dialog holds
 * the choices, starts the export, shows its progress, and — when an agent's
 * export request opened it — ends that request with the outcome (EXD-FR-DHAN).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { browseForSavePath, exportDocument, resolveExportRequest } from "../../api";
import { onDocumentExportProgress } from "../../events";
import { logInfo, logWarn } from "../../logging";
import { showToast } from "../../state/toasts";
import {
  closeExportDialog,
  setExportDialogCloseHandler,
  type ExportDialogRequest,
} from "../../state/exportDialog";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import { Icon } from "../icons";
import type {
  ExportFormat,
  ExportOutcome,
  ExportProgress,
  ExportRequest,
  ExportWarning,
} from "../../types";

/** The file name of a project-relative path. */
export function baseName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] ?? path;
}

/** The file name less its last extension. */
function stem(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/**
 * EXD-FR-LQAW: the destination with the extension of `format` in place of its
 * own. A path with no extension gets one.
 */
export function withExtension(path: string, format: ExportFormat): string {
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const dot = path.lastIndexOf(".");
  const base = dot > slash + 1 ? path.slice(0, dot) : path;
  return `${base}.${format}`;
}

/** EXD-FR-RNJL: what the author reads for each typed error of the export. */
export function exportErrorText(code: string): string {
  switch (code) {
    case "no_project_open":
      return "No project is open.";
    case "not_found":
      return "The file is not in the project any more.";
    case "read_failed":
      return "The file could not be read.";
    case "not_doc_artifact":
      return "The file is not a document of the Documents type, so it can not be exported.";
    case "not_markdown":
      return "The file is not a Markdown file, so it can not be exported.";
    case "outside_project":
      return "The file is not in the project.";
    case "invalid_format":
      return "The format is not .pdf or .docx.";
    case "invalid_destination":
      return "The destination is not valid. Choose it again.";
    case "conversion_failed":
      return "The document could not be converted.";
    case "write_failed":
      return "The file could not be written to the destination.";
    default:
      return code === "" ? "The export failed." : code;
  }
}

/** The typed code of a rejection, for the log (EXD-FR-MRTI). */
function codeOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const KNOWN_CODES = new Set([
  "no_project_open",
  "not_found",
  "read_failed",
  "not_doc_artifact",
  "not_markdown",
  "outside_project",
  "invalid_format",
  "invalid_destination",
  "conversion_failed",
  "write_failed",
]);

/** EXD-FR-QWZO: the progress text of one event. */
export function progressText(progress: ExportProgress | null, format: ExportFormat): string {
  if (progress === null) return "Starting the export…";
  switch (progress.stage) {
    case "reading":
      return "Reading the document…";
    case "diagrams":
      return `Rendering diagrams ${progress.done} of ${progress.total}…`;
    case "converting":
      return `Converting to ${format === "pdf" ? "PDF" : "DOCX"}…`;
    case "writing":
      return "Writing the file…";
  }
}

let exportSeq = 0;
function newExportId(): string {
  exportSeq += 1;
  return `export-${Date.now().toString(36)}-${exportSeq}`;
}

/**
 * EXD-FR-DHAN / EXD-FR-VFFP: end the request with `outcome`, and bring the
 * agents back when the resolution appended a comment.
 */
async function resolveRequest(
  request: ExportRequest,
  outcome: ExportOutcome,
  detail: { destination?: string; error?: string },
): Promise<void> {
  try {
    const resolved = await resolveExportRequest({
      requestId: request.id,
      outcome,
      ...detail,
    });
    logInfo(["frontend"], "export request resolved", {
      requestId: request.id,
      outcome,
      comment: resolved.commentId !== undefined,
    });
    if (resolved.commentFailed) {
      // EXD-FR-VFFP: the request is resolved, and its conversation did not get the outcome.
      logWarn(["frontend"], "export outcome was not posted in the conversation", {
        requestId: request.id,
        outcome,
      });
      showNotToldToast(request.id, "The conversation was not told what happened.");
      return;
    }
    await dispatchDecision(
      { id: request.id, threadId: request.discussionId, target: request.target },
      { commentId: resolved.commentId },
      undefined,
      [],
      "",
    );
  } catch (error) {
    const code = codeOf(error);
    logWarn(["frontend"], "export request could not be resolved", {
      requestId: request.id,
      outcome,
      failure: code === "already_resolved" || code === "request_not_found" ? code : "other",
    });
    showNotToldToast(
      request.id,
      code === "already_resolved"
        ? "This export request is already resolved."
        : "The conversation was not told what happened.",
    );
  }
}

/** EXD-FR-VFFP: the error toast of a resolution that did not reach the conversation. */
function showNotToldToast(requestId: string, body: string): void {
  showToast(
    {
      key: `export-request-${requestId}`,
      level: "Error",
      title: "Could not record the export outcome",
      body,
      address: null,
    },
    { priority: true },
  );
}

/** EXD-FR-YTQG / EXD-FR-OOTQ: the toast of an export that wrote its file. */
function showExportedToast(exportId: string, destination: string, warnings: number): void {
  showToast({
    key: `document-export-${exportId}`,
    level: warnings === 0 ? "Info" : "Warn",
    title: "Document exported",
    body:
      warnings === 0
        ? `${baseName(destination)} was written.`
        : `${baseName(destination)} was written. ${
            warnings === 1 ? "One diagram" : `${warnings} diagrams`
          } stayed as code.`,
    address: null,
  });
}

type Phase = "idle" | "running" | "warnings";

export function ExportDialog({ opening }: { opening: ExportDialogRequest }) {
  const name = baseName(opening.path);
  const [format, setFormat] = useState<ExportFormat>(opening.format);
  const [includeFrontmatter, setIncludeFrontmatter] = useState(false);
  const [destination, setDestination] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<ExportWarning[]>([]);
  const [written, setWritten] = useState<string | null>(null);
  /** EXD-FR-DHAN: the last error, while the last attempt failed. */
  const lastError = useRef<string | null>(null);
  const exportId = useRef<string | null>(null);
  const finished = useRef(false);
  /** EXD-FR-DHAN: what a close by another surface owes the request. */
  const writtenRef = useRef<string | null>(null);
  const runningRef = useRef(false);
  /** EXD-FR-OOTQ: another surface closed the dialog, so a running export reports by toast. */
  const detached = useRef(false);
  const running = phase === "running";

  // EXD-FR-QWZO: the progress of this dialog's own export alone.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void onDocumentExportProgress((event) => {
      if (event.exportId === exportId.current) setProgress(event);
    }).then(
      (fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      },
      () => {},
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /** EXD-FR-DHAN: end a request with its outcome, once. */
  const settle = useCallback(
    (outcome: ExportOutcome, detail: { destination?: string; error?: string }) => {
      if (finished.current) return false;
      finished.current = true;
      if (opening.request) void resolveRequest(opening.request, outcome, detail);
      return true;
    },
    [opening.request],
  );

  /** EXD-FR-DHAN: close the dialog, and end a request with its outcome. */
  const finish = useCallback(
    (outcome: ExportOutcome, detail: { destination?: string; error?: string }) => {
      if (settle(outcome, detail)) closeExportDialog(outcome === "exported" ? "done" : "cancel");
    },
    [settle],
  );

  // EXD-FR-DHAN / EXD-FR-OOTQ: a dialog that another overlay or a project
  // change closes still ends its request. An export that still runs ends it
  // when it settles. The store calls this from its close path, so a cleanup
  // that React runs on a dialog that stays mounted ends nothing.
  useEffect(() => {
    detached.current = false;
    setExportDialogCloseHandler(opening.id, () => {
      detached.current = true;
      if (runningRef.current) return;
      if (writtenRef.current !== null) settle("exported", { destination: writtenRef.current });
      else if (lastError.current !== null) settle("failed", { error: lastError.current });
      else settle("cancelled", {});
    });
    // The handler stays registered after an unmount. A project that closes
    // takes the whole shell down before the window closes the dialog, and the
    // close path still owes the request its outcome. The store forgets the
    // handler when it closes the dialog of `opening.id`.
  }, [opening.id, settle]);

  /** EXD-FR-SVLO: Cancel and Escape, while no export runs. */
  const cancel = useCallback(() => {
    if (running) return;
    if (phase === "warnings" && written !== null) {
      finish("exported", { destination: written });
      return;
    }
    if (lastError.current !== null) finish("failed", { error: lastError.current });
    else finish("cancelled", {});
  }, [finish, phase, running, written]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // EXD-FR-ESQB: Escape does nothing while an export runs.
      e.preventDefault();
      e.stopPropagation();
      cancel();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [cancel]);

  /** EXD-FR-LQAW: the OS-native save dialog. A cancel keeps what was chosen. */
  const choose = async () => {
    try {
      const picked = await browseForSavePath(`${stem(name)}.${format}`);
      if (picked === "cancelled") return;
      setDestination(withExtension(picked.selected.path, format));
    } catch (e) {
      logWarn(["frontend"], "export destination dialog failed", {
        failure: "browse_failed",
      });
      setError(`The save dialog could not open: ${codeOf(e)}`);
    }
  };

  /** EXD-FR-LQAW: a change of format moves the destination's extension too. */
  const changeFormat = (next: ExportFormat) => {
    setFormat(next);
    setDestination((d) => (d === null ? d : withExtension(d, next)));
  };

  /** EXD-FR-ESQB: run the export. */
  const run = async () => {
    if (destination === null || running) return;
    const id = newExportId();
    exportId.current = id;
    setProgress(null);
    setError(null);
    setPhase("running");
    runningRef.current = true;
    const started = Date.now();
    logInfo(["frontend"], "document export started", {
      format,
      includeFrontmatter,
      fromRequest: opening.request !== null,
    });
    try {
      const result = await exportDocument({
        path: opening.path,
        format,
        includeFrontmatter,
        destination,
        exportId: id,
      });
      runningRef.current = false;
      writtenRef.current = result.destination;
      lastError.current = null;
      logInfo(["frontend"], "document export finished", {
        format,
        warnings: result.warnings.length,
        durationMs: Date.now() - started,
      });
      if (detached.current) {
        // EXD-FR-OOTQ / EXD-FR-DHAN: the dialog closed while the export ran.
        showExportedToast(id, result.destination, result.warnings.length);
        settle("exported", { destination: result.destination });
        return;
      }
      if (result.warnings.length === 0) {
        // EXD-FR-YTQG.
        showExportedToast(id, result.destination, 0);
        finish("exported", { destination: result.destination });
        return;
      }
      // EXD-FR-YALT.
      setWarnings(result.warnings);
      setWritten(result.destination);
      setPhase("warnings");
    } catch (e) {
      // EXD-FR-RNJL.
      runningRef.current = false;
      const code = codeOf(e);
      const text = exportErrorText(code);
      lastError.current = text;
      logWarn(["frontend"], "document export failed", {
        format,
        failure: KNOWN_CODES.has(code) ? code : "other",
      });
      if (detached.current) {
        // EXD-FR-OOTQ / EXD-FR-DHAN: the dialog closed while the export ran.
        showToast(
          {
            key: `document-export-${id}`,
            level: "Error",
            title: "Could not export the document",
            body: text,
            address: null,
          },
          { priority: true },
        );
        settle("failed", { error: text });
        return;
      }
      setError(text);
      setPhase("idle");
    }
  };

  const extensionLabel = (f: ExportFormat) => `.${f}`;
  const blockedReason = destination === null ? "Choose a destination first" : undefined;

  return (
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) cancel();
      }}
    >
      <div
        className="modal export-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="export-dialog-title"
        aria-busy={running}
      >
        <div className="modal__head">
          <Icon.Doc size={14} />
          <div className="modal__title" id="export-dialog-title">
            Export document
          </div>
        </div>
        <div className="modal__body export-dialog__body">
          <div className="export-dialog__file" data-testid="export-dialog-file">
            {name}
          </div>

          {phase === "warnings" ? (
            <div className="export-dialog__warnings" role="status">
              <p>{written ? baseName(written) : name} was written.</p>
              <p>
                {warnings.length === 1 ? "One diagram" : `${warnings.length} diagrams`} could
                not be rendered and stayed as code:{" "}
                {warnings.map((w) => `diagram ${w.index}`).join(", ")}.
              </p>
            </div>
          ) : (
            <>
              <fieldset className="export-dialog__formats" disabled={running}>
                <legend className="picker-field__label">Format</legend>
                {(["pdf", "docx"] as const).map((f) => (
                  <label key={f} className="export-dialog__radio">
                    <input
                      type="radio"
                      name="export-format"
                      value={f}
                      checked={format === f}
                      onChange={() => changeFormat(f)}
                    />
                    {extensionLabel(f)}
                  </label>
                ))}
              </fieldset>

              <label className="export-dialog__check">
                <input
                  type="checkbox"
                  checked={includeFrontmatter}
                  disabled={running}
                  onChange={(e) => setIncludeFrontmatter(e.target.checked)}
                />
                Include frontmatter as a metadata table
              </label>

              <div className="picker-field">
                <span className="picker-field__label" id="export-destination-label">
                  Destination
                </span>
                <div className="export-dialog__destination-row">
                  <span
                    className="export-dialog__path"
                    aria-labelledby="export-destination-label"
                    data-testid="export-destination"
                    title={destination ?? undefined}
                  >
                    {destination ?? "No destination chosen"}
                  </span>
                  <button
                    className="btn btn--ghost btn--sm"
                    disabled={running}
                    onClick={() => void choose()}
                  >
                    Choose…
                  </button>
                </div>
              </div>

              {running && (
                <div className="export-dialog__progress" role="status" aria-live="polite">
                  <span className="export-dialog__spinner" aria-hidden="true" />
                  {progressText(progress, format)}
                </div>
              )}
              {error !== null && !running && (
                <div className="export-dialog__error" role="alert">
                  {error}
                </div>
              )}
            </>
          )}
        </div>
        <div className="modal__actions">
          {phase === "warnings" ? (
            <button className="btn btn--primary" onClick={cancel}>
              Close
            </button>
          ) : (
            <>
              <button className="btn btn--ghost" disabled={running} onClick={cancel}>
                Cancel
              </button>
              <button
                className="btn btn--primary"
                disabled={destination === null || running}
                title={blockedReason}
                onClick={() => void run()}
              >
                Export
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
