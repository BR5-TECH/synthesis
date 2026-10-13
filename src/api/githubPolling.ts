/**
 * GitHub polling (`GPP-github-polling.md`): the Ready tasks of the Git panel
 * and the GitHub polling section of Project settings.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under. Every
 * refusal is a bare typed code (GPP-FR-SUFH).
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  GithubClaimResult,
  GithubPollingInterval,
  GithubPollingView,
  GithubProjectOption,
  GithubProjectStatusOption,
} from "../types";

/** GPP-FR-WYRP: the GitHub Projects the token can read. */
export const listGithubProjects = () =>
  invoke<GithubProjectOption[]>("list_github_projects");

/** GPP-FR-NNSJ: the options of one Project's `Status` field. Read-only. */
export const listGithubProjectStatuses = (projectNodeId: string) =>
  invoke<GithubProjectStatusOption[]>("list_github_project_statuses", {
    projectNodeId,
  });

/** The polling view, read from memory and disk with no network call. */
export const getGithubPollingState = () =>
  invoke<GithubPollingView>("get_github_polling_state");

/**
 * GPP-FR-IURX / GPP-FR-DBPI: persist the Project, the interval, and the two
 * status names, then validate.
 */
export const setGithubPollingSettings = (
  projectNodeId: string | null,
  intervalMinutes: GithubPollingInterval | null,
  readyStatus: string | null,
  inProgressStatus: string | null,
) =>
  invoke<GithubPollingView>("set_github_polling_settings", {
    projectNodeId,
    intervalMinutes,
    readyStatus,
    inProgressStatus,
  });

/** GPP-FR-XSKT / GPP-FR-XZTP: run one poll, or join the one in flight. */
export const pollGithubReadyTasks = () =>
  invoke<GithubPollingView>("poll_github_ready_tasks");

/** GPP-FR-IFVC: move the issue to the In Progress status and create its shadow draft. */
export const claimGithubTask = (issueNumber: number) =>
  invoke<GithubClaimResult>("claim_github_task", { issueNumber });

/** GPP-FR-TOED: finish the local steps of a pending claim. */
export const retryGithubClaim = (issueNumber: number) =>
  invoke<GithubClaimResult>("retry_github_claim", { issueNumber });

/** GPP-FR-BSLI: remove the pending claim once the start dialog is open. */
export const acknowledgeGithubClaim = (issueNumber: number) =>
  invoke<void>("acknowledge_github_claim", { issueNumber });

/** GPP-FR-WOLE: open a ready task's issue in the default browser. */
export const openGithubTaskIssue = (issueNumber: number) =>
  invoke<void>("open_github_task_issue", { issueNumber });
