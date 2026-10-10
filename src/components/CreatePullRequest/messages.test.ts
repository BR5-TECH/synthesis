import { describe, expect, it } from "vitest";

import { baseBranchNames } from "./baseBranches";
import {
  pullRequestBlocks,
  pullRequestFailureMessage,
  pullRequestHeadReadMessage,
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

  it("CPR-FR-VYPG: commit comes before push, and one commit reads in the singular", () => {
    const blocks = pullRequestBlocks({
      ...STATE,
      unpushed: 1,
      uncommittedPaths: ["a.md"],
    });
    expect(blocks.map((b) => b.key)).toEqual(["uncommitted", "unpushed"]);
    expect(blocks[0].text).toBe("1 uncommitted file (a.md). Commit first.");
    expect(blocks[1].text).toBe("1 commit is not on the remote. Push first.");
  });

  it("CPR-FR-VYPG: no remote does not also say the branch is not on the remote", () => {
    const blocks = pullRequestBlocks({
      ...STATE,
      hasRemote: false,
      remoteBranchExists: false,
      unpushed: null,
    });
    expect(blocks.map((b) => b.key)).toEqual(["no-remote"]);
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
