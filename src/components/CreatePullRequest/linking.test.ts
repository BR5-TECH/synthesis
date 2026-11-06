/**
 * Which issues a created pull request is linked to, and how the toast says it
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-BLEN,
 * CPR-FR-UXXF).
 */
import { describe, expect, it } from "vitest";

import {
  issueRefText,
  issuesToLink,
  linkFailureOf,
  linkResultOf,
  linkSentences,
  repositoryOfPullRequestUrl,
} from "./linking";

const issue = (owner: string, repository: string, number: number) => ({ owner, repository, number });
const task = (n: number, owner = "acme", repository = "platform") => ({
  draftId: `d${n}`,
  draftName: `Draft ${n}`,
  issue: issue(owner, repository, n),
});
const feature = (n: number) => ({ ...issue("acme", "platform", n), title: `Feature ${n}` });
const URL_42 = "https://github.com/acme/platform/pull/42";

describe("the issues to link (CPR-FR-BLEN)", () => {
  it("CPR-FR-BLEN: the tasks come first, then the selected features, each issue once", () => {
    expect(issuesToLink([task(31), task(32)], [feature(12), feature(31)])).toEqual([
      issue("acme", "platform", 31),
      issue("acme", "platform", 32),
      issue("acme", "platform", 12),
    ]);
  });

  it("CPR-FR-BLEN: an issue named in two cases is still one issue", () => {
    expect(issuesToLink([task(5, "ACME", "Platform")], [feature(5)])).toHaveLength(1);
  });

  it("CPR-FR-BLEN: a link that fails as a whole fails every issue and knows nothing of the base", () => {
    expect(linkFailureOf([issue("a", "b", 1)])).toEqual({
      linked: [],
      failed: [issue("a", "b", 1)],
      closesOnMerge: null,
    });
  });
});

describe("the toast sentences (CPR-FR-UXXF)", () => {
  it("CPR-FR-UXXF: a linked issue of the pull request's repository reads #N, another reads owner/repo#N", () => {
    const repo = repositoryOfPullRequestUrl(URL_42);
    expect(repo).toEqual({ owner: "acme", repository: "platform" });
    expect(issueRefText(issue("acme", "platform", 12), repo)).toBe("#12");
    expect(issueRefText(issue("other", "tracker", 12), repo)).toBe("other/tracker#12");
    expect(repositoryOfPullRequestUrl("not a url")).toBeNull();
  });

  it("CPR-FR-UXXF: names the linked issues with their way, the failed ones, and that the issues stay open", () => {
    const result = linkResultOf({
      linked: [{ issue: issue("acme", "platform", 12), way: "closing_keyword" }],
      failed: [{ issue: issue("acme", "platform", 9), error: "github_unreachable" }],
      closesOnMerge: false,
    });
    expect(linkSentences(result, URL_42)).toEqual([
      "Linked by closing keyword: #12.",
      "Not linked: #9.",
      "The issues stay open when the pull request merges, because it does not merge into the default branch.",
    ]);
  });

  it("CPR-FR-UXXF: no link says nothing, and a whole failure does not guess at the base", () => {
    expect(linkSentences(null, URL_42)).toEqual([]);
    expect(linkSentences(linkFailureOf([issue("acme", "platform", 3)]), URL_42)).toEqual([
      "Not linked: #3.",
    ]);
  });
});
