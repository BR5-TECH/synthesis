/**
 * The failure of a read of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-RFGG,
 * OCW-FR-PFUQ, OCW-FR-AKVG).
 *
 * An OpenSpec failure renders the failure notice of
 * `../../../specifications/ui/OFN-openspec-failure-notices.md`. This module
 * says the codes that are not OpenSpec failures.
 */
import { toOpenSpecFailure } from "../../state/openspec/failureNotice";
import type { ChangeLocation, OpenSpecFailure } from "../../types";

/**
 * The failure a read rejected with. A typed code inside an object
 * (`{ code: "code: detail" }`) is split as a typed string is.
 */
export function readFailureOf(error: unknown): OpenSpecFailure {
  const failure = toOpenSpecFailure(error);
  const at = failure.code.indexOf(":");
  if (at === -1) return failure;
  return {
    ...failure,
    code: failure.code.slice(0, at).trim(),
    detail: failure.detail ?? (failure.code.slice(at + 1).trim() || null),
  };
}

/** The code a read rejected with. */
export function readErrorCode(error: unknown): string {
  return readFailureOf(error).code;
}

/**
 * OCW-FR-AKVG / OSM-FR-GMKV: the archive folder name of a `change_archived`
 * failure, `{ code: "change_archived", detail }` on the wire, or null for
 * every other failure.
 */
export function archivedAsOfError(error: unknown): string | null {
  const failure = readFailureOf(error);
  if (failure.code !== "change_archived") return null;
  return failure.detail ?? "";
}

/** OCW-FR-RFGG: what a read failure that is not an OpenSpec failure says. */
export function failureStatement(code: string, name: string, location: ChangeLocation): string {
  const where = location.kind === "stream" ? "this work stream" : "the active worktree";
  switch (code) {
    case "change_not_found":
      return `The change “${name}” is not in ${where} any more.`;
    case "unknown_stream":
      return "The change could not be read: its work stream is not there any more.";
    case "invalid_change_name":
      return "The change could not be read: its name is not a valid change name.";
    case "no_open_project":
      return "The change could not be read: no project is open.";
    default:
      return "The change could not be read.";
  }
}
