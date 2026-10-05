// The Documents collection (`specifications/core/DCL-documents-collection.md`)
//
// The payload shapes carry the snake_case field names the backend sends, because
// the contract spells them that way (DCL-FR-TYOU .. DCL-FR-TEUK).

/** DCL-FR-FGGU: the three formats a document can have. */
export type DocumentFormat = "pdf" | "markdown" | "text";

/** The status of a source or of a document (DCL-FR-EWPO, DCL-FR-QTGN). */
export type DocumentStatus = "available" | "unavailable";

/** DCL-FR-TYOU: a source is a selected file or a selected folder. */
export type DocumentSourceKind = "file" | "folder";

/** DCL-FR-EWPO: why a source is unavailable. */
export type DocumentSourceReason = "missing" | "unreadable" | "link";

/** One selected reference, with the path exactly as the user selected it. */
export interface DocumentSource {
  kind: DocumentSourceKind;
  path: string;
  status: DocumentStatus;
  reason?: DocumentSourceReason;
}

/** One document of the collection (DCL-FR-SQEP). */
export interface DocumentEntry {
  /** `doc-` followed by 32 hexadecimal characters (DCL-FR-QKJO). */
  id: string;
  path: string;
  name: string;
  format: DocumentFormat;
  status: DocumentStatus;
  /** Absent while the document is unavailable. */
  revision?: string;
}

/** What `"list documents"`, `"remove document source (path)"` and the event carry. */
export interface DocumentsSnapshot {
  sources: DocumentSource[];
  documents: DocumentEntry[];
}

/** The mode of `"pick document sources (mode)"` (DCL-FR-VXXI). */
export type PickDocumentSourcesMode = "files" | "folder";

export interface PickDocumentSourcesResult {
  cancelled: boolean;
  ignored_count: number;
  snapshot: DocumentsSnapshot;
}

/** What `"read document (id)"` returns for a Markdown or text document. */
export interface DocumentText {
  id: string;
  name: string;
  format: DocumentFormat;
  text: string;
  revision: string;
}

/** What `"read document pdf (id)"` returns: the whole file, base64-encoded. */
export interface DocumentPdf {
  id: string;
  name: string;
  revision: string;
  bytes_base64: string;
}

/** DCL contract surface: the typed errors the commands can return. */
export const DOCUMENT_ERRORS = {
  noProjectOpen: "no_project_open",
  storeUnavailable: "store_unavailable",
  invalidPath: "invalid_path",
  unknownDocument: "unknown_document",
  unavailable: "unavailable",
  wrongFormat: "wrong_format",
} as const;

export type DocumentErrorCode =
  (typeof DOCUMENT_ERRORS)[keyof typeof DOCUMENT_ERRORS];
