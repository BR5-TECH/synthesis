import { describe, expect, it } from "vitest";
import { parseRejection, rejectionMessage, rejectionPaths } from "./errors";

describe("the typed rejections of the Git panel (GIT-FR-UDKY, GIT-FR-GAMV, GIT-FR-TFAU)", () => {
  it("GIT-FR-UDKY: parses a code, a detail, an Error and a bare code", () => {
    expect(parseRejection("worktree_dirty: a.txt, b.txt")).toEqual({
      code: "worktree_dirty",
      detail: "a.txt, b.txt",
    });
    expect(parseRejection(new Error("Error: stream_unmerged: 3"))).toEqual({
      code: "stream_unmerged",
      detail: "3",
    });
    expect(parseRejection("no remote configured")).toEqual({
      code: "no remote configured",
      detail: null,
    });
    expect(parseRejection(undefined)).toEqual({ code: "", detail: null });
  });

  it("GIT-FR-SIJB: splits the paths a dirty refusal carries", () => {
    expect(rejectionPaths(parseRejection("worktree_dirty: a.txt, b/c.txt"))).toEqual([
      "a.txt",
      "b/c.txt",
    ]);
    expect(rejectionPaths(parseRejection("worktree_dirty"))).toEqual([]);
  });

  it.each([
    ["not a git repository", {}, "This project is not inside a Git repository."],
    ["unknown_commit", {}, "That commit does not exist any more."],
    ["path_not_in_commit", {}, "That commit did not change this file."],
    // GIT-FR-SION / GTC-FR-QVDE: the branch comparison's refusals.
    ["no_comparison_base", { branch: "b" }, "The base branch of b does not exist"],
    ["no_comparison_base", {}, "The base branch of this branch does not exist"],
    ["no_merge_base", { branch: "b" }, "Branch b shares no commit with its base branch"],
    ["path_not_in_comparison", { branch: "b" }, "Branch b did not change this file against its base."],
    ["branch_in_primary_worktree", { branch: "b" }, "Branch b is checked out in the primary worktree."],
    ["branch_in_active_worktree", { branch: "b" }, "Branch b is checked out in the active worktree."],
    ["branch_belongs_to_work_stream: s1", { branch: "b" }, "Branch b belongs to a work stream."],
    ["no remote configured", {}, "This repository has no remote configured."],
    ["not_a_github_remote", {}, "not on github.com"],
    ["github_token_rejected", {}, "GitHub rejected the token"],
    ["github_unreachable", {}, "Could not reach GitHub."],
    ["github_token_missing", {}, "Add one in Global settings → GitHub."],
    ["stream_busy", {}, "A run holds this work stream"],
    ["stream_has_runs", {}, "Runs of this work stream are still working."],
    ["stream_active", {}, "the active one"],
    ["stream_unmerged: 1", {}, "1 commit its base branch does not hold"],
  ])("GIT-FR-UDKY: %s reads as a sentence", (raw, subject, sentence) => {
    expect(rejectionMessage(raw, subject)).toContain(sentence);
  });

  it("GIT-FR-TFAU: a code the panel has no words for is shown whole", () => {
    expect(rejectionMessage("odd_code: with detail: and more")).toBe(
      "odd_code: with detail: and more",
    );
    expect(rejectionMessage("")).toBe("The operation failed.");
  });

  it("AAP-FR-LRTC: a refused certificate names the host and the cause, and is not cut at its first colon", () => {
    const wire = "tls_untrusted:unknown_issuer:api.github.com";
    expect(parseRejection(wire)).toEqual({ code: wire, detail: null });
    for (const rejection of [wire, `Error: ${wire}`, new Error(wire)]) {
      const text = rejectionMessage(rejection, { branch: "feature" });
      expect(text).toContain("api.github.com");
      expect(text).toMatch(/issuer.*unknown/);
    }
  });
});
