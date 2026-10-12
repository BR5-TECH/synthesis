import { describe, expect, it } from "vitest";

import { baseBranchNames } from "./baseBranches";
import {
  pullRequestBlocks,
  pullRequestFailureMessage,
  pullRequestHeadReadMessage,
  pullRequestPushNotice,
  pushFailureMessage,
} from "./messages";
import type { PullRequestHeadState } from "../../types";

const STATE: PullRequestHeadState = {
  head: "feature",
  base: "main",
  hasRemote: true,
  remoteBranchExists: true,
  unpushed: 0,
  uncommittedPaths: [],
  aheadOfBase: 1,
};

describe("the words of the Create a PR window", () => {
  it("CPR-FR-VYPG: a clean branch holds no block", () => {
    expect(pullRequestBlocks(STATE)).toEqual([]);
  });

  it("CPR-FR-VYPG, CPR-FR-PLWK: a branch the remote lacks is no block, and one file reads in the singular", () => {
    expect(pullRequestBlocks({ ...STATE, remoteBranchExists: false, unpushed: null })).toEqual([]);
    expect(pullRequestBlocks({ ...STATE, unpushed: 5 })).toEqual([]);
    const blocks = pullRequestBlocks({ ...STATE, unpushed: 1, uncommittedPaths: ["a.md"] });
    expect(blocks.map((b) => b.key)).toEqual(["uncommitted"]);
    expect(blocks[0].text).toBe("1 uncommitted file (a.md). Commit first.");
  });

  it("CPR-FR-VYPG, CPR-FR-PLWK: no remote is a block and holds no push notice", () => {
    const state = { ...STATE, hasRemote: false, remoteBranchExists: false, unpushed: null };
    expect(pullRequestBlocks(state).map((b) => b.key)).toEqual(["no-remote"]);
    expect(pullRequestPushNotice(state)).toBeNull();
  });

  it("CPR-FR-PLWK: the notice says which case applies, and how many commits are missing", () => {
    expect(pullRequestPushNotice(STATE)).toBeNull();
    expect(pullRequestPushNotice({ ...STATE, unpushed: null })).toBeNull();
    expect(pullRequestPushNotice({ ...STATE, remoteBranchExists: false, unpushed: null })).toEqual({
      key: "not-pushed",
      text: "The branch is not on the remote yet. Submit pushes it first.",
    });
    expect(pullRequestPushNotice({ ...STATE, unpushed: 1 })).toEqual({
      key: "unpushed",
      text: "1 commit is not on the remote yet. Submit pushes it first.",
    });
    expect(pullRequestPushNotice({ ...STATE, unpushed: 3 })?.text).toBe(
      "3 commits are not on the remote yet. Submit pushes them first.",
    );
  });

  it("CPR-FR-VZNE: a push failure reads as a sentence, and the remote's own refusal is named as one", () => {
    expect(pushFailureMessage("unknown branch", "feature")).toBe(
      "This repository holds no branch feature, so it was not pushed.",
    );
    expect(pushFailureMessage("no remote configured", "feature")).toBe(
      "This repository has no remote configured.",
    );
    expect(pushFailureMessage("github_unreachable", "feature")).toMatch(/^Could not reach GitHub/);
    expect(pushFailureMessage("github_token_rejected", "feature")).toMatch(/rejected the token/);
    expect(pushFailureMessage("cannot push non-fastforwardable reference", "feature")).toBe(
      "The remote refused the push of feature: cannot push non-fastforwardable reference",
    );
  });

  it("CPR-FR-VZNE: a failure reads as a sentence, and an unknown reason is shown whole", () => {
    expect(pullRequestFailureMessage("pull_request_title_required")).toBe(
      "A pull request needs a title.",
    );
    expect(pullRequestFailureMessage("pull_request_rejected")).toBe(
      "GitHub refused the pull request.",
    );
    expect(pullRequestFailureMessage("something_else: detail")).toBe(
      "something_else: detail",
    );
  });

  it("GHA-FR-LBLM: a host mismatch reads as a sentence", () => {
    expect(pullRequestFailureMessage("github_host_mismatch")).toBe(
      "The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.",
    );
  });

  it("CPR-FR-SQGZ: a branch the local read does not find is the repository's, not GitHub's", () => {
    expect(pullRequestHeadReadMessage("unknown branch", "feature", "gone")).toBe(
      "This repository holds no branch feature, or no branch gone.",
    );
    expect(pullRequestHeadReadMessage("github_unreachable", "a", "b")).toMatch(/Could not reach/);
  });

  it("CPR-FR-FZHF: base names drop the remote, the head, HEAD and duplicates", () => {
    expect(
      baseBranchNames(
        [
          { name: "main", kind: "local", isCurrent: true },
          { name: "origin/main", kind: "remote", isCurrent: false },
          { name: "origin/HEAD", kind: "remote", isCurrent: false },
          { name: "origin/synthesis/stream/x", kind: "remote", isCurrent: false },
          { name: "feature", kind: "local", isCurrent: false },
        ],
        "feature",
      ),
    ).toEqual(["main", "synthesis/stream/x"]);
  });
});
