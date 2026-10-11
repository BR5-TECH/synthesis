/**
 * **Create a PR** for the project's current branch
 * (`../../../specifications/ui/CHG-changes.md` CHG-FR-UPFP, CHG-FR-UCRL;
 * `../../../specifications/ui/GIT-git.md` GIT-FR-05).
 *
 * The Changes panel's footer and the Git panel's PRs rail both render it, so
 * the two offer a pull request on the same terms. It starts nothing itself: it
 * opens the shared window with the current branch as its source.
 */
import { useId } from "react";

import {
  createPullRequestAvailability,
  currentBranchSource,
  useCurrentBranchPullRequest,
} from "../../hooks/useCurrentBranchPullRequest";
import { Icon } from "../icons";
import type { PullRequestSource } from "./types";

export function CurrentBranchButton({
  label,
  className,
  withIcon = false,
  extraReason = null,
  testId,
  onCreatePullRequest,
}: {
  label: string;
  className: string;
  withIcon?: boolean;
  /** A reason of the host surface that makes the button unavailable. */
  extraReason?: string | null;
  testId: string;
  onCreatePullRequest?: (source: PullRequestSource) => void;
}) {
  const { current } = useCurrentBranchPullRequest();
  const reasonId = useId();
  const availability = createPullRequestAvailability(current);
  const reason = extraReason ?? availability.reason;
  const available = reason === null && availability.available;

  return (
    <>
      <button
        type="button"
        className={`${className} create-pr-button`}
        data-testid={testId}
        data-state={available ? "available" : "unavailable"}
        // CHG-FR-UCRL: an unavailable button stays reachable by keyboard and
        // starts nothing, so it is not `disabled`.
        aria-disabled={!available}
        aria-describedby={reason ? reasonId : undefined}
        title={reason ?? undefined}
        onClick={() => {
          if (!available) return;
          const source = currentBranchSource(current);
          if (source) onCreatePullRequest?.(source);
        }}
      >
        {withIcon && <Icon.GitPull size={12} />} {label}
      </button>
      {reason && (
        <span id={reasonId} className="sr-only">
          {reason}
        </span>
      )}
    </>
  );
}
