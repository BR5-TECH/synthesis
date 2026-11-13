import { describe, expect, it } from "vitest";

import { changeDetail, changeProposal, NEW_PATH } from "../../test/changeWorkspaceFixtures";
import {
  artifactOfPath,
  modeOf,
  pathsWithHunks,
  shownFiles,
  undecidedByArtifact,
} from "./changeFiles";

describe("where the hunks of a proposal fall", () => {
  it("OCW-FR-RRXG: a path belongs to the artifact that lists it, or to the artifact of its folder or its name", () => {
    const detail = changeDetail();
    expect(artifactOfPath(detail, "proposal.md")).toBe("proposal");
    expect(artifactOfPath(detail, "specs/auth/spec.md")).toBe("specs");
    expect(artifactOfPath(detail, NEW_PATH)).toBe("specs");
    expect(artifactOfPath(detail, "tasks.md")).toBe("tasks");
    expect(artifactOfPath(detail, "notes/extra.md")).toBeNull();
  });

  it("OCW-FR-RRXG: counts the undecided hunks of each artifact, a held one included, and of a pending proposal alone", () => {
    const detail = changeDetail();
    expect(undecidedByArtifact(detail, changeProposal(["discussing", "pending"]))).toEqual(
      new Map([
        ["proposal", 1],
        ["specs", 1],
      ]),
    );
    expect(undecidedByArtifact(detail, changeProposal(["accepted", "rejected"])).size).toBe(0);
    expect(
      undecidedByArtifact(detail, changeProposal(["pending", "pending"], { state: "accepted" })).size,
    ).toBe(0);
    expect(undecidedByArtifact(detail, undefined).size).toBe(0);
  });

  it("OCW-FR-RRXG: a new file shows after the files of its artifact, marked new, while its hunk is undecided", () => {
    const detail = changeDetail();
    const specs = detail.artifacts[1];
    const shown = shownFiles(detail, specs, changeProposal());
    expect(shown.map((f) => [f.path, f.isNew])).toEqual([
      ["specs/auth/spec.md", false],
      ["specs/ui/spec.md", false],
      [NEW_PATH, true],
    ]);
    expect(shownFiles(detail, specs, changeProposal(["pending", "rejected"]))).toHaveLength(2);
    expect(shownFiles(detail, detail.artifacts[0], changeProposal())).toHaveLength(1);
  });

  it("OCW-FR-COLR, OCW-FR-ABUR: a file with a hunk opens in Source, every other file in Rich, and a choice wins", () => {
    const withHunks = pathsWithHunks(changeProposal(["pending", "accepted"]));
    expect([...withHunks]).toEqual(["proposal.md"]);
    const file = { path: "proposal.md", text: "x", unreadable: false, isNew: false };
    const other = { path: "design.md", text: "x", unreadable: false, isNew: false };
    expect(modeOf(file, {}, withHunks)).toBe("source");
    expect(modeOf(other, {}, withHunks)).toBe("rich");
    expect(modeOf(file, { "proposal.md": "rich" }, withHunks)).toBe("rich");
    expect(modeOf({ ...other, isNew: true }, { "design.md": "rich" }, withHunks)).toBe("source");
  });
});
