/**
 * Document export (`DXP-document-export.md`, `EXD-export-dialog.md`).
 *
 * The wire shapes of the export command, its progress event, and the export
 * requests an agent records (`RDX-request-document-export-tool.md`).
 */
import type { DiscussionTarget } from "./comments";

/** DXP-FR-IEQA: the two formats an export writes. */
export type ExportFormat = "pdf" | "docx";

/** DXP-FR-IOFM: a diagram that stayed as code. `index` counts from 1. */
export interface ExportWarning {
  kind: "diagram_failed";
  index: number;
}

/** DXP-FR-VMPP: the print drawing of one CALM source (CAD-FR-QHEJ). */
export interface CalmPicture {
  source: string;
  svg: string;
}

/** DXP contract surface: what `export_document` returns. */
export interface ExportResult {
  destination: string;
  warnings: ExportWarning[];
}

/** DXP-FR-LSYA: the stages of an export, in order. */
export type ExportStage = "reading" | "diagrams" | "converting" | "writing";

/** DXP-FR-LSYA: one progress event of one export. */
export interface ExportProgress {
  exportId: string;
  stage: ExportStage;
  done: number;
  total: number;
}

/** DXP-FR-YEAQ: where an export request stands. */
export type ExportRequestState = "pending" | "exported" | "cancelled" | "failed";

/** DXP-FR-LHAG: how a dialog opened from a card ends the request. */
export type ExportOutcome = "exported" | "cancelled" | "failed";

/** DXP-FR-YEAQ: an export request, as `get_export_request` serves it. */
export interface ExportRequest {
  id: string;
  path: string;
  format?: ExportFormat;
  discussionId: string;
  target: DiscussionTarget;
  state: ExportRequestState;
  destination?: string;
  error?: string;
  createdAt: string;
  resolvedAt?: string;
}

/** DXP-FR-LHAG / DXP-FR-VFFP: what a resolution returns. */
export interface ExportResolution {
  request: ExportRequest;
  commentId?: string;
  /** DXP-FR-UOFT: the request is resolved, and its outcome comment was not appended. */
  commentFailed: boolean;
}

/** CMS-FR-OKDW: the `export_request` attachment of an agent's comment. */
export interface ExportRequestReference {
  kind: "exportRequest";
  requestId: string;
  path: string;
  format?: ExportFormat;
}
