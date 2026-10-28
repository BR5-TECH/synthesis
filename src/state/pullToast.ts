/**
 * The toast a pull of the top-chrome pull control shows
 * (`specifications/ui/NTF-notifications.md` NTF-FR-PLQV, NTF-FR-TSRL;
 * `specifications/ui/WTS-worktree-selector.md` WTS-FR-34, WTS-FR-35).
 *
 * The words come from `pullText.ts`, so the toast and the Git panel's Logs line
 * never say a different thing about the same outcome.
 */
import { GITHUB_TOKEN_ERRORS, type PullOutcome } from "../types";
import { logDebug } from "../logging";
import { pullNote } from "./pullText";
import { OPEN_GITHUB_SETTINGS } from "./pushToast";
import { showToast } from "./toasts";

/** NTF-FR-PLQV: the key of the pull toast. A second pull replaces the first. */
export const PULL_RESULT_KEY = "pull-result";

/** NTF-FR-PLQV: the label of the link a missing-token failure carries. */
export { OPEN_GITHUB_SETTINGS };

/** NTF-FR-PLQV: an Error toast that names why the pull did not land. */
export function showPullFailure(
  body: string,
  onOpenGithubSettings?: () => void,
): void {
  logDebug(["frontend", "remote"], "pull result toast", { level: "Error" });
  // The author just acted, so the toast takes the next free slot.
  showToast(
    {
      key: PULL_RESULT_KEY,
      level: "Error",
      title: "Pull failed",
      body,
      address: null,
      link: onOpenGithubSettings
        ? { label: OPEN_GITHUB_SETTINGS, onActivate: onOpenGithubSettings }
        : null,
    },
    { priority: true },
  );
}

/**
 * NTF-FR-PLQV: the toast for a pull that returned. A diverged branch shows
 * none, because the merge-or-rebase dialog answers it (WTS-FR-DVGQ).
 */
export function showPullOutcome(
  outcome: PullOutcome,
  onOpenGithubSettings?: () => void,
): void {
  const body = pullNote(outcome);
  if (body === null) return;
  if (outcome.state === "up_to_date" || outcome.state === "fast_forwarded") {
    logDebug(["frontend", "remote"], "pull result toast", { level: "Info" });
    showToast(
      {
        key: PULL_RESULT_KEY,
        level: "Info",
        title: outcome.state === "up_to_date" ? "Branch up to date" : "Branch pulled",
        body,
        address: null,
      },
      { priority: true },
    );
    return;
  }
  // WTS-FR-35 / GHA-FR-19: no token is stored, so the toast routes to where
  // one is added.
  const missingToken = outcome.error === GITHUB_TOKEN_ERRORS.tokenMissing;
  showPullFailure(body, missingToken ? onOpenGithubSettings : undefined);
}
