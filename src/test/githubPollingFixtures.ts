/**
 * Test fixtures for the GitHub polling view (`GPP-github-polling.md`).
 */
import type {
  GithubPendingClaim,
  GithubPollingView,
  GithubReadyTask,
  GithubShadowDraftRow,
} from "../types";

export function readyTask(over: Partial<GithubReadyTask> = {}): GithubReadyTask {
  return {
    repositoryOwner: "acme",
    repositoryName: "platform",
    issueNumber: 42,
    title: "Add retry to the sync job",
    url: "https://github.com/acme/platform/issues/42",
    status: "Ready",
    ...over,
  };
}

export function pendingClaim(
  over: Partial<GithubPendingClaim> = {},
): GithubPendingClaim {
  return {
    repositoryOwner: "acme",
    repositoryName: "platform",
    issueNumber: 17,
    issueUrl: "https://github.com/acme/platform/issues/17",
    projectNodeId: "P1",
    draftId: null,
    claimedAt: "2026-09-30T10:00:00Z",
    ...over,
  };
}

export function shadowRow(
  over: Partial<GithubShadowDraftRow> = {},
): GithubShadowDraftRow {
  return {
    draftId: "gh-1",
    name: "Cache invalidation",
    status: "github_shadow",
    repositoryOwner: "acme",
    repositoryName: "platform",
    issueNumber: 9,
    issueUrl: "https://github.com/acme/platform/issues/9",
    projectNodeId: "P1",
    locked: false,
    ...over,
  };
}

/** A configured, valid view with one unclaimed task. */
export function pollingView(
  over: Partial<GithubPollingView> = {},
): GithubPollingView {
  return {
    settings: { projectNodeId: "P1", intervalMinutes: 5 },
    configuration: {
      state: "valid",
      errorCode: null,
      error: null,
      projectTitle: "Roadmap",
    },
    repository: { owner: "acme", name: "platform" },
    polling: false,
    tasks: [readyTask()],
    stale: false,
    lastErrorCode: null,
    lastError: null,
    lastSuccessAt: "2026-10-01T09:00:00Z",
    pendingClaims: [],
    shadows: [],
    ...over,
  };
}
