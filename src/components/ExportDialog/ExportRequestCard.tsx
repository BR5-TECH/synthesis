/**
 * The export request card (`EXD-export-dialog.md` EXD-FR-UQUD, EXD-FR-WLUG,
 * EXD-FR-UDHD; `CMT-comments.md` CMT-FR-HFXL).
 *
 * An agent cannot choose where a file goes on the author's machine, so it asks
 * (`RDX-request-document-export-tool.md`). The card shows what was asked and
 * where the request stands, and opens the Export dialog while it is pending.
 */
import { useEffect, useState } from "react";

import { getExportRequest } from "../../api";
import { onExportRequestChanged } from "../../events";
import { logWarn } from "../../logging";
import { openExportDialog } from "../../state/exportDialog";
import { Icon } from "../icons";
import type { ExportRequest, ExportRequestReference } from "../../types";

function fileName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] ?? path;
}

/** EXD-FR-WLUG: what the card says of each state. */
function stateText(request: ExportRequest): string {
  switch (request.state) {
    case "pending":
      return "Waiting for you";
    case "exported":
      return `Exported to ${request.destination ?? "the chosen destination"}`;
    case "cancelled":
      return "Cancelled";
    case "failed":
      return `Failed: ${request.error ?? "the export did not finish"}`;
  }
}

export function ExportRequestCard({ attachment }: { attachment: ExportRequestReference }) {
  const [request, setRequest] = useState<ExportRequest | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [opening, setOpening] = useState(false);

  // EXD-FR-UQUD: the state is read, never assumed from the attachment.
  useEffect(() => {
    let cancelled = false;
    getExportRequest(attachment.requestId).then(
      (read) => {
        if (!cancelled) setRequest(read);
      },
      () => {
        if (cancelled) return;
        logWarn(["frontend"], "export request could not be read", {
          requestId: attachment.requestId,
        });
        setUnavailable(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [attachment.requestId]);

  // EXD-FR-WLUG: the card follows its own request.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void onExportRequestChanged((changed) => {
      if (changed.id === attachment.requestId) {
        setRequest(changed);
        setUnavailable(false);
      }
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
  }, [attachment.requestId]);

  const format = request?.format ?? attachment.format;
  const formatLabel = format ? format.toUpperCase() : "Any format";

  return (
    <div
      className="export-request-card"
      data-testid="export-request-card"
      data-state={request?.state ?? (unavailable ? "unavailable" : "loading")}
    >
      <div className="export-request-card__head">
        <Icon.PullDown size={12} />
        <span className="export-request-card__title">Export request</span>
        <span className="export-request-card__file">{fileName(attachment.path)}</span>
        <span className="export-request-card__format">{formatLabel}</span>
      </div>
      <div className="export-request-card__foot">
        <span className="export-request-card__state">
          {request
            ? stateText(request)
            : unavailable
              ? "This export request is no longer available"
              : "Loading…"}
        </span>
        {request?.state === "pending" && (
          <button
            className="btn btn--primary btn--sm"
            disabled={opening}
            onClick={(e) => {
              // EXD-FR-UDHD: write any pending edit of the file, then open.
              const opener = e.currentTarget;
              setOpening(true);
              void openExportDialog(request.path, {
                format: request.format,
                request,
                opener,
              }).finally(() => setOpening(false));
            }}
          >
            Export…
          </button>
        )}
      </div>
    </div>
  );
}
