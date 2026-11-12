/*
 * OpenSpec failures stand-in (OFP-openspec-failure-policy.md OFP-FR-QLQJ,
 * OFN-openspec-failure-notices.md), for UI auditing only.
 *
 * Each flag below takes a failure in the form `code`, `code:detail`, or
 * `wrapper/cause`, for example `cli_unavailable:not_found`,
 * `apply_progress_unavailable/cli_timed_out`, or
 * `archive_failed/archive_spec_update_failed`. The command then rejects with
 * an `OpenSpecFailure` object, camelCase as on the wire. A failure of the
 * class `transient` or `content` carries diagnostics, and an `environment`
 * failure carries none (OFP-FR-OFLU).
 *
 * Flags, by surface:
 * - `?openspecFail=<failure>`: the default of every flag below, so one flag
 *   reaches each surface (the read of the change fails too).
 * - `?openspecRead=<failure>`: every read of the change (OCW-FR-RFGG,
 *   OCW-FR-PFUQ). `?openspecReadAfter=1` lets the first read succeed, so the
 *   failure stands above the content (OCW-FR-PFUQ).
 * - `?openspecApply=`, `?openspecRework=`, `?openspecRequest=`: the starts and
 *   the confirmation of the change workspace (OCW-FR-EKYF, OCW-FR-CTWP).
 * - `?openspecProgress=<failure>`: `read_apply_progress`, for the short form
 *   of the run strip and of the Tasks stage (OCW-FR-BTSI, GRU-FR-PXCW).
 *   `?openspecProgress=fail` stays `cli_failed`.
 * - `?openspecPropose=<failure>`: a propose start (GSD-FR-RFIG), for example
 *   `cli_unavailable:unsupported`.
 * - `?openspecMerge=<failure>`: every stream merge (WSS-FR-NTGC). The failure
 *   names the change `add-logout-3f2a` and the thread `ocd-req`. With
 *   `?openspecRepair=run` it carries the repair run `g-52`, and with
 *   `?openspecRepair=<code>` the repair refusal `<code>`, for example
 *   `proposal_pending`.
 * - `?openspecBlocker=<failure>`: the apply run `g-51` rests `blocked` on that
 *   OpenSpec blocker (GRU-FR-LBPR). The image probe rest is
 *   `openspec_image_cli_unavailable:failed`, and a gate rest is
 *   `openspec_validation_unavailable/cli_rejected`.
 * - `?openspecContinue=<failure>`: every Continue of a run (GRU-FR-BPNJ), for
 *   example `openspec_validation_unavailable/cli_failed` with
 *   `?openspecPlan=update`.
 * - `?openspecMap=<failure>`: every load of the map (SMP-FR-KBXW), and
 *   `?openspecMap=unavailable&openspecValidation=<failure>` the failure of
 *   the validation (SMP-FR-KVKD).
 * - `?openspecDiagnostics=<n>`: every failure carries its sample diagnostics
 *   `n` times, for the scrolled Details list (OFN-FR-UNEF).
 */

/** The value of a flag, or the value of `openspecFail` where it is absent. */
export function failureFlag(name: string): string | null {
  if (typeof location === "undefined") return null;
  const params = new URLSearchParams(location.search);
  return params.get(name) ?? params.get("openspecFail");
}

const ENVIRONMENT = new Set([
  "openspec_mode_inactive",
  "cli_unavailable",
  "cli_output_unreadable",
  "openspec_image_cli_unavailable",
]);

/** Sample diagnostics of the CLI for a classifying code. */
function diagnosticsFor(code: string, detail: string | null): unknown[] {
  if (ENVIRONMENT.has(code)) return [];
  switch (code) {
    case "cli_timed_out":
    case "cli_failed":
      return [
        {
          message:
            "node:internal/modules/cjs/loader:1228\n  throw err;\n  ^\n\nError: Cannot find module 'commander'\u0007",
        },
      ];
    case "change_not_ready":
      return [
        ...(detail ?? "design,tasks").split(",").map((artifact) => ({
          severity: "error",
          message: `Missing artifact: ${artifact.trim()}`,
        })),
        { severity: "warning", message: "The proposal names no capability." },
        { severity: "info", message: "Create the missing artifacts with /opsx:continue, then apply again." },
      ];
    default:
      return [
        {
          severity: "error",
          code: code.startsWith("archive_") ? code : "spec_delta_invalid",
          path: "openspec/changes/add-logout-3f2a/specs/auth/spec.md",
          message:
            'MODIFIED "Session Expiration" not found in auth. A long diagnostic wraps at a word: ' +
            "the live specification auth holds the requirements Login, Logout, and Session Renewal, and none of them is named Session Expiration.",
          fix: "Use ADDED for a new requirement, or RENAMED to rename one.",
        },
        { severity: "warning", path: "openspec/changes/add-logout-3f2a/tasks.md", message: "<b>Task 3</b> has **no** [link](https://example.test) check box." },
      ];
  }
}

/** One `OpenSpecFailure` from a flag value. */
export function failureOf(value: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  const [head, cause] = value.split("/");
  const at = head.indexOf(":");
  const code = (at === -1 ? head : head.slice(0, at)).trim();
  const detail = at === -1 ? null : head.slice(at + 1).trim() || null;
  const classifying = cause?.trim() || code;
  // OFN-FR-UNEF: `?openspecDiagnostics=<n>` repeats the sample diagnostics
  // `n` times, so the Details list grows above 12 lines and scrolls.
  const repeat =
    typeof location === "undefined"
      ? 1
      : Math.max(1, Number(new URLSearchParams(location.search).get("openspecDiagnostics") ?? "1") || 1);
  const diagnostics = Array.from({ length: repeat }, () => diagnosticsFor(classifying, detail)).flat();
  return {
    code,
    ...(cause ? { cause: cause.trim() } : {}),
    ...(detail ? { detail } : {}),
    ...extra,
    diagnostics,
  };
}

/** Throw the failure of a flag, where the flag is set. */
export function throwFailureFlag(name: string, extra: Record<string, unknown> = {}): void {
  const value = failureFlag(name);
  // eslint-disable-next-line no-throw-literal
  if (value) throw failureOf(value, extra);
}

/** The merge failure of `?openspecMerge`, with its thread and its repair, or null. */
export function mergeFailure(): Record<string, unknown> | null {
  const value = failureFlag("openspecMerge");
  if (!value) return null;
  const params = new URLSearchParams(location.search);
  const repair = params.get("openspecRepair");
  return failureOf(value, {
    change: "add-logout-3f2a",
    threadId: "ocd-req",
    ...(repair === "run" ? { repairRunId: "g-52" } : repair ? { repairRefusal: repair } : {}),
  });
}

/** The blocker of `?openspecBlocker`, in the shape of a run's blocker, or null. */
export function openspecBlocker(): Record<string, unknown> | null {
  const value = failureFlag("openspecBlocker");
  if (!value) return null;
  const failure = failureOf(value);
  return {
    ...failure,
    message: `${failure.code}: ${String(failure.cause ?? failure.detail ?? "")}`,
    clearsBy:
      failure.code === "openspec_image_cli_unavailable"
        ? "Fix the agent image of this project, then continue the run."
        : "Continue the run to run the OpenSpec step again.",
    attempt: 1,
  };
}
