/**
 * The Documents collection: the reference files a user selects, and the reads
 * that serve their content to the viewers.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 *
 * The frontend names no selected path in a read: a document is read by its id
 * alone, and the picker is opened by the backend (DPN-FR-FDVO, PDV-FR-GPLH).
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  DocumentPdf,
  DocumentsSnapshot,
  DocumentText,
  PickDocumentSourcesMode,
  PickDocumentSourcesResult,
} from "../types";

// --- Documents collection (documents/commands.rs) -------------------------

/** DCL-FR-SQEP: the in-memory snapshot. It does not wait for a refresh. */
export const listDocuments = () => invoke<DocumentsSnapshot>("list_documents");

/**
 * DCL-FR-VXXI: open the OS-native picker for `mode` and add what it returns.
 * The panel passes no path and reads none from the picker (DPN-FR-FDVO).
 */
export const pickDocumentSources = (mode: PickDocumentSourcesMode) =>
  invoke<PickDocumentSourcesResult>("pick_document_sources", { mode });

/** DCL-FR-ATNL: remove one reference. The file or folder itself is untouched. */
export const removeDocumentSource = (path: string) =>
  invoke<DocumentsSnapshot>("remove_document_source", { path });

/** DCL-FR-NCBQ: the current text of a Markdown or text document. */
export const readDocument = (id: string) =>
  invoke<DocumentText>("read_document", { id });

/** DCL-FR-TEUK: the whole content of a PDF document, base64-encoded. */
export const readDocumentPdf = (id: string) =>
  invoke<DocumentPdf>("read_document_pdf", { id });
