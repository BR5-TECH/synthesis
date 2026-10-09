/**
 * The typed rejections of the Git panel's operations, in words
 * (`../../../specifications/ui/GIT-git.md` GIT-FR-TFAU, GIT-FR-GAMV,
 * GIT-FR-UDKY, GIT-FR-LKRX).
 *
 * The backend rejects with a string: a code, optionally followed by `: detail`.
 * A code is not a sentence. This module is the one place that parses a
 * rejection and turns its code into a plain sentence, so every region of the
 * panel says the same thing about the same code.
 */

import { tlsErrorMessage } from "../../tlsError";

export const GIT_ERRORS = {
  notARepository: "not a git repository",
  unknownBranch: "unknown branch",
  unknownCommit: "unknown_commit",
  pathNotInCommit: "path_not_in_commit",
  noComparisonBase: "no_comparison_base",
  noMergeBase: "no_merge_base",
  pathNotInComparison: "path_not_in_comparison",
  notALocalBranch: "not_a_local_branch",
  inPrimaryWorktree: "branch_in_primary_worktree",
  inActiveWorktree: "branch_in_active_worktree",
  belongsToStream: "branch_belongs_to_work_stream",
  directGraduation: "direct graduation active",
  worktreeDirty: "worktree_dirty",
  noRemote: "no remote configured",
  notGithubRemote: "not_a_github_remote",
  pullRequestNotFound: "pull_request_not_found",
  tokenRejected: "github_token_rejected",
  githubUnreachable: "github_unreachable",
  tokenSelectionRequired: "github_token_selection_required",
  tokenMissing: "github_token_missing",
  streamBusy: "stream_busy",
  streamHasRuns: "stream_has_runs",
  streamActive: "stream_active",
  streamUnmerged: "stream_unmerged",
  streamDirty: "stream_dirty",
  unknownStream: "unknown_stream",
  /** Raised by the panel itself when the author closes the token picker. */
  tokenSelectionCancelled: "github_token_selection_cancelled",
} as const;

export interface Rejection {
  code: string;
  detail: string | null;
}

/** Split a rejection into its code and its detail. */
export function parseRejection(error: unknown): Rejection {
  const raw =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : error == null
          ? ""
          : String(error);
  const cleaned = raw.replace(/^Error:\s*/, "").trim();
  const at = cleaned.indexOf(": ");
  if (at === -1) return { code: cleaned, detail: null };
  return {
    code: cleaned.slice(0, at).trim(),
    detail: cleaned.slice(at + 2).trim(),
  };
}

/** The paths a `worktree_dirty` or `stream_dirty` rejection carries. */
export function rejectionPaths(rejection: Rejection): string[] {
  if (!rejection.detail) return [];
  return rejection.detail
    .split(", ")
    .map((p) => p.trim())
    .filter((p) => p !== "");
}

export function isTokenSelectionRequired(error: unknown): boolean {
  return parseRejection(error).code === GIT_ERRORS.tokenSelectionRequired;
}

export function isTokenMissing(error: unknown): boolean {
  return parseRejection(error).code === GIT_ERRORS.tokenMissing;
}

/** What the sentence is about: the branch the author acted on, when there is one. */
export interface RejectionSubject {
  branch?: string;
}

/**
 * GIT-FR-UDKY: a typed error in words, naming the branch it concerns.
 *
 * A reason this panel has no wording for is shown whole, so it is never
 * swallowed and never cut at its first colon.
 */
export function rejectionMessage(
  error: unknown,
  subject: RejectionSubject = {},
): string {
  const rejection = parseRejection(error);
  const { code, detail } = rejection;
  // AAP-FR-LRTC: a refused certificate names its host and its cause.
  const tls = tlsErrorMessage(code);
  if (tls) return tls;
  const branch = subject.branch ? `Branch ${subject.branch}` : "This branch";
  switch (code) {
    case GIT_ERRORS.notARepository:
      return "This project is not inside a Git repository.";
    case GIT_ERRORS.unknownBranch:
      return `${branch} does not exist any more.`;
    case GIT_ERRORS.unknownCommit:
      return "That commit does not exist any more.";
    case GIT_ERRORS.pathNotInCommit:
      return "That commit did not change this file.";
    case GIT_ERRORS.noComparisonBase:
      return `The base branch of ${subject.branch ?? "this branch"} does not exist, so there is nothing to compare it with.`;
    case GIT_ERRORS.noMergeBase:
      return `${branch} shares no commit with its base branch, so there is nothing to compare.`;
    case GIT_ERRORS.pathNotInComparison:
      return `${branch} did not change this file against its base.`;
    case GIT_ERRORS.notALocalBranch:
      return `${branch} is not a local branch.`;
    case GIT_ERRORS.inPrimaryWorktree:
      return `${branch} is checked out in the primary worktree. Check out another branch or switch worktree first.`;
    case GIT_ERRORS.inActiveWorktree:
      return `${branch} is checked out in the active worktree. Check out another branch or switch worktree first.`;
    case GIT_ERRORS.belongsToStream:
      return `${branch} belongs to a work stream. Delete the work stream instead.`;
    case GIT_ERRORS.directGraduation:
      return `A graduation is running on ${subject.branch ?? "this branch"}. Wait until it ends.`;
    case GIT_ERRORS.worktreeDirty:
      return `The worktree of ${subject.branch ?? "this branch"} has uncommitted changes.`;
    case GIT_ERRORS.noRemote:
      return "This repository has no remote configured.";
    case GIT_ERRORS.notGithubRemote:
      return "The remote of this repository is not on github.com.";
    case GIT_ERRORS.pullRequestNotFound:
      return "That pull request does not exist any more.";
    case GIT_ERRORS.tokenRejected:
      return "GitHub rejected the token of this project. Check it in Global settings → GitHub.";
    case GIT_ERRORS.githubUnreachable:
      return "Could not reach GitHub. Check the network and try again.";
    case GIT_ERRORS.tokenSelectionRequired:
      return "Choose which GitHub token this project should use.";
    case GIT_ERRORS.tokenSelectionCancelled:
      return "No GitHub token was selected, so nothing was read from GitHub.";
    case GIT_ERRORS.tokenMissing:
      return "No GitHub token is stored. Add one in Global settings → GitHub.";
    case GIT_ERRORS.streamBusy:
      return "A run holds this work stream or waits in its queue.";
    case GIT_ERRORS.streamHasRuns:
      return "Runs of this work stream are still working.";
    case GIT_ERRORS.streamActive:
      return "The worktree of this work stream is the active one. Switch to another worktree first.";
    case GIT_ERRORS.streamUnmerged:
      return detail
        ? `This work stream holds ${detail} commit${detail === "1" ? "" : "s"} its base branch does not hold. Merge it from the Streams window first.`
        : "This work stream holds commits its base branch does not hold. Merge it from the Streams window first.";
    case GIT_ERRORS.streamDirty:
      return "The work stream holds changes that are not committed.";
    case GIT_ERRORS.unknownStream:
      return "That work stream does not exist any more.";
    default:
      return detail ? `${code}: ${detail}` : code || "The operation failed.";
  }
}

/** True for the refusals that mean the author must pick something else. */
export function isStreamRefusal(code: string): boolean {
  return (
    code === GIT_ERRORS.streamBusy ||
    code === GIT_ERRORS.streamHasRuns ||
    code === GIT_ERRORS.streamActive ||
    code === GIT_ERRORS.streamUnmerged
  );
}
