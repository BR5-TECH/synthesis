/**
 * PDF and DOCX attachments in the composer and on a card
 * (`CVP-conversation-presentation.md` CVP-FR-YBTS, `CMT-comments.md` CMT-FR-HQDV).
 */

/** CVP-FR-YBTS: the media type of a `.docx` file. */
export const DOCX_MEDIA_TYPE =
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/** CVP-FR-YBTS: the files the Attach picker offers, the set the backend accepts. */
export const ATTACH_ACCEPT = "image/*,.pdf,.docx,.txt,.log,.md";

/** CMT-FR-HQDV: whether a media type is a PDF or DOCX document. */
export function isDocumentMediaType(mediaType: string): boolean {
  const base = mediaType.split(";")[0].trim().toLowerCase();
  return base === "application/pdf" || base === DOCX_MEDIA_TYPE;
}

/** CMT-FR-HQDV: a byte size as a short label, for example `12 KB`. */
export function formatByteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The decoded length of base64 data. */
export function base64ByteLength(data: string): number {
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
  return Math.max(0, Math.floor((data.length * 3) / 4) - padding);
}
