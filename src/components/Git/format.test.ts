import { describe, expect, it } from "vitest";
import { groupFilesByFolder, summarizePaths } from "./format";
import type { CommitFile } from "../../types";

const file = (path: string): CommitFile => ({ path, status: "modified", isBinary: false });

describe("the commit file view model (GIT-FR-DXNC, GIT-FR-SIJB)", () => {
  it("GIT-FR-DXNC: groups the flat list by folder, root first, folders and files in order", () => {
    const groups = groupFilesByFolder([
      file("z/b.ts"),
      file("README.md"),
      file("a/deep/x.ts"),
      file("z/a.ts"),
      file("LICENSE"),
    ]);
    expect(groups.map((g) => g.label)).toEqual(["Repository root", "a/deep", "z"]);
    expect(groups[0].files.map((f) => f.name)).toEqual(["LICENSE", "README.md"]);
    expect(groups[2].files.map((f) => f.name)).toEqual(["a.ts", "b.ts"]);
    expect(groups[1].folder).toBe("a/deep");
  });

  it("GIT-FR-DXNC: a commit with no files has no groups", () => {
    expect(groupFilesByFolder([])).toEqual([]);
  });

  it("GIT-FR-SIJB: lists the first paths and counts the rest", () => {
    expect(summarizePaths(["a", "b", "c"], 2)).toEqual({ listed: ["a", "b"], remaining: 1 });
    expect(summarizePaths(["a"], 5)).toEqual({ listed: ["a"], remaining: 0 });
  });
});
