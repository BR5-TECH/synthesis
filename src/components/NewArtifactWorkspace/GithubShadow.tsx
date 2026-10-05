/**
 * What the New Artifact tab adds for a GitHub-shadow draft
 * (`NAW-new-artifact.md` NAW-FR-UEWC, NAW-FR-AXFQ, NAW-FR-BCHZ): the banner that
 * says why the prompt cannot be edited, the tag that names the issue and opens
 * it, and the reasons the disabled actions state.
 */
import { useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import { githubPollingErrorMessage, refusalCode } from "../../state/githubPolling";
import type { GithubIssueLink } from "../../types";
import { Icon } from "../icons";

/** NAW-FR-AXFQ: why each action other than Graduate is disabled. */
export const SHADOW_DISCUSS_REASON =
  "This draft mirrors a GitHub issue, so it cannot be discussed here";
export const SHADOW_PUBLISH_REASON =
  "This draft already mirrors a GitHub issue, so it cannot be published";
export const SHADOW_ARCHIVE_REASON =
  "This draft mirrors a GitHub issue, so it cannot be archived or restored";

const issueLabel = (issue: GithubIssueLink) =>
  `${issue.repositoryOwner}/${issue.repositoryName}#${issue.issueNumber}`;

/** NAW-FR-UEWC: the banner, in words. */
export function GithubShadowBanner({ issue }: { issue: GithubIssueLink | null }) {
  return (
    <div
      className="draft-workspace__locked t-ui-xs"
      role="note"
      data-testid="draft-github-shadow-banner"
    >
      This draft mirrors the GitHub issue
      {issue ? ` ${issueLabel(issue)}` : ""} and cannot be edited. Graduate it
      to start work on the issue.
    </div>
  );
}

/**
 * NAW-FR-BCHZ: the tag naming the repository and the issue number. Activating
 * it opens the issue in the default browser through the publication
 * operation, which admits a shadow draft's issue URL.
 */
export function GithubShadowTag({
  draftId,
  issue,
}: {
  draftId: string;
  issue: GithubIssueLink;
}) {
  const [error, setError] = useState<string | null>(null);
  const open = () => {
    setError(null);
    api.openPublicationIssue(draftId, issue.issueUrl).catch((e) => {
      logWarn(["frontend"], "shadow draft issue could not be opened", {
        draftId,
        code: refusalCode(e),
      });
      setError(githubPollingErrorMessage(e));
    });
  };
  return (
    <>
      <button
        type="button"
        className="draft-workspace__graduation t-ui-xs"
        data-testid="draft-github-shadow-tag"
        title={`Open GitHub issue ${issueLabel(issue)}`}
        aria-label={`Open GitHub issue ${issueLabel(issue)}`}
        onClick={open}
      >
        <Icon.GitPull size={11} /> {issueLabel(issue)}
      </button>
      {error && (
        <span className="t-ui-xs draft-workspace__name-error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}
