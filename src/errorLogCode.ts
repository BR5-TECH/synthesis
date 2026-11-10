/**
 * The code of a rejection, for a log record (`LOG-logs.md` LOG-FR-20).
 *
 * A backend error is `{ code, message }`, and its message can hold the text of
 * the author or of the backend. A string error can be `code: text`. Only the
 * code goes into a log record. A rejection that carries no code reads as
 * `unknown`, so no text of the error reaches the log.
 */
export function errorLogCode(error: unknown): string {
  if (error && typeof error === "object" && "code" in error) {
    const code = (error as { code: unknown }).code;
    if (typeof code === "string" && CODE.test(code)) return code;
  }
  const text =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : null;
  if (text !== null) {
    const head = text.split(":")[0].trim();
    if (CODE.test(head)) return head;
  }
  return "unknown";
}

/** A code is one word of lowercase letters, digits, and underscores, such as `not_found`. */
const CODE = /^[a-z][a-z0-9_]*$/;
