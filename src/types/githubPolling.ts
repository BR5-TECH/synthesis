// GitHub polling (GPP-github-polling.md, consumed by GIT-git.md and
// SET-project-settings.md). Field names match the camelCase wire shapes of the
// polling commands byte-for-byte.

/** GPP-FR-HZDD: the polling intervals, in minutes. Unset is `null`. */
export type GithubPollingInterval = 1 | 5 | 15 | 30 | 60;

/** SET-FR-NLIX: the intervals the Project settings section offers, in order. */
export const GITHUB_POLLING_INTERVALS: readonly GithubPollingInterval[] = [
  1, 5, 15, 30, 60,
];

/** GPP-FR-NLPG: one selected GitHub Project and one interval. */
export interface GithubPollingSettings {
  projectNodeId: string | null;
  intervalMinutes: GithubPollingInterval | null;
}

/** GPP-FR-WYRP: one GitHub Project the token can read. */
export interface GithubProjectOption {
  nodeId: string;
  title: string;
  ownerLogin: string;
  number: number;
}

/** GPP-FR-DEZO / GPP-FR-EHRC: whether the selected Project permits polling. */
export interface GithubPollingConfiguration {
  state: "unset" | "unchecked" | "valid" | "invalid";
  errorCode: string | null;
  error: string | null;
  projectTitle: string | null;
}

/** GPP-FR-XSKT: one eligible issue of the last successful poll. */
export interface GithubReadyTask {
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  title: string;
  url: string;
  status: string;
}

/** GPP-FR-DHQM: a claim whose local steps are not acknowledged yet. */
export interface GithubPendingClaim {
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  issueUrl: string;
  projectNodeId: string;
  draftId: string | null;
  claimedAt: string;
}

/** GPP-FR-UBDE: one GitHub-shadow draft of the active worktree. */
export interface GithubShadowDraftRow {
  draftId: string;
  name: string;
  status: "github_shadow" | "graduated";
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  issueUrl: string;
  projectNodeId: string;
  /** A non-terminal graduation run holds the draft. */
  locked: boolean;
}

/** GPP-FR-XSKT … GPP-FR-UBDE: everything the Ready tasks section renders. */
export interface GithubPollingView {
  settings: GithubPollingSettings;
  configuration: GithubPollingConfiguration;
  repository: { owner: string; name: string } | null;
  polling: boolean;
  tasks: GithubReadyTask[];
  stale: boolean;
  lastErrorCode: string | null;
  lastError: string | null;
  lastSuccessAt: string | null;
  pendingClaims: GithubPendingClaim[];
  shadows: GithubShadowDraftRow[];
}

/** GPP-FR-CWGH: the shadow draft a claim or a retry produced. */
export interface GithubClaimResult {
  draftId: string;
  draftName: string;
}

/** GPP-FR-TYOV: one issue a poll reports as new. */
export interface GithubNewIssue {
  issueNumber: number;
  title: string;
}

/** GPP-FR-TYOV: the payload of `"github-polling-changed"`. */
export interface GithubPollingChangedPayload {
  newIssues: GithubNewIssue[];
}
