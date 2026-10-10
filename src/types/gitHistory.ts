/**
 * The Git panel's history, branch, and pull request payloads
 * (`../../specifications/core/GTC-git.md` payload shapes). Field names are the
 * camelCase form of the spec's snake_case names.
 */

/** GTC-FR-RFLW: one commit as the log rail and the branch overlay render it. */
export interface CommitSummary {
  id: string;
  shortId: string;
  authorName: string;
  authorEmail: string;
  /** Unix seconds, UTC. */
  authoredAt: number;
  subject: string;
  message: string;
  /** Short names of the local and remote-tracking branches whose tip this is. */
  refs: string[];
}

/** GTC-FR-BQNM: the active worktree's latest commits, newest first. */
export interface CommitHistory {
  /** Absent when `HEAD` is detached. */
  branch?: string;
  isDetached: boolean;
  headId?: string;
  commits: CommitSummary[];
}

export type CommitFileStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "type_changed";

/** GTC-FR-YCEV: one path a commit changed. */
export interface CommitFile {
  path: string;
  previousPath?: string;
  status: CommitFileStatus;
  isBinary: boolean;
}

export interface BranchWorktreeRef {
  path: string;
  name: string;
  isActive: boolean;
  isPrimary: boolean;
}

export interface BranchStreamRef {
  streamId: string;
  streamName: string;
}

/** GTC-FR-TGOI: what one branch is and holds. */
export interface BranchInformation {
  name: string;
  kind: "local" | "remote";
  upstream?: string;
  isCurrent: boolean;
  worktree?: BranchWorktreeRef;
  stream?: BranchStreamRef;
  tip: CommitSummary;
  commits: CommitSummary[];
}

/**
 * GTC-FR-YQVD / GTC-FR-MBBH: one branch against its base. `mergeBase` is absent
 * and `files` is empty when the branch is its own base.
 */
export interface BranchComparison {
  branch: string;
  kind: "local" | "remote";
  base: string;
  mergeBase?: string;
  sameAsBase: boolean;
  files: CommitFile[];
}

/** GTC-FR-UMXA: what deleting a local branch would remove and discard. */
export interface BranchDeletionPlan {
  branch: string;
  worktree?: BranchWorktreeRef;
  /**
   * GTC-FR-JOWX: `aheadOfBase` counts the commits the stream holds that its
   * base branch does not, as WKS-FR-EIBC counts them.
   */
  stream?: BranchStreamRef & { busyRunId: string | null; aheadOfBase: number };
  uncommittedPaths: string[];
  remoteBranch?: string;
}

export type RemoteDeletionState = "not_requested" | "deleted" | "failed";

/** GTC-FR-WNZH / GTC-FR-FSRQ: the result of a branch deletion. */
export interface BranchDeletionOutcome {
  branch: string;
  removedWorktreePath?: string;
  remote: {
    requested: boolean;
    state: RemoteDeletionState;
    branch?: string;
    error?: string;
  };
}

export type PullRequestListState = "open" | "closed";
export type PullRequestState = "open" | "closed" | "merged";

/** GTC-FR-GXUB */
export interface PullRequestSummary {
  number: number;
  title: string;
  state: PullRequestState;
  isDraft: boolean;
  author: string;
  headBranch: string;
  baseBranch: string;
  createdAt: string;
  updatedAt: string;
}

/** GTC-FR-MMFM: the pull request GitHub created. */
export interface CreatedPullRequest {
  number: number;
  /** The address of its page on the GitHub host of the repository. */
  url: string;
}

/** GTC-FR-NEIW, GTC-FR-YWCP: what stands between a branch and a pull request. */
export interface PullRequestHeadState {
  head: string;
  base: string;
  hasRemote: boolean;
  remoteBranchExists: boolean;
  /** Absent (null) while the remote has no branch of the head's name. */
  unpushed: number | null;
  uncommittedPaths: string[];
  aheadOfBase: number;
}

/** GTC-FR-ZIHE */
export interface PullRequestDetail {
  number: number;
  title: string;
  state: PullRequestState;
  isDraft: boolean;
  author: string;
  body: string;
  url: string;
  headBranch: string;
  baseBranch: string;
  createdAt: string;
  updatedAt: string;
  closedAt?: string;
  mergedAt?: string;
}

export type PullRequestTimelineKind =
  | "comment"
  | "review"
  | "review_comment"
  | "commit"
  | "event";

export type PullRequestReviewState =
  | "approved"
  | "changes_requested"
  | "commented"
  | "dismissed"
  | "pending";

/** GTC-FR-CKTM */
export interface PullRequestTimelineItem {
  id: string;
  kind: PullRequestTimelineKind;
  actor?: string;
  createdAt: string;
  body?: string;
  reviewState?: PullRequestReviewState;
  event?: string;
  path?: string;
  commitId?: string;
  subject?: string;
}

export interface PullRequestTimeline {
  /** Oldest first. */
  items: PullRequestTimelineItem[];
  truncated: boolean;
}
