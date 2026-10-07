/**
 * The pull request notice
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-ITWJ,
 * CPR-FR-RDJP).
 *
 * What a created pull request leaves behind: its number and a link to its page.
 * It is a status, dismissible, and not one of the main window's floating
 * overlays (SNV-FR-56): it opens no window, closes none, and takes no focus.
 */
import { openUrl } from "@tauri-apps/plugin-opener";

import { logWarn } from "../logging";
import type { CreatedPullRequest } from "../types";
import { Icon } from "./icons";

export function PullRequestNotice({
  notice,
  onDismiss,
}: {
  notice: CreatedPullRequest | null;
  onDismiss: () => void;
}) {
  if (!notice) return null;
  return (
    <div
      className="pr-notice"
      role="status"
      aria-live="polite"
      data-testid="pull-request-notice"
    >
      <span className="pr-notice__text">
        Pull request #{notice.number} created:{" "}
        <a
          className="pr-notice__link"
          href={notice.url}
          data-testid="pull-request-notice-link"
          onClick={(e) => {
            e.preventDefault();
            // CPR-FR-ITWJ: the application's opener, in the system browser.
            void openUrl(notice.url).catch((error: unknown) => {
              logWarn(["frontend", "remote"], "pull request link did not open", {
                number: notice.number,
                error: String(error),
              });
            });
          }}
        >
          Open on GitHub
        </a>
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-label="Dismiss"
        data-testid="pull-request-notice-dismiss"
        onClick={onDismiss}
      >
        <Icon.X size={12} className="icon" />
      </button>
    </div>
  );
}
