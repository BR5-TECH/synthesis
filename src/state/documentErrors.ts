/**
 * Reading the typed errors of the Documents collection
 * (`../../specifications/core/DCL-documents-collection.md` contract surface).
 *
 * A backend refusal reaches the frontend as the text of its typed code, with or
 * without a short explanation after it. The match is made on whole words, so
 * `store_unavailable` is never read as `unavailable`: an underscore is part of a
 * word and no boundary falls inside it.
 */
import { DOCUMENT_ERRORS, type DocumentErrorCode } from "../types";

const CODES = Object.values(DOCUMENT_ERRORS);

function textOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object") {
    const record = error as { code?: unknown; message?: unknown };
    if (typeof record.code === "string") return record.code;
    if (typeof record.message === "string") return record.message;
  }
  return String(error);
}

/** The typed code a refusal carries, or null when it carries none. */
export function documentErrorCode(error: unknown): DocumentErrorCode | null {
  const text = textOf(error);
  for (const code of CODES) {
    if (new RegExp(`(^|[^A-Za-z0-9_])${code}([^A-Za-z0-9_]|$)`).test(text)) {
      return code;
    }
  }
  return null;
}

/**
 * DTV-FR-BEQL / PDV-FR-SLWS: a read that fails as unavailable or unknown shows
 * the unavailable state. Every other failure is a read that could not be made.
 */
export function isUnavailableRefusal(error: unknown): boolean {
  const code = documentErrorCode(error);
  return (
    code === DOCUMENT_ERRORS.unavailable ||
    code === DOCUMENT_ERRORS.unknownDocument
  );
}
