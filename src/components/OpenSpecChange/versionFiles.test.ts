/**
 * The files of a version, by artifact and against the current change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-RKCC,
 * OCW-FR-DRQD, OCW-FR-ABAR).
 */
import { describe, expect, it } from "vitest";

import { demoChange } from "./fixtures";
import {
  OTHER_FILES,
  changedFileCount,
  compareWithCurrent,
  shortCommit,
  versionArtifacts,
} from "./versionFiles";

describe("versionFiles", () => {
  it("OCW-FR-RKCC: the count of files a version changed sums its added, modified, and deleted paths, and the short id is 7 characters", () => {
    expect(
      changedFileCount({ commitId: "x", summary: "", authorName: "", committedAt: "", added: ["a"], modified: ["b", "c"], deleted: ["d"] }),
    ).toBe(4);
    expect(shortCommit("0123456789abcdef")).toBe("0123456");
  });

  it("OCW-FR-DRQD: the files of a version stand under the artifacts of the schema, and a file no artifact claims stands last", () => {
    const groups = versionArtifacts(demoChange(), [
      { path: "notes.txt", text: "n", bytes: 1, binary: false },
      { path: "proposal.md", text: "p", bytes: 1, binary: false },
      { path: "specs/new/spec.md", text: "s", bytes: 1, binary: false },
      { path: "tasks.md", text: "t", bytes: 1, binary: false },
    ]);
    expect(groups.map((g) => [g.id, g.files.map((f) => f.path)])).toEqual([
      ["proposal", ["proposal.md"]],
      ["specs", ["specs/new/spec.md"]],
      ["design", []],
      ["tasks", ["tasks.md"]],
      [OTHER_FILES, ["notes.txt"]],
    ]);
    expect(versionArtifacts(demoChange(), []).map((g) => g.id)).not.toContain(OTHER_FILES);
  });

  it("OCW-FR-ABAR: each file is marked added, removed, changed, unchanged, or not text, ordered by path", () => {
    const detail = demoChange();
    detail.artifacts[1].files[0].text = "same";
    const compared = compareWithCurrent(
      [
        { path: "proposal.md", text: "old", bytes: 3, binary: false },
        { path: "specs/auth/spec.md", text: "same", bytes: 4, binary: false },
        { path: "design.md", text: "was text", bytes: 8, binary: false },
        { path: "gone.md", text: "a\nb\n", bytes: 4, binary: false },
      ],
      detail,
    );
    const by = Object.fromEntries(compared.map((f) => [f.path, f]));
    expect(compared.map((f) => f.path)).toEqual([
      "design.md",
      "gone.md",
      "proposal.md",
      "specs/auth/spec.md",
      "specs/ui/spec.md",
    ]);
    expect(by["proposal.md"].mark).toBe("changed");
    expect(by["specs/auth/spec.md"].mark).toBe("unchanged");
    expect(by["specs/ui/spec.md"].mark).toBe("added");
    expect(by["gone.md"].mark).toBe("removed");
    expect(by["gone.md"].rows.map((r) => [r.old?.content ?? null, r.new?.content ?? null])).toEqual([
      ["a", null],
      ["b", null],
    ]);
    // design.md is not text in the current change, so it is named with no diff.
    expect(by["design.md"]).toMatchObject({ mark: "notText", notText: true, rows: [] });
  });
});
