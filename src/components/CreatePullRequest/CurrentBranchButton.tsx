/**
 * **Create a PR** for the project's current branch
 * (`../../../specifications/ui/CHG-changes.md` CHG-FR-UPFP, CHG-FR-UCRL;
 * `../../../specifications/ui/GIT-git.md` GIT-FR-05).
 *
 * The Changes panel's footer and the Git panel's PRs rail both render it, so
 * the two offer a pull request on the same terms. It starts nothing itself: it
 * opens the shared window with the current branch as its source. A click waits
 * for the stack answer of the branch before it builds the source, and the
 * button is busy while it waits (CHG-FR-JQSD).
 */
import { useId, useRef, useState } from "react";

import {
  createPullRequestAvailability,
  currentBranchSource,
  useCurrentBranchPullRequest,
} from "../../hooks/useCurrentBranchPullRequest";
import { openPullRequestFor, useOpenPullRequestList } from "../../state/openPullRequests";
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
  const { current, stackFor } = useCurrentBranchPullRequest();
  const [opening, setOpening] = useState(false);
  const openingRef = useRef(false);
  const head = current.status === "ready" ? current.head : null;
  // CHG-FR-NKGY / GIT-FR-HMRW: read on mount and when the branch changes.
  const openPulls = useOpenPullRequestList(head, head !== null);
  const reasonId = useId();
  const availability = createPullRequestAvailability(current, openPullRequestFor(openPulls, head));
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
        aria-busy={opening || undefined}
        title={reason ?? undefined}
        onClick={() => {
          if (!available || openingRef.current || current.status !== "ready") return;
          // CHG-FR-JQSD: build the source from the stack answer, not from a
          // read that still runs.
          openingRef.current = true;
          setOpening(true);
          void stackFor(current.head).then((stack) => {
            openingRef.current = false;
            setOpening(false);
            const source = currentBranchSource(current, stack);
            if (source) onCreatePullRequest?.(source);
          });
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
