/**
 * The words a pull is reported in (`specifications/ui/WTS-worktree-selector.md`
 * WTS-FR-34, `specifications/ui/GIT-git.md` GIT-FR-PLWD, GIT-FR-RCDL).
 *
 * One module for the top-chrome pull control, the Git panel's **Pull**, and the
 * merge-or-rebase dialog, so the three never say a different thing about the
 * same outcome.
 */
import {
  GITHUB_TOKEN_ERRORS,
  GIT_REMOTE_ERRORS,
  PULL_ERRORS,
  type PullOutcome,
  type PullReconcileOutcome,
} from "../types";

/** The typed code at the head of an error, and the detail after its colon. */
function split(error: string): [string, string] {
  const at = error.indexOf(":");
  if (at < 0) return [error, ""];
  return [error.slice(0, at), error.slice(at + 1).trim()];
}

/** WTS-FR-34: the typed cause of a pull that did not land, in words. */
export function pullErrorText(error: string): string {
  const [code, detail] = split(error);
  switch (code) {
    case PULL_ERRORS.wouldOverwrite:
      return `The pull would overwrite uncommitted changes in ${detail}. Commit or discard them, then pull again.`;
    case PULL_ERRORS.noBranch:
      return "No branch is checked out — there is nothing to pull into.";
    case PULL_ERRORS.noUpstream:
      return "This branch tracks no remote branch — there is nothing to pull.";
    case PULL_ERRORS.upstreamGone:
      return "The remote branch this branch tracks is gone.";
    case PULL_ERRORS.upstreamNotOnPrimaryRemote:
      return "This branch tracks a branch of another remote, which is not fetched.";
    case PULL_ERRORS.tipsMoved:
      return "The branch or its upstream moved since the pull. Pull again.";
    case PULL_ERRORS.pullInProgress:
      return "Another pull is running in this repository.";
    case PULL_ERRORS.mergeInProgress:
    case PULL_ERRORS.updateInProgress:
      return "A work stream merge or update is running in this repository. Pull again when it ends.";
    case PULL_ERRORS.pullRunActive:
      return "A pull run is already reconciling this branch.";
    case PULL_ERRORS.directGraduationActive:
      return "A graduation run is working in this worktree. Pull again when it ends.";
    case PULL_ERRORS.workStreamBusy:
      return "A run is working in this work stream. Pull again when it ends.";
    case GIT_REMOTE_ERRORS.noRemoteConfigured:
      return "No remote is configured — there is nothing to pull.";
    case GITHUB_TOKEN_ERRORS.tokenMissing:
      return "Pulling from the remote needs a GitHub token.";
    case GITHUB_TOKEN_ERRORS.selectionRequired:
      return "No GitHub token was selected — nothing was pulled.";
    case GITHUB_TOKEN_ERRORS.invalidToken:
      return "The remote refused the credential — nothing was pulled.";
    case GITHUB_TOKEN_ERRORS.keychainUnavailable:
      return "That GitHub token could not be read from the keychain.";
    case GITHUB_TOKEN_ERRORS.hostMismatch:
      return "The project token belongs to another GitHub host than this remote — nothing was pulled.";
    case GITHUB_TOKEN_ERRORS.githubUnreachable:
      return "Couldn't reach the remote — nothing was pulled.";
    default:
      return `The pull did not run: ${error}`;
  }
}

/**
 * WTS-FR-34 / GIT-FR-PLWD: the note a returning pull leaves, or `null` for a
 * diverged branch, which opens the merge-or-rebase dialog instead
 * (WTS-FR-DVGQ, GIT-FR-MRBQ).
 */
export function pullNote(outcome: PullOutcome): string | null {
  const branch = outcome.branch ?? "The branch";
  switch (outcome.state) {
    case "up_to_date":
      return `${branch} is up to date with ${outcome.upstream ?? "its upstream"}.`;
    case "fast_forwarded":
      return `Pulled ${outcome.behind} commit${outcome.behind === 1 ? "" : "s"} into ${branch}.`;
    case "diverged":
      return null;
    case "fetch_failed":
      // A cause outside the known vocabulary is not dressed up as one of them,
      // and its raw code is no sentence (WTS-FR-34).
      return KNOWN_FETCH_CAUSES.has(outcome.error ?? "")
        ? pullErrorText(outcome.error ?? "")
        : "The remote could not be fetched — nothing was pulled.";
    default:
      return pullErrorText(outcome.error ?? "");
  }
}

/** The causes of a failed fetch `pullErrorText` words. */
const KNOWN_FETCH_CAUSES: ReadonlySet<string> = new Set([
  GIT_REMOTE_ERRORS.noRemoteConfigured,
  GITHUB_TOKEN_ERRORS.tokenMissing,
  GITHUB_TOKEN_ERRORS.selectionRequired,
  GITHUB_TOKEN_ERRORS.invalidToken,
  GITHUB_TOKEN_ERRORS.keychainUnavailable,
  GITHUB_TOKEN_ERRORS.hostMismatch,
  GITHUB_TOKEN_ERRORS.githubUnreachable,
]);

/** GIT-FR-RCDL: what the dialog states once a reconcile returned. */
export function reconcileText(outcome: PullReconcileOutcome): string {
  const tip = (outcome.commit ?? "").slice(0, 7);
  switch (outcome.state) {
    case "merged":
      return `Merged. The branch is at ${tip}.`;
    case "rebased":
      return `Rebased. The branch is at ${tip}.`;
    case "handed_to_run":
      return `Git could not settle every conflict, so the pull run “${outcome.runTitle ?? ""}” settles them. Nothing changes until its review lets the result through.`;
    default:
      return pullErrorText(outcome.error ?? "");
  }
}
