/**
 * The toast a created pull request shows
 * (`specifications/ui/NTF-notifications.md` NTF-FR-PRCK;
 * `specifications/ui/CPR-create-pull-request.md` CPR-FR-ITWJ, CPR-FR-RDJP).
 *
 * It is a toast of the one stack, not a notice of the shell (NTF-FR-TSRL). A
 * change of the project removes it with every other toast (NTF-FR-15).
 */
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  linkSentences,
  type PullRequestLinkResult,
} from "../components/CreatePullRequest/linking";
import { logWarn } from "../logging";
import type { CreatedPullRequest } from "../types";
import { showToast } from "./toasts";

/** NTF-FR-PRCK: the key of the toast. A second one replaces the first. */
export const PULL_REQUEST_CREATED_KEY = "pull-request-created";

/**
 * NTF-FR-PRCK: an Info toast with a link to the page of the pull request.
 * CPR-FR-UXXF: it also names the issues the pull request is linked to, and
 * how, and each issue whose link failed.
 */
export function showPullRequestCreated(
  pr: CreatedPullRequest,
  link: PullRequestLinkResult | null = null,
): void {
  showToast(
    {
      key: PULL_REQUEST_CREATED_KEY,
      level: "Info",
      title: "Pull request created",
      body: [`Pull request #${pr.number} is created.`, ...linkSentences(link, pr.url)].join(" "),
      address: null,
      link: {
        label: `Open pull request #${pr.number}`,
        // CPR-FR-ITWJ: the application's opener, in the system browser.
        onActivate: () => {
          void openUrl(pr.url).catch((error: unknown) => {
            logWarn(["frontend", "remote"], "pull request link did not open", {
              number: pr.number,
              error: String(error),
            });
          });
        },
      },
    },
    { priority: true },
  );
}
