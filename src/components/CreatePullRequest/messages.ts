/**
 * The words of the Create a PR window
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-VYPG,
 * CPR-FR-VZNE).
 *
 * Kept apart from the window so the wording of a block or a failure is read and
 * tested without a render. No text here carries a token: none reaches the
 * window (per `GTC-git.md` GTC-FR-11).
 */
import type { PullRequestHeadState } from "../../types";
import { parseRejection, rejectionMessage } from "../Git/errors";

/** The rejection codes this window gives wording of its own. */
const PR_ERRORS = {
  exists: "pull_request_exists",
  rejected: "pull_request_rejected",
  titleRequired: "pull_request_title_required",
  unknownBranch: "unknown branch",
} as const;

/** How many uncommitted paths a block names before it counts the rest. */
const NAMED_PATHS = 3;

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

/** CPR-FR-VZNE: a failure of the creation, in words. */
export function pullRequestFailureMessage(error: unknown): string {
  const { code, detail } = parseRejection(error);
  switch (code) {
    case PR_ERRORS.exists:
      return "A pull request for this head branch and base branch already exists.";
    case PR_ERRORS.rejected:
      return detail
        ? `GitHub refused the pull request: ${detail}`
        : "GitHub refused the pull request.";
    case PR_ERRORS.titleRequired:
      return "A pull request needs a title.";
    case PR_ERRORS.unknownBranch:
      return "GitHub does not hold the head branch or the base branch. Push the head branch and choose an existing base branch.";
    default:
      return rejectionMessage(error);
  }
}

/**
 * CPR-FR-SQGZ: a failed read of the head state, in words. The read is local,
 * so a branch it does not find is one this repository does not hold, which is
 * not what GitHub's refusal of the same name says.
 */
export function pullRequestHeadReadMessage(
  error: unknown,
  head: string,
  base: string,
): string {
  if (parseRejection(error).code === PR_ERRORS.unknownBranch) {
    return `This repository holds no branch ${head}, or no branch ${base}.`;
  }
  return pullRequestFailureMessage(error);
}

/** CPR-FR-VYPG: a block that stands in the way of **Submit**. */
export interface PullRequestBlock {
  /** Stable key, also used as a test hook. */
  key: "no-remote" | "not-pushed" | "unpushed" | "uncommitted" | "nothing";
  text: string;
}

/**
 * CPR-FR-VYPG: every block the head state holds, in the order the author acts
 * on them: commit, then push. Several may stand together.
 */
export function pullRequestBlocks(state: PullRequestHeadState): PullRequestBlock[] {
  const blocks: PullRequestBlock[] = [];
  const paths = state.uncommittedPaths;
  if (paths.length > 0) {
    const named = paths.slice(0, NAMED_PATHS).join(", ");
    const rest = paths.length - NAMED_PATHS;
    blocks.push({
      key: "uncommitted",
      text: `${paths.length} uncommitted ${plural(paths.length, "file", "files")} (${named}${rest > 0 ? `, and ${rest} more` : ""}). Commit first.`,
    });
  }
  if (!state.hasRemote) {
    blocks.push({
      key: "no-remote",
      text: "This repository has no remote to push to.",
    });
  } else if (!state.remoteBranchExists) {
    blocks.push({
      key: "not-pushed",
      text: "The branch is not on the remote. Push it first.",
    });
  } else if ((state.unpushed ?? 0) > 0) {
    const n = state.unpushed ?? 0;
    blocks.push({
      key: "unpushed",
      text: `${n} ${plural(n, "commit is", "commits are")} not on the remote. Push first.`,
    });
  }
  if (state.aheadOfBase === 0) {
    blocks.push({
      key: "nothing",
      text: `Nothing to propose: ${state.head} holds no commit that ${state.base} lacks.`,
    });
  }
  return blocks;
}
