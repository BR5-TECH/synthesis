/**
 * What a pull run says about itself
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-HDPQ,
 * GRU-FR-PRNW, GRU-FR-PLFW).
 */

import { describe, expect, it } from "vitest";

import { makeMergeRun } from "../../test/graduationFixtures";
import type { GraduationRun } from "../../types/graduation";
import { blockerStatement } from "./blockers";
import {
  failureStatement,
  isPullRun,
  mergeProvenanceLine,
  mergeResultSentence,
  MERGE_BRANCH_MOVED_STATEMENT,
  PULL_BRANCH_MOVED_STATEMENT,
  PULL_TARGET_CHANGED_STATEMENT,
  pullStrategySentence,
  runTitle,
} from "./merge";

function pullRun(
  state: GraduationRun["state"] = "working",
  strategy: "merge" | "rebase" = "merge",
  over: Partial<GraduationRun> = {},
): GraduationRun {
  const run = makeMergeRun("p1", state, {
    merge: {
      name: "Pull origin/main into main",
      streamBranch: "origin/main",
      baseBranch: "main",
      publication: { kind: "commit", message: "Merge origin/main into main" },
      pull: { branch: "main", upstream: "origin/main", worktreePath: "/dev/acme", strategy },
    },
  });
  return {
    ...run,
    streamId: "",
    streamName: "",
    directTarget: { worktreePath: "/dev/acme", worktreeName: "acme", branch: "main" },
    ...over,
  };
}

describe("a pull run (GRU-FR-HDPQ, GRU-FR-PRNW)", () => {
  it("GRU-FR-HDPQ: its title is its merge name", () => {
    expect(isPullRun(pullRun())).toBe(true);
    expect(isPullRun(makeMergeRun("m1"))).toBe(false);
    expect(runTitle(pullRun())).toBe("Pull origin/main into main");
  });

  it("GRU-FR-PRNW: its provenance names the local branch, the upstream and the worktree", () => {
    expect(mergeProvenanceLine(pullRun())).toBe(
      "Pulls “origin/main” into the local branch “main” in the worktree “acme”.",
    );
  });

  it("GRU-FR-PRNW: it states its strategy in place of the publication", () => {
    expect(pullStrategySentence(pullRun("working", "merge"))).toBe(
      "The result is one merge commit of “origin/main” into “main”.",
    );
    expect(pullStrategySentence(pullRun("working", "rebase"))).toBe(
      "The local commits are replayed onto “origin/main”.",
    );
    expect(pullStrategySentence(makeMergeRun("m1"))).toBeNull();
  });

  it("GRU-FR-PRNW: a completed rebase says the commits were replayed and names the new tip", () => {
    const run = pullRun("completed", "rebase");
    run.merge!.result = { published: "rebased", commit: "e".repeat(40), mergedPaths: ["a.md"] };
    expect(mergeResultSentence(run)).toBe(
      "Rebased: the local commits were replayed onto origin/main. main is at eeeeeee.",
    );
  });
});

describe("a pull run that stopped (GRU-FR-PLFW)", () => {
  it("GRU-FR-PLFW: a moved local branch and a changed target say to pull again", () => {
    const moved = pullRun("failed", "merge", {
      failure: { code: "merge_branch_moved", message: "backend text", retryable: false },
    });
    expect(failureStatement(moved)).toBe(PULL_BRANCH_MOVED_STATEMENT);
    expect(failureStatement(moved)).not.toBe(MERGE_BRANCH_MOVED_STATEMENT);
    const target = pullRun("failed", "merge", {
      failure: { code: "pull_target_changed", message: "backend text", retryable: false },
    });
    expect(failureStatement(target)).toBe(PULL_TARGET_CHANGED_STATEMENT);
  });

  it("GRU-FR-PLFW: an unsettled rebase says nothing was written and a merge pull can take it", () => {
    const run = pullRun("blocked", "rebase", {
      blocker: {
        code: "pull_rebase_unsettled",
        message: "extra.md",
        clearsBy: "Discard the run, then pull again and choose Merge.",
      } as GraduationRun["blocker"],
    });
    const statement = blockerStatement(run) ?? "";
    expect(statement).toContain("The rebase could not reproduce the reconciled result.");
    expect(statement).toContain("choose Merge");
    expect(statement).toContain("Nothing was written");
  });

  it("GRU-FR-PLFW: a dirty side of a pull run names the worktree's uncommitted changes", () => {
    const run = pullRun("blocked", "merge", {
      blocker: {
        code: "merge_dirty_side",
        message: "pull_would_overwrite: a.md",
        clearsBy: "",
      } as GraduationRun["blocker"],
    });
    expect(blockerStatement(run)).toContain(
      "The pull would overwrite uncommitted changes in the worktree",
    );
  });
});
