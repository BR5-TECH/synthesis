/**
 * The toast a created pull request shows
 * (`specifications/ui/NTF-notifications.md` NTF-FR-PRCK;
 * `specifications/ui/CPR-create-pull-request.md` CPR-FR-ITWJ, CPR-FR-RDJP,
 * CPR-FR-YDLV; NTF-FR-SXLR).
 *
 * It is a toast of the one stack, not a notice of the shell (NTF-FR-TSRL). A
 * change of the project removes it with every other toast (NTF-FR-15).
 */
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  linkSentences,
  type PullRequestLinkResult,
} from "../components/CreatePullRequest/linking";
import { stackStepSentence } from "../components/CreatePullRequest/stackMessages";
import { logWarn } from "../logging";
import type { CreatedPullRequest, StackStep } from "../types";
import { showToast } from "./toasts";

/** NTF-FR-PRCK: the key of the toast. A second one replaces the first. */
export const PULL_REQUEST_CREATED_KEY = "pull-request-created";

/**
 * NTF-FR-PRCK: an Info toast with a link to the page of the pull request.
 * CPR-FR-UXXF: it also names the issues the pull request is linked to, and
 * how, and each issue whose link failed. NTF-FR-SXLR: a pull request submitted
 * to a stack also says whether it was added to the stack. A pull request not
 * added still shows an Info toast (CPR-FR-YDLV).
 */
export function showPullRequestCreated(
  pr: CreatedPullRequest,
  link: PullRequestLinkResult | null = null,
  stack: StackStep | null = null,
): void {
  const stackSentence = stackStepSentence(stack);
  showToast(
    {
      key: PULL_REQUEST_CREATED_KEY,
      level: "Info",
      title: "Pull request created",
      body: [
        `Pull request #${pr.number} is created.`,
        ...(stackSentence ? [stackSentence] : []),
        ...linkSentences(link, pr.url),
      ].join(" "),
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
