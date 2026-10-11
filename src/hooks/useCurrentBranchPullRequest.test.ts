import { describe, expect, it } from "vitest";

import {
  createPullRequestAvailability,
  currentBranchSource,
  type CurrentBranchPullRequest,
} from "./useCurrentBranchPullRequest";

const ready = (aheadOfBase: number, streamName: string | null = null): CurrentBranchPullRequest => ({
  status: "ready",
  head: "feature",
  streamName,
  state: {
    head: "feature",
    base: "main",
    hasRemote: true,
    remoteBranchExists: false,
    unpushed: null,
    uncommittedPaths: [],
    aheadOfBase,
  },
});

describe("Create a PR for the current branch", () => {
  it("CHG-FR-UCRL: is available only for a branch holding a commit the default branch lacks", () => {
    expect(createPullRequestAvailability(ready(1))).toEqual({ available: true, reason: null });
    expect(createPullRequestAvailability(ready(0)).reason).toBe(
      "This branch has no commit of its own yet.",
    );
  });

  it("CHG-FR-UCRL: a read in progress, a detached HEAD and a failed read each state their reason", () => {
    expect(createPullRequestAvailability({ status: "reading" }).reason).toBe("Reading the branch…");
    expect(createPullRequestAvailability({ status: "detached" }).reason).toBe(
      "HEAD is detached, so there is no branch to propose.",
    );
    expect(createPullRequestAvailability({ status: "failed" }).available).toBe(false);
  });

  it("CHG-FR-UPFP: the title is the stream's name where the branch belongs to a stream, and the branch name otherwise", () => {
    expect(currentBranchSource(ready(1, "editor work"))).toEqual({
      head: "feature",
      base: "main",
      title: "editor work",
    });
    expect(currentBranchSource(ready(1))?.title).toBe("feature");
    expect(currentBranchSource({ status: "reading" })).toBeNull();
  });
});
