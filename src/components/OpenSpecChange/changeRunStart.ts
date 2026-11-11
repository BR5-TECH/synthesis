/**
 * What a refused start of an OpenSpec change run says to the author
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-EKYF,
 * OCW-FR-SCIA, OCW-FR-CTWP; the refusals of
 * `../../../specifications/core/OCR-openspec-change-runs.md`).
 *
 * The same words stand beside Apply, in the Rework dialog, and on a request
 * card whose confirmation was refused.
 */
import { graduationErrorMessage, splitTyped } from "../../state/graduation";
import { CHANGE_RUN_ERRORS, type ChangeAction } from "../../types";

/** A refused start, as a surface states it. */
export interface StartRefusal {
  code: string;
  sentence: string;
  /** `openspec_run_active`: the run that holds the change, for its route. */
  runId: string | null;
  /** `openspec_mode_inactive` and `openspec_cli_unavailable` route to Project settings. */
  settings: boolean;
  /** OCR-FR-SFDE / OCW-FR-SCIA: the thread a failed enqueue left open, if any. */
  threadId: string | null;
}

/** The refusals of the image preflight (per `GSU-graduation-start.md` GSU-FR-GLVQ). */
const IMAGE_PREFLIGHT: ReadonlySet<string> = new Set([
  CHANGE_RUN_ERRORS.imageUnconfigured,
  CHANGE_RUN_ERRORS.imageInvalid,
  CHANGE_RUN_ERRORS.executionUnsupported,
  CHANGE_RUN_ERRORS.dockerUnverified,
  CHANGE_RUN_ERRORS.socketUnavailable,
]);

/** What the author configures for each refusal of the image preflight. */
const IMAGE_CONFIGURE: Record<string, string> = {
  [CHANGE_RUN_ERRORS.imageUnconfigured]:
    "Configure an agent image for the active agent in Project settings, then start again.",
  [CHANGE_RUN_ERRORS.imageInvalid]:
    "Choose an agent image that can carry a run in Project settings, then start again.",
  [CHANGE_RUN_ERRORS.executionUnsupported]:
    "Choose an agent that can run in a container in Project settings, then start again.",
  [CHANGE_RUN_ERRORS.dockerUnverified]:
    "Verify the Docker backend of this machine in Settings, then start again.",
};

/**
 * A refusal as a code, its detail, and the thread it carries. A refusal is
 * `code` or `code: detail`, with an optional `; thread: <id>` suffix
 * (OCR-FR-SFDE), or a JSON object with `code` and `threadId`.
 */
export function parseStartError(error: unknown): {
  code: string;
  detail: string | null;
  threadId: string | null;
} {
  const fromObject = (value: Record<string, unknown>) => {
    const raw = typeof value.code === "string" ? value.code : String(value.message ?? "");
    const [code, detail] = splitTyped(raw);
    const thread = value.threadId ?? value.thread_id;
    return { code, detail, threadId: typeof thread === "string" && thread !== "" ? thread : null };
  };
  if (error !== null && typeof error === "object" && !(error instanceof Error)) {
    return fromObject(error as Record<string, unknown>);
  }
  const text = error instanceof Error ? error.message : String(error);
  const trimmed = text.trim();
  if (trimmed.startsWith("{")) {
    try {
      return fromObject(JSON.parse(trimmed) as Record<string, unknown>);
    } catch {
      // Not an object, so it is read as a typed string.
    }
  }
  // OCR-FR-SFDE: a failure after the discussion opened ends in `; thread: <id>`.
  const marker = trimmed.lastIndexOf(THREAD_SUFFIX);
  const typed = marker >= 0 ? trimmed.slice(0, marker) : trimmed;
  const thread = marker >= 0 ? trimmed.slice(marker + THREAD_SUFFIX.length).trim() : "";
  const [code, detail] = splitTyped(typed);
  return { code, detail, threadId: thread !== "" ? thread : null };
}

const THREAD_SUFFIX = "; thread: ";

/**
 * OCW-FR-EKYF / OCW-FR-SCIA: the statement of a refused start. `runTitleOf`
 * names the run of `openspec_run_active` where the surface knows it.
 * `action` is the run that the start asked for: an apply start is also
 * refused for a closed proposal whose commit is still owed (OCR-FR-LNKQ).
 */
export function startRefusal(
  error: unknown,
  runTitleOf: (runId: string) => string | null,
  action: ChangeAction = "rework",
): StartRefusal {
  const { code, detail, threadId } = parseStartError(error);
  const E = CHANGE_RUN_ERRORS;
  const base = { code, runId: null, settings: false, threadId };
  switch (code) {
    case E.runActive: {
      const runId = detail ?? null;
      const title = runId ? runTitleOf(runId) : null;
      const who = title ? `The run “${title}”` : "Another run";
      return {
        ...base,
        runId,
        sentence: `${who} of this change on this stream has not ended, so no run started.`,
      };
    }
    case E.notReady: {
      const missing = (detail ?? "")
        .split(",")
        .map((a) => a.trim())
        .filter(Boolean);
      const named = missing.length > 0 ? `: ${missing.join(", ")}` : "";
      return {
        ...base,
        sentence: `The change is not ready to apply. It is missing artifacts${named}. No run started.`,
      };
    }
    case E.modeInactive:
      return {
        ...base,
        settings: true,
        sentence: "OpenSpec mode is not active for this project, so no run started.",
      };
    case E.cliUnavailable:
      return {
        ...base,
        settings: true,
        sentence: "The OpenSpec CLI is not available on this machine, so no run started.",
      };
    case E.proposalPending:
      return {
        ...base,
        sentence:
          action === "apply"
            ? "The change holds a pending edit proposal, or accepted edits that are not committed. Decide the proposal or commit the edits first. No run started."
            : "The change holds a pending edit proposal. Decide it first, then start the rework. No run started.",
      };
    case E.blankInstruction:
      return { ...base, sentence: "The instruction is empty. Write what to change. No run started." };
    default:
      break;
  }
  if (IMAGE_PREFLIGHT.has(code)) {
    const what = graduationErrorMessage(detail ? `${code}: ${detail}` : code);
    const configure = IMAGE_CONFIGURE[code];
    return {
      ...base,
      sentence: [what, configure, "No run started."].filter(Boolean).join(" "),
    };
  }
  return { ...base, sentence: `The run could not start (${code || "unknown"}). No run started.` };
}
