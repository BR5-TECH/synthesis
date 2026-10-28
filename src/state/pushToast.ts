/**
 * The toast a push shows when it ends
 * (`specifications/ui/NTF-notifications.md` NTF-FR-PSHT, NTF-FR-TSRL;
 * `specifications/ui/GIT-git.md` GIT-FR-IMSH;
 * `specifications/ui/CHG-changes.md` CHG-FR-66).
 *
 * The top-chrome **Push** control and the push half of a graduation's
 * **Commit & Push** use it. It writes no log: no push failure goes to the
 * diagnostic Logs panel from the main window (GIT-FR-IMSH).
 */
import { showToast } from "./toasts";

/** NTF-FR-PSHT: the key of the push toast. A second push replaces the first. */
export const PUSH_RESULT_KEY = "push-result";

/** NTF-FR-PSHT: the label of the link a missing-token failure carries. */
export const OPEN_GITHUB_SETTINGS = "Open GitHub settings";

/** NTF-FR-PSHT: the bodies of the causes the toast names in words. */
export const PUSH_NO_TOKEN_SELECTED = "No GitHub token was selected, so nothing was pushed.";
export const PUSH_TOKEN_MISSING = "Pushing to the remote needs a GitHub token.";

/** NTF-FR-PSHT: an Info toast for a push that succeeded. */
export function showPushSucceeded(): void {
  // The author just acted, so the toast takes the next free slot.
  showToast(
    {
      key: PUSH_RESULT_KEY,
      level: "Info",
      title: "Branch pushed",
      body: "The commits of the branch are on the remote.",
      address: null,
    },
    { priority: true },
  );
}

/**
 * NTF-FR-PSHT: an Error toast that names why the push failed. A route to the
 * GitHub settings is given only for a missing token.
 */
export function showPushFailure(body: string, onOpenGithubSettings?: () => void): void {
  showToast(
    {
      key: PUSH_RESULT_KEY,
      level: "Error",
      title: "Push failed",
      body,
      address: null,
      link: onOpenGithubSettings
        ? { label: OPEN_GITHUB_SETTINGS, onActivate: onOpenGithubSettings }
        : null,
    },
    { priority: true },
  );
}
