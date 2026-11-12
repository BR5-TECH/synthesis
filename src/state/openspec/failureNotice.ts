/**
 * The one table of OpenSpec failure notices
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md`, the notice
 * table; the class table of
 * `../../../specifications/core/OFP-openspec-failure-policy.md`).
 *
 * Every surface that states an OpenSpec failure takes its class, its
 * sentence, and its actions from here, so one condition has one sentence and
 * one route wherever the author meets it. Pure functions of the failure.
 */
import { logWarn } from "../../logging";
import type { CliDiagnostic, FailureClass, OpenSpecFailure } from "../../types";

/** OFN-FR-PVYE: the codes of the class `environment`. */
const ENVIRONMENT: ReadonlySet<string> = new Set([
  "openspec_mode_inactive",
  "cli_unavailable",
  "cli_output_unreadable",
  "openspec_image_cli_unavailable",
]);

/** OFN-FR-PVYE: the codes of the class `transient`. */
const TRANSIENT: ReadonlySet<string> = new Set(["cli_timed_out", "cli_failed"]);

/** OFN-FR-PVYE: the codes of the class `content` that are not archive codes. */
const CONTENT: ReadonlySet<string> = new Set([
  "cli_rejected",
  "change_not_ready",
  "tasks_incomplete",
]);

/** OAR-FR-EUXZ: the wrapping refusal whose cause is a CLI `status` code of an archive. */
const ARCHIVE_FAILED = "archive_failed";

/** The two codes whose `detail` is a CLI state (OFP-FR-EOAD). */
const CLI_STATE_CODES: ReadonlySet<string> = new Set([
  "cli_unavailable",
  "openspec_image_cli_unavailable",
]);

/** The CLI states a `detail` can name. */
const CLI_STATES: ReadonlySet<string> = new Set(["not_found", "unsupported", "failed"]);

/** OFN-FR-RNWG / OFP-FR-BYYT: the classifying code, the `cause` where present. */
export function classifyingCode(failure: OpenSpecFailure): string {
  return failure.cause ? failure.cause : failure.code;
}

/**
 * OFP class table: whether a classifying code is a CLI `status` code of an
 * archive. That is a code that starts with `archive_` and is not the
 * wrapping refusal `archive_failed`. A wrapping refusal with no cause, or
 * with a cause that is not an OpenSpec code (such as `commit_failed`), is
 * `none` (OFP-FR-FKRV).
 */
function isArchiveCode(code: string): boolean {
  return code.startsWith("archive_") && code !== ARCHIVE_FAILED;
}

/** OFN-FR-PVYE / OFP-FR-PHOW: the class of a failure, `none` for a code no table names. */
export function failureClass(failure: OpenSpecFailure): FailureClass {
  const code = classifyingCode(failure);
  if (ENVIRONMENT.has(code)) return "environment";
  if (TRANSIENT.has(code)) return "transient";
  if (CONTENT.has(code)) return "content";
  if (isArchiveCode(code)) return "content";
  return "none";
}

/** Whether the failure gets a notice (OFN-FR-PVYE). */
export function hasNotice(failure: OpenSpecFailure): boolean {
  return failureClass(failure) !== "none";
}

/** The CLI state of a failure whose code carries one, or null. */
export function cliStateOf(failure: OpenSpecFailure): string | null {
  if (!CLI_STATE_CODES.has(classifyingCode(failure))) return null;
  const state = (failure.detail ?? "").trim();
  return CLI_STATES.has(state) ? state : null;
}

function quoted(change: string): string {
  return `"${change}"`;
}

/**
 * OFN notice table: the sentence of the classifying code, or null for a code
 * that gets no notice. `fallbackChange` names the change where the failure
 * does not.
 */
export function noticeSentence(
  failure: OpenSpecFailure,
  fallbackChange?: string | null,
): string | null {
  const code = classifyingCode(failure);
  // A change with no name reads as "this change", with no quotes.
  const change = failure.change || fallbackChange;
  const name = change ? quoted(change) : "this change";
  const state = cliStateOf(failure);
  switch (code) {
    case "openspec_mode_inactive":
      return "OpenSpec mode is off for this project.";
    case "cli_unavailable":
      if (state === "unsupported") {
        return "The OpenSpec CLI on this machine is too old. OpenSpec mode needs version 1.0 or later.";
      }
      if (state === "failed") {
        return "The OpenSpec CLI on this machine does not start or does not report its version.";
      }
      return "The OpenSpec CLI is not installed on this machine.";
    case "cli_output_unreadable":
      return "The OpenSpec CLI gave an answer that the application cannot read. Check the version of the OpenSpec CLI.";
    case "openspec_image_cli_unavailable":
      if (state === "unsupported") {
        return "The OpenSpec CLI in the agent image of this project is too old. It must be version 1.0 or later.";
      }
      if (state === "failed") {
        return "The OpenSpec CLI in the agent image of this project does not start. The image needs Node 22.12.0 or later.";
      }
      return "The agent image of this project does not hold the OpenSpec CLI.";
    case "cli_timed_out":
      return "The OpenSpec CLI did not finish in time, also after one more try.";
    case "cli_failed":
      return "The OpenSpec CLI stopped before it answered, also after one more try.";
    case "cli_rejected":
      return "OpenSpec reported a problem in the OpenSpec files of this change.";
    case "change_not_ready": {
      const artifacts = (failure.detail ?? "")
        .split(",")
        .map((a) => a.trim())
        .filter(Boolean)
        .join(", ");
      return `The change misses these artifacts: ${artifacts || "unknown"}. Complete them before Apply.`;
    }
    case "tasks_incomplete":
    case "archive_tasks_incomplete":
      return `The change ${name} has tasks that are not done.`;
    case "archive_spec_update_failed":
      return `The deltas of ${name} do not apply to the live specifications.`;
    case "archive_spec_validation_failed":
      return `The live specifications are not valid after the deltas of ${name}.`;
    case "archive_validation_failed":
      return `The change ${name} is not valid.`;
    case "archive_target_exists":
      return `An archive folder for ${name} with the date of today exists already.`;
    default:
      return isArchiveCode(code) ? `OpenSpec could not archive ${name}.` : null;
  }
}

/** The refusals of an image preflight (per `GSU-graduation-start.md` GSU-FR-GLVQ). */
const IMAGE_REFUSALS: ReadonlySet<string> = new Set([
  "vendor_image_unconfigured",
  "vendor_image_invalid",
  "vendor_execution_unsupported",
  "docker_backend_unverified",
  "docker_socket_unavailable",
]);

/**
 * OFN-FR-RCWY / OFN-FR-LRCZ: the sentence that a merge notice adds about its
 * repair run, or null where the failure names none.
 */
export function repairSentence(failure: OpenSpecFailure): string | null {
  if (failure.repairRunId) return "A rework run started to repair the change.";
  const refusal = failure.repairRefusal;
  if (!refusal) return null;
  if (refusal === "proposal_pending") {
    return "No repair run started: the change holds a pending edit proposal.";
  }
  if (refusal === "openspec_run_active") {
    return "No repair run started: a run of the change is active.";
  }
  if (IMAGE_REFUSALS.has(refusal)) {
    return "No repair run started: the agent image is not ready.";
  }
  return `No repair run started (${refusal}).`;
}

/** OFN-FR-QZEC: the short form of a notice: its sentence alone, or null for no notice. */
export function shortForm(failure: OpenSpecFailure, fallbackChange?: string | null): string | null {
  return noticeSentence(failure, fallbackChange);
}

/** The settings route of an `environment` notice (OFN-FR-EPIG). */
export type SettingsAction = "openspec" | "docker";

/** OFN-FR-EPIG: which settings section an `environment` notice opens, or null. */
export function settingsActionOf(failure: OpenSpecFailure): SettingsAction | null {
  if (failureClass(failure) !== "environment") return null;
  return classifyingCode(failure) === "openspec_image_cli_unavailable" ? "docker" : "openspec";
}

/** OFN-FR-UNEF: each control character renders as a space. */
export function plainText(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/[\u0000-\u001f\u007f-\u009f]/g, " ");
}

/**
 * OFN-FR-IHSQ: the lines of one diagnostic: the severity, the code, and the
 * path, each where present, then the message; and the fix on the next line.
 */
export function diagnosticLines(diagnostic: CliDiagnostic): string[] {
  const head = [diagnostic.severity, diagnostic.code, diagnostic.path]
    .map((part) => plainText(part ?? "").trim())
    .filter((part) => part !== "")
    .join(" ");
  const message = plainText(diagnostic.message ?? "");
  const lines = [head ? `${head}: ${message}` : message];
  const fix = plainText(diagnostic.fix ?? "").trim();
  if (fix) lines.push(`Fix: ${fix}`);
  return lines;
}

/** OFN-FR-DZSQ: every diagnostic as plain text, in the form of OFN-FR-IHSQ. */
export function diagnosticsText(diagnostics: readonly CliDiagnostic[]): string {
  return diagnostics.flatMap(diagnosticLines).join("\n");
}

function text(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

/** One diagnostic of the wire, or null where it holds no message. */
function diagnosticOf(value: unknown): CliDiagnostic | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const message = typeof raw.message === "string" ? raw.message : null;
  if (message === null) return null;
  return {
    severity: text(raw.severity),
    code: text(raw.code),
    path: text(raw.path),
    message,
    fix: text(raw.fix),
  };
}

/**
 * The text of a rejection that is not an object: the `Error: ` that an
 * `Error` adds is removed.
 */
function rejectionText(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  return raw.startsWith("Error: ") ? raw.slice(7) : raw;
}

/**
 * OFP-FR-QLQJ: the failure of a rejection. An `OpenSpecFailure` object is
 * read field by field. A string, or an object in JSON, is read too; a typed
 * string `code: detail` gives the code and the detail with no diagnostic.
 */
export function toOpenSpecFailure(error: unknown): OpenSpecFailure {
  if (error && typeof error === "object" && !(error instanceof Error)) {
    return fromObject(error as Record<string, unknown>);
  }
  const raw = rejectionText(error).trim();
  if (raw.startsWith("{")) {
    try {
      const parsed: unknown = JSON.parse(raw);
      if (parsed && typeof parsed === "object") return fromObject(parsed as Record<string, unknown>);
    } catch {
      // Not an object, so it is read as a typed string.
    }
  }
  const at = raw.indexOf(":");
  const code = (at === -1 ? raw : raw.slice(0, at)).trim();
  const detail = at === -1 ? null : raw.slice(at + 1).trim() || null;
  // A wrapping refusal as a typed string names its cause as its detail, and
  // a refused archive names its change before it: `<change>: <cause>`.
  if (detail && WRAPPING_READS.has(code)) return { code, cause: detail, diagnostics: [] };
  if (detail && WRAPPING_ARCHIVES.has(code)) {
    const split = detail.indexOf(":");
    if (split !== -1) {
      return {
        code,
        change: detail.slice(0, split).trim() || null,
        cause: detail.slice(split + 1).trim() || null,
        diagnostics: [],
      };
    }
    return { code, cause: detail, diagnostics: [] };
  }
  return { code, detail, diagnostics: [] };
}

/** OFP-FR-ZDEE: the wrapping refusals whose cause is the code of a read. */
const WRAPPING_READS: ReadonlySet<string> = new Set([
  "apply_progress_unavailable",
  "openspec_validation_unavailable",
]);

/** OFP-FR-ZDEE: the wrapping refusals of an archive. */
const WRAPPING_ARCHIVES: ReadonlySet<string> = new Set(["change_not_archivable", "archive_failed"]);

function fromObject(raw: Record<string, unknown>): OpenSpecFailure {
  const code = typeof raw.code === "string" ? raw.code : rejectionText(raw.message ?? "");
  const list = Array.isArray(raw.diagnostics) ? raw.diagnostics : [];
  return {
    code,
    cause: text(raw.cause),
    change: text(raw.change),
    detail: text(raw.detail),
    threadId: text(raw.threadId ?? raw.thread_id),
    repairRunId: text(raw.repairRunId ?? raw.repair_run_id),
    repairRefusal: text(raw.repairRefusal ?? raw.repair_refusal),
    diagnostics: list.map(diagnosticOf).filter((d): d is CliDiagnostic => d !== null),
  };
}

/**
 * The OpenSpec failure of a rejection, where it gets a notice, or null. A
 * rejection of another refusal gets no notice, and its surface states it.
 */
export function noticeFailureOf(error: unknown): OpenSpecFailure | null {
  const failure = toOpenSpecFailure(error);
  return hasNotice(failure) ? failure : null;
}

/**
 * The typed string of a rejection, in the form that the backend writes for
 * an `OpenSpecFailure`: the code, the change of a wrapping refusal of an
 * archive, and then the cause where present and the detail otherwise. A
 * surface that reads typed strings reads it as one. The string keeps the
 * classifying code, but not the detail beside a cause, so a surface that
 * states the failure takes the object (`noticeFailureOf`).
 */
export function refusalString(error: unknown): string {
  if (error && typeof error === "object" && !(error instanceof Error)) {
    const failure = toOpenSpecFailure(error);
    const parts = [failure.code];
    if (WRAPPING_ARCHIVES.has(failure.code) && failure.change && failure.cause) parts.push(failure.change);
    const tail = failure.cause ?? failure.detail;
    if (tail) parts.push(tail);
    return parts.join(": ");
  }
  return String(error);
}

/**
 * OFN-FR-LKAT: what makes two failures the same notice: the code, the cause,
 * and the CLI state. A surface that reads the same failure again shows the
 * same notice, and logs no new record.
 */
export function noticeKey(failure: OpenSpecFailure): string {
  return [failure.code, failure.cause ?? "", cliStateOf(failure) ?? ""].join("|");
}

/**
 * OFN-FR-LKAT: the one record of a notice that appears. It holds the surface,
 * the code, the cause, and the class, and the CLI state alone of the detail.
 * It holds no diagnostic and no change name.
 */
export function logFailureNotice(surface: string, failure: OpenSpecFailure): void {
  const fields: Record<string, string> = {
    surface,
    code: failure.code,
    cause: failure.cause ?? "",
    class: failureClass(failure),
  };
  const state = cliStateOf(failure);
  if (state) fields.state = state;
  logWarn(["frontend"], "an openspec failure notice appeared", fields);
}
