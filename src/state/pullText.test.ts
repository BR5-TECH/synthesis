/**
 * The words a pull is reported in (`../../specifications/ui/WTS-worktree-selector.md`
 * WTS-FR-34, `../../specifications/ui/GIT-git.md` GIT-FR-PLWD, GIT-FR-RCDL).
 */

import { describe, expect, it } from "vitest";

import { GITHUB_TOKEN_ERRORS, type PullOutcome } from "../types";
import { pullErrorText, pullNote, reconcileText } from "./pullText";

const outcome = (over: Partial<PullOutcome>): PullOutcome => ({
  state: "up_to_date",
  branch: "main",
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  updatedPaths: [],
  refresh: {
    context: { repositoryRoot: "/r", activeWorktreePath: "/r", worktrees: [], branches: [] },
    remoteState: "refreshed",
  },
  ...over,
});

describe("the note a pull leaves (WTS-FR-34)", () => {
  it("WTS-FR-34: names an up-to-date branch and the commits a fast-forward brought", () => {
    expect(pullNote(outcome({}))).toBe("main is up to date with origin/main.");
    expect(pullNote(outcome({ state: "fast_forwarded", behind: 1 }))).toBe(
      "Pulled 1 commit into main.",
    );
    expect(pullNote(outcome({ state: "fast_forwarded", behind: 4 }))).toBe(
      "Pulled 4 commits into main.",
    );
  });

  it("WTS-FR-34, WTS-FR-DVGQ: a diverged pull leaves no note, because the dialog opens", () => {
    expect(pullNote(outcome({ state: "diverged", ahead: 1, behind: 1 }))).toBeNull();
  });

  it("WTS-FR-34: distinguishes the causes the author acts on differently", () => {
    const failed = (error: string) => pullNote(outcome({ state: "fetch_failed", error }));
    expect(failed(GITHUB_TOKEN_ERRORS.githubUnreachable)).toMatch(/couldn't reach the remote/i);
    expect(failed(GITHUB_TOKEN_ERRORS.invalidToken)).toMatch(/refused the credential/i);
    expect(failed(GITHUB_TOKEN_ERRORS.tokenMissing)).toMatch(/needs a GitHub token/);
    expect(failed("something_new")).toBe("The remote could not be fetched — nothing was pulled.");
    const refused = (error: string) => pullNote(outcome({ state: "refused", error }));
    expect(refused("no remote configured")).toMatch(/No remote is configured/);
    expect(refused("no_upstream")).toMatch(/tracks no remote branch/);
    expect(refused("upstream_gone")).toMatch(/is gone/);
    expect(refused("upstream_not_on_primary_remote")).toMatch(/another remote/);
    expect(refused("no branch is checked out")).toMatch(/No branch is checked out/);
  });

  it("WTS-FR-34: names the paths a pull would overwrite and the work that holds the worktree", () => {
    expect(pullErrorText("pull_would_overwrite: a.md, b/c.md")).toBe(
      "The pull would overwrite uncommitted changes in a.md, b/c.md. Commit or discard them, then pull again.",
    );
    expect(pullErrorText("pull_in_progress")).toMatch(/Another pull is running/);
    expect(pullErrorText("merge_in_progress")).toMatch(/merge or update is running/);
    expect(pullErrorText("pull_run_active: run-1")).toMatch(/pull run is already reconciling/);
    expect(pullErrorText("direct graduation active: run-2")).toMatch(/graduation run is working/);
    expect(pullErrorText("work stream busy: s, r")).toMatch(/work stream/);
  });
});

describe("what a reconcile did (GIT-FR-RCDL)", () => {
  it("GIT-FR-RCDL: states the new tip, the run, or the refusal", () => {
    expect(reconcileText({ state: "merged", commit: "a".repeat(40), updatedPaths: [] })).toBe(
      "Merged. The branch is at aaaaaaa.",
    );
    expect(reconcileText({ state: "rebased", commit: "b".repeat(40), updatedPaths: [] })).toBe(
      "Rebased. The branch is at bbbbbbb.",
    );
    expect(
      reconcileText({ state: "handed_to_run", runTitle: "Pull origin/main into main", updatedPaths: [] }),
    ).toMatch(/“Pull origin\/main into main” settles them/);
    expect(
      reconcileText({ state: "refused", error: "pull_tips_moved", updatedPaths: [] }),
    ).toBe("The branch or its upstream moved since the pull. Pull again.");
  });
});
