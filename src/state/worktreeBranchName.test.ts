import { describe, expect, it } from "vitest";

import { normalizeBranchInput } from "./worktreeBranchName";

describe("normalizeBranchInput (WTS-FR-UKSX)", () => {
  it("WTS-FR-UKSX: lower-cases and turns whitespace into a hyphen", () => {
    expect(normalizeBranchInput("More fixes")).toBe("more-fixes");
    expect(normalizeBranchInput("a\tb\nc")).toBe("a-b-c");
  });

  it("WTS-FR-UKSX: keeps the namespace separator and the allowed characters", () => {
    expect(normalizeBranchInput("feature/PROJ-12/Editor_scroll")).toBe(
      "feature/proj-12/editor_scroll",
    );
  });

  it("WTS-FR-UKSX: turns every other character into a hyphen", () => {
    expect(normalizeBranchInput("release/2.1")).toBe("release/2-1");
    expect(normalizeBranchInput("feat:a")).toBe("feat-a");
    expect(normalizeBranchInput("café")).toBe("caf-");
    expect(normalizeBranchInput("ÉCOLE")).toBe("-cole");
  });

  it("WTS-FR-UKSX: collapses a run of hyphens or of slashes", () => {
    expect(normalizeBranchInput("a - b")).toBe("a-b");
    expect(normalizeBranchInput("a--b")).toBe("a-b");
    expect(normalizeBranchInput("a//b")).toBe("a/b");
  });

  it("WTS-FR-UKSX: keeps a hyphen or a slash at an end so typing can continue", () => {
    expect(normalizeBranchInput("more ")).toBe("more-");
    expect(normalizeBranchInput("feature/")).toBe("feature/");
    expect(normalizeBranchInput(" a")).toBe("-a");
  });

  it("WTS-FR-UKSX: leaves no hyphen beside an inner slash", () => {
    expect(normalizeBranchInput("a-/-b")).toBe("a/b");
    expect(normalizeBranchInput("a/-/b")).toBe("a/b");
    expect(normalizeBranchInput("a / b")).toBe("a/b");
    expect(normalizeBranchInput("a-/")).toBe("a/");
    expect(normalizeBranchInput("a/-")).toBe("a/");
  });

  it("WTS-FR-UKSX: typing one character at a time gives the value of the whole text", () => {
    for (const text of ["More fixes", "A - B", "feature//x", "A-/-B", "a / b", "Team/Big Spike"]) {
      let typed = "";
      for (const ch of text) typed = normalizeBranchInput(typed + ch);
      expect(typed).toBe(normalizeBranchInput(text));
    }
  });

  it("WTS-FR-UKSX: is stable on its own output", () => {
    for (const raw of ["More  fixes", "a//b", "x.y/Z ", ""]) {
      const once = normalizeBranchInput(raw);
      expect(normalizeBranchInput(once)).toBe(once);
    }
  });
});
