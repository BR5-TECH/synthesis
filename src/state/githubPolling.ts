/**
 * Pure rules over the GitHub polling view (`GPP-github-polling.md`), shared by
 * the main window's polling schedule, the Git panel's Ready tasks section, and
 * the Project settings GitHub Project settings section.
 *
 * Kept free of React so each rule is tested as a function of the view alone.
 */
import { tlsErrorMessage } from "../tlsError";
import { GITHUB_HOST_MISMATCH_MESSAGE } from "../types/github";
import type {
  GithubNewIssue,
  GithubPollingConfiguration,
  GithubPollingInterval,
  GithubPollingView,
  GithubProjectStatusOption,
} from "../types";

/**
 * GPP-FR-SUFH: the displayable text of each typed refusal. A refusal is a bare
 * code on the wire; the text tells the author what happened and, where it can,
 * what to change.
 */
const ERROR_TEXT: Record<string, string> = {
  no_project_open: "No project is open.",
  polling_unconfigured:
    "No GitHub Project is selected for polling. Select one in Project settings → GitHub Project settings.",
  polling_configuration_invalid:
    "The GitHub polling configuration has an error. Fix it in Project settings → GitHub Project settings.",
  invalid_interval: "That polling interval is not available.",
  duplicate_status_option:
    "The Ready status and the In Progress status must be different options.",
  project_unavailable:
    "The selected GitHub Project is missing or the GitHub token cannot read it. Select another Project in Project settings.",
  status_field_missing:
    "The selected GitHub Project has no single-select field named “Status”. Add it, or select another Project in Project settings.",
  ready_option_missing:
    "The Ready status is not an option of the “Status” field of the selected GitHub Project. Choose an option in Project settings → GitHub Project settings.",
  in_progress_option_missing:
    "The In Progress status is not an option of the “Status” field of the selected GitHub Project. Choose an option in Project settings → GitHub Project settings.",
  no_remote_configured: "This repository has no remote, so there is no GitHub repository to poll.",
  no_github_remote: "No remote of this repository is on a GitHub host.",
  github_host_mismatch:
    GITHUB_HOST_MISMATCH_MESSAGE,
  token_unavailable:
    "No GitHub token is available for this project. Add or select one in settings.",
  issues_inaccessible: "The GitHub token cannot read the issues of this repository.",
  issues_create_forbidden: "The GitHub token cannot change issues in this repository.",
  issues_disabled: "Issues are disabled in this GitHub repository.",
  github_unreachable: "GitHub could not be reached. Check the network and try again.",
  github_request_failed: "GitHub refused the request. Try again later.",
  task_not_ready:
    "This issue is no longer a ready Task on GitHub, so it was not claimed.",
  claim_pending: "This issue already has a claim that is not finished. Use Retry.",
  claim_in_progress: "A claim of this issue is already running.",
  no_pending_claim: "This issue has no unfinished claim.",
  status_update_failed:
    "GitHub did not move the issue to the In Progress status, so nothing was claimed.",
  pending_claim_write_failed: "The claim could not be recorded on this machine.",
  shadow_draft_create_failed:
    "The issue is in the In Progress status, but its draft could not be created. Use Retry.",
  issue_not_listed: "This issue is not in the current list of ready tasks.",
  invalid_publication_settings:
    "Keep at least one parent issue Type, and name a sub-issue Type.",
  parent_issues_unreadable: "The open issues of this repository could not be read.",
  issue_types_unreadable: "The issue Types of this repository could not be read.",
  draft_github_shadow:
    "This draft mirrors a GitHub issue, so it cannot be changed.",
};

/** The typed code a refusal carries, read off the raw rejection. */
export function refusalCode(error: unknown): string {
  const raw = String(error);
  const text = raw.startsWith("Error: ") ? raw.slice(7) : raw;
  const at = text.indexOf(":");
  return (at === -1 ? text : text.slice(0, at)).trim();
}

/** GPP-FR-SUFH: the displayable text of a refusal, by its code. */
export function githubPollingErrorMessage(error: unknown): string {
  // AAP-FR-LRTC: a refused certificate names its host and its cause.
  const tls = tlsErrorMessage(error);
  if (tls) return tls;
  const code = refusalCode(error);
  return ERROR_TEXT[code] ?? `The GitHub operation failed (${code || "unknown error"}).`;
}

/**
 * GIT-FR-CVSB / SET-FR-NLIX: whether the main window runs the launch poll and
 * the timed polls. It needs a Project, an interval, and a configuration that
 * is not `invalid`.
 */
export function timedPollingEnabled(view: GithubPollingView | null): boolean {
  if (!view) return false;
  return (
    view.settings.projectNodeId !== null &&
    view.settings.intervalMinutes !== null &&
    view.configuration.state !== "invalid"
  );
}

/**
 * GIT-FR-EZFL: whether Refresh may run a poll. It needs a Project and a
 * configuration that is not `invalid`; the interval does not matter.
 */
export function refreshAvailable(view: GithubPollingView | null): boolean {
  if (!view) return false;
  return (
    view.settings.projectNodeId !== null &&
    view.configuration.state !== "invalid"
  );
}

/** The interval in milliseconds, or null when it is Off. */
export function intervalMs(minutes: GithubPollingInterval | null): number | null {
  return minutes === null ? null : minutes * 60_000;
}

/**
 * GIT-FR-HNGQ: the one state the Ready tasks section renders.
 *
 * `unread` is the moment before the first read of the view lands, and
 * `unpolled` is a configured section that no poll has answered yet. Neither
 * reads as an empty result.
 */
export type ReadyTasksState =
  | "unread"
  | "not-configured"
  | "configuration-error"
  | "loading"
  | "unpolled"
  | "empty"
  | "rows";

export function readyTasksState(
  view: GithubPollingView | null,
  pollInFlight: boolean,
): ReadyTasksState {
  if (!view) return "unread";
  if (view.settings.projectNodeId === null) return "not-configured";
  if (view.configuration.state === "invalid") return "configuration-error";
  const anyRows =
    view.tasks.length > 0 ||
    view.pendingClaims.length > 0 ||
    view.shadows.length > 0;
  if (view.tasks.length > 0) return "rows";
  if ((view.polling || pollInFlight) && !anyRows) return "loading";
  // GIT-FR-RUAH: a failed poll keeps what the last successful one found and
  // marks it stale. That is never the empty state, even with no rows.
  if (view.stale) return "rows";
  if (view.lastSuccessAt === null) return "unpolled";
  return "empty";
}

/**
 * NTF-FR-JLXL: the body of the raise for one poll's new issues — their titles
 * where there are at most three, their count otherwise.
 */
export function newIssuesBody(issues: GithubNewIssue[]): string {
  if (issues.length <= 3) return issues.map((i) => i.title).join(", ");
  return `${issues.length} new ready tasks`;
}

/** NTF-FR-JLXL: the title of the same raise. */
export function newIssuesTitle(issues: GithubNewIssue[]): string {
  return issues.length === 1
    ? "New ready task on GitHub"
    : `${issues.length} new ready tasks on GitHub`;
}

/** GPP-FR-VXSB: the default name of each status. */
export const READY_STATUS_DEFAULT = "Ready";
export const IN_PROGRESS_STATUS_DEFAULT = "In Progress";

/**
 * SET-FR-CJZH: the option a selector selects by default — the first, in the
 * order of the field, whose name equals the default name case-insensitively
 * and that the other selector does not hold (SET-FR-QZYV). `null` where none
 * does.
 */
export function defaultStatusOption(
  options: readonly GithubProjectStatusOption[],
  defaultName: string,
  other: string | null = null,
): string | null {
  const wanted = defaultName.toLowerCase();
  return (
    options.find((o) => o.name.toLowerCase() === wanted && o.name !== other)?.name ?? null
  );
}

/**
 * SET-FR-SDKP: the status names to persist after a status read. A stored name
 * is kept as it is; an unset one takes its default where one matches. `null`
 * where nothing changes, so the read writes nothing.
 */
export function autoSelectedStatuses(
  stored: { readyStatus: string | null; inProgressStatus: string | null },
  options: readonly GithubProjectStatusOption[],
): { readyStatus: string | null; inProgressStatus: string | null } | null {
  // SET-FR-QZYV: a default never takes the option the other selector holds.
  const readyStatus =
    stored.readyStatus ??
    defaultStatusOption(options, READY_STATUS_DEFAULT, stored.inProgressStatus);
  const inProgressStatus =
    stored.inProgressStatus ??
    defaultStatusOption(options, IN_PROGRESS_STATUS_DEFAULT, readyStatus);
  if (
    readyStatus === stored.readyStatus &&
    inProgressStatus === stored.inProgressStatus
  ) {
    return null;
  }
  return { readyStatus, inProgressStatus };
}

/** NTF-FR-UZGZ: the key of the configuration-error raise. */
export const CONFIGURATION_ERROR_KEY = "github-polling-configuration";

/**
 * GIT-FR-AKYZ: what a change of the configuration asks of the notification
 * facility. `previous` is the error code of the configuration last seen, or
 * `null` where it was not `invalid`. It raises when the configuration becomes
 * `invalid` or changes its error code, and retracts when it leaves `invalid`.
 */
export function configurationErrorTransition(
  previous: string | null,
  configuration: GithubPollingConfiguration,
): { action: "raise" | "retract" | "none"; code: string | null } {
  const code =
    configuration.state === "invalid" ? (configuration.errorCode ?? "invalid") : null;
  if (code === previous) return { action: "none", code };
  if (code === null) return { action: "retract", code };
  return { action: "raise", code };
}

/** NTF-FR-RNGG: the title of the configuration-error raise. */
export const CONFIGURATION_ERROR_TITLE = "GitHub polling stopped";

/**
 * NTF-FR-RNGG: the body of the configuration-error raise. It names the
 * missing status and the Project for a missing status, and carries the
 * displayable error otherwise.
 */
export function configurationErrorBody(
  configuration: GithubPollingConfiguration,
): string {
  const project = configuration.projectTitle
    ? `the GitHub Project “${configuration.projectTitle}”`
    : "the selected GitHub Project";
  const which =
    configuration.errorCode === "ready_option_missing"
      ? "Ready"
      : configuration.errorCode === "in_progress_option_missing"
        ? "In Progress"
        : null;
  if (which && configuration.missingStatus) {
    return `The ${which} status “${configuration.missingStatus}” is not an option in ${project}. Choose an option in GitHub Project settings.`;
  }
  return (
    configuration.error ??
    githubPollingErrorMessage(configuration.errorCode ?? "polling_configuration_invalid")
  );
}

/** An RFC 3339 instant as a short local date and time. */
export function formatInstant(iso: string | null): string {
  if (!iso) return "never";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
