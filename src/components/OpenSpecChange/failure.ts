/**
 * The words a failed read of a change renders
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-RFGG,
 * OCW-FR-PFUQ).
 */
import type { ChangeLocation } from "../../types";

/**
 * The code a read rejected with. The backend rejects with one string code, or
 * `code: detail`. An `Error` and an object with a `code` carry the same code.
 */
export function readErrorCode(error: unknown): string {
  let raw: string;
  if (typeof error === "string") raw = error;
  else if (error instanceof Error) raw = error.message;
  else if (error && typeof error === "object" && "code" in error)
    raw = String((error as { code: unknown }).code);
  else raw = String(error);
  if (raw.startsWith("Error: ")) raw = raw.slice(7);
  const at = raw.indexOf(":");
  return (at === -1 ? raw : raw.slice(0, at)).trim();
}

/** OCW-FR-RFGG: the codes that route to the OpenSpec section of Project settings. */
export function routesToSettings(code: string): boolean {
  return code === "openspec_mode_inactive" || code === "cli_unavailable";
}

/** OCW-FR-RFGG: what the read failure says. */
export function failureStatement(code: string, name: string, location: ChangeLocation): string {
  const where = location.kind === "stream" ? "this work stream" : "the active worktree";
  switch (code) {
    case "change_not_found":
      return `The change “${name}” is not in ${where} any more.`;
    case "openspec_mode_inactive":
      return "OpenSpec cannot be read: OpenSpec mode is not active for this project.";
    case "cli_unavailable":
      return "OpenSpec cannot be read: the OpenSpec CLI is not available on this machine.";
    case "unknown_stream":
      return "The change could not be read: its work stream is not there any more.";
    case "cli_timed_out":
      return "The change could not be read: the OpenSpec CLI did not answer in time.";
    case "cli_failed":
      return "The change could not be read: the OpenSpec CLI failed.";
    case "cli_output_unreadable":
      return "The change could not be read: the OpenSpec CLI gave output that cannot be read.";
    case "invalid_change_name":
      return "The change could not be read: its name is not a valid change name.";
    case "no_open_project":
      return "The change could not be read: no project is open.";
    default:
      return "The change could not be read.";
  }
}
