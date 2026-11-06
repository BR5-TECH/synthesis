/**
 * Which issues a created pull request is linked to, and how the toast says it
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-BLEN,
 * CPR-FR-UXXF). Pure functions, so the wording is tested without a render.
 */
import type { PullRequestFeature } from "../../state/pullRequestSessions";
import type { PullRequestIssueRef, PullRequestLinkOutcome } from "../../types";

/** CPR-FR-IDRP: one task: the issue a draft of the branch was published as. */
export interface PullRequestTask {
  draftId: string;
  draftName: string;
  issue: PullRequestIssueRef;
}

/** CPR-FR-UXXF: what the link did, as the toast tells it. */
export interface PullRequestLinkResult {
  linked: PullRequestIssueRef[];
  failed: PullRequestIssueRef[];
  /** Null when the link failed as a whole, so nothing is known of the base. */
  closesOnMerge: boolean | null;
}

/** Two references name the same issue. GitHub names are case-insensitive. */
export const sameIssue = (a: PullRequestIssueRef, b: PullRequestIssueRef) =>
  a.number === b.number &&
  a.owner.toLowerCase() === b.owner.toLowerCase() &&
  a.repository.toLowerCase() === b.repository.toLowerCase();

/** CPR-FR-BLEN: the tasks, then the selected features, each issue once. */
export function issuesToLink(
  tasks: readonly PullRequestTask[],
  features: readonly PullRequestFeature[],
): PullRequestIssueRef[] {
  const out: PullRequestIssueRef[] = [];
  const all = [
    ...tasks.map((t) => t.issue),
    ...features.map(({ owner, repository, number }) => ({ owner, repository, number })),
  ];
  for (const issue of all) {
    if (!out.some((seen) => sameIssue(seen, issue))) out.push(issue);
  }
  return out;
}

/** CPR-FR-BLEN: the answer of the link operation, as the toast reads it. */
export function linkResultOf(outcome: PullRequestLinkOutcome): PullRequestLinkResult {
  return {
    linked: outcome.linked.map((l) => l.issue),
    failed: outcome.failed.map((f) => f.issue),
    closesOnMerge: outcome.closesOnMerge,
  };
}

/** CPR-FR-BLEN: a link that failed as a whole fails each issue. */
export function linkFailureOf(issues: PullRequestIssueRef[]): PullRequestLinkResult {
  return { linked: [], failed: issues, closesOnMerge: null };
}

/** The repository of a pull request page `https://<host>/<owner>/<repo>/pull/<n>`. */
export function repositoryOfPullRequestUrl(
  url: string,
): { owner: string; repository: string } | null {
  try {
    const parts = new URL(url).pathname.split("/").filter(Boolean);
    if (parts.length < 2) return null;
    return { owner: parts[0], repository: parts[1] };
  } catch {
    return null;
  }
}

/** `#N` in the repository of the pull request, else `owner/repository#N`. */
export function issueRefText(
  issue: PullRequestIssueRef,
  repository: { owner: string; repository: string } | null,
): string {
  const own =
    repository !== null &&
    issue.owner.toLowerCase() === repository.owner.toLowerCase() &&
    issue.repository.toLowerCase() === repository.repository.toLowerCase();
  return own ? `#${issue.number}` : `${issue.owner}/${issue.repository}#${issue.number}`;
}

/**
 * CPR-FR-UXXF: the sentences the toast adds about the links: each linked
 * issue and its way, each issue whose link failed, and that the issues stay
 * open when `closesOnMerge` is false.
 */
export function linkSentences(result: PullRequestLinkResult | null, url: string): string[] {
  if (!result) return [];
  const repository = repositoryOfPullRequestUrl(url);
  const refs = (issues: PullRequestIssueRef[]) =>
    issues.map((issue) => issueRefText(issue, repository)).join(", ");
  const sentences: string[] = [];
  if (result.linked.length > 0) {
    sentences.push(`Linked by closing keyword: ${refs(result.linked)}.`);
  }
  if (result.failed.length > 0) {
    sentences.push(`Not linked: ${refs(result.failed)}.`);
  }
  if (result.closesOnMerge === false && result.linked.length > 0) {
    sentences.push(
      "The issues stay open when the pull request merges, because it does not merge into the default branch.",
    );
  }
  return sentences;
}
