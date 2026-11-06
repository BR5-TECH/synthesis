/**
 * What the Create a PR window's tests share
 * (`../../specifications/ui/CPR-create-pull-request.md`).
 *
 * The window reads more than the head state when it opens: the completed runs
 * of the head branch, the open pull requests, the agentic integration, the
 * PR summary runs in the queue, and the publication availability. A test that
 * is about something else answers those reads with these neutral answers, so
 * no read is rejected for a reason the test does not state.
 */
import { clearAllPullRequestSessions } from "../state/pullRequestSessions";
import { resetPullRequestSummariesForTest } from "../state/pullRequestSummaries";
import type {
  BranchCompletedRun,
  NotePublicationAvailability,
  PullRequestIssueRef,
  PullRequestLinkOutcome,
} from "../types";

/** GHP-FR-CNPX: a project whose publication is not available. */
export const PUBLICATION_UNAVAILABLE: NotePublicationAvailability = {
  available: false,
  reasonCode: "no_github_remote",
  reason: "The project has no GitHub remote.",
  remoteName: null,
  repositoryHost: null,
  repositoryOwner: null,
  repositoryName: null,
};

/** GHP-FR-CNPX: a project that publishes to `acme/platform` through `origin`. */
export const PUBLICATION_AVAILABLE: NotePublicationAvailability = {
  available: true,
  reasonCode: null,
  reason: null,
  remoteName: "origin",
  repositoryHost: "github.com",
  repositoryOwner: "acme",
  repositoryName: "platform",
};

/** GTC-FR-WJUG: every issue linked by a closing keyword. */
export function linkedAll(
  issues: PullRequestIssueRef[],
  closesOnMerge = true,
): PullRequestLinkOutcome {
  return {
    linked: issues.map((issue) => ({ issue, way: "closing_keyword" })),
    failed: [],
    closesOnMerge,
  };
}

/**
 * The neutral answer of each read the window makes besides the head state, or
 * `undefined` when `cmd` is not one of them.
 */
export function pullRequestWindowRead(
  cmd: string,
  args?: Record<string, unknown>,
): { answer: unknown } | undefined {
  switch (cmd) {
    case "list_branch_completed_runs":
      return { answer: [] as BranchCompletedRun[] };
    case "list_pull_requests":
      return { answer: [] };
    case "get_project_agentic_integration":
      return {
        answer: { vendor: "claude_code", resolution: "inherited", overrideVendor: null },
      };
    case "list_graduation_queue":
      return { answer: { projectKey: "/dev/acme", runs: [] } };
    case "get_note_publication_availability":
      return { answer: PUBLICATION_UNAVAILABLE };
    case "link_pull_request_issues":
      return { answer: linkedAll((args?.issues as PullRequestIssueRef[]) ?? []) };
    default:
      return undefined;
  }
}

/** Reset every module store the window touches. Call in `beforeEach`. */
export function resetCreatePullRequestStores(): void {
  clearAllPullRequestSessions();
  resetPullRequestSummariesForTest();
}

/** GRD-FR-BHFP: one completed run of a branch, with a run summary when given. */
export function branchRun(
  runId: string,
  over: Partial<BranchCompletedRun> = {},
): BranchCompletedRun {
  return {
    runId,
    title: `Run ${runId}`,
    draftId: `d-${runId}`,
    draftName: `Draft ${runId}`,
    kind: "draft",
    landing: { branch: "feature/panel", commit: "c".repeat(40), at: "2026-09-06T10:00:00Z" },
    runSummary: null,
    ...over,
  };
}
