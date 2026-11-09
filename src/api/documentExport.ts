/**
 * Document export (`DXP-document-export.md`): the conversion of a DOC artifact
 * and the export requests an agent records.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  CalmPicture,
  ExportFormat,
  ExportOutcome,
  ExportRequest,
  ExportResolution,
  ExportResult,
} from "../types";

/**
 * DXP-FR-IEQA: convert the saved file at the project-relative `path` and write
 * it to `destination`. Progress arrives on `"document export progress"` for
 * `exportId` (DXP-FR-LSYA). `calmPictures` holds the print drawing of each
 * CALM source (DXP-FR-VMPP).
 */
export const exportDocument = (request: {
  path: string;
  format: ExportFormat;
  includeFrontmatter: boolean;
  destination: string;
  exportId: string;
  calmPictures: CalmPicture[];
}) => invoke<ExportResult>("export_document", request);

/** DXP-FR-TQCQ: the CALM sources of the saved file at `path`, in order. */
export const listExportCalmSources = (path: string) =>
  invoke<string[]>("list_export_calm_sources", { path });

/** DXP-FR-NXGQ: one export request. */
export const getExportRequest = (requestId: string) =>
  invoke<ExportRequest>("get_export_request", { requestId });

/** DXP-FR-LHAG: end a pending export request with `outcome`. */
export const resolveExportRequest = (request: {
  requestId: string;
  outcome: ExportOutcome;
  destination?: string;
  error?: string;
}) => invoke<ExportResolution>("resolve_export_request", request);
