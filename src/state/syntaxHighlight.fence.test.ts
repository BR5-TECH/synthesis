/**
 * The language of a fenced code block of the issue view, and the Gherkin
 * grammar (`MIV-map-issue-view.md` MIV-FR-ALPJ, MIV-FR-IVGV, MIV-FR-EDFS;
 * `ESH-editor-source-files.md` ESH-FR-LMNI, ESH-FR-QBZJ).
 */
import hljs from "highlight.js/lib/common";
import { describe, expect, it, vi } from "vitest";

vi.mock("../logging", () => ({ logWarn: vi.fn(), logInfo: vi.fn(), logDebug: vi.fn(), logError: vi.fn() }));

import { detectLanguage, fenceLanguage, highlightFile, tokenize } from "./syntaxHighlight";

describe("fenceLanguage (MIV-FR-ALPJ, MIV-FR-IVGV)", () => {
  it.each([
    ["Python", "python"],
    ["{.python}", "python"],
    [".rust", "rust"],
    ["language-ts", "ts"],
    ["LANGUAGE-Python", "python"],
    ["py title=x", "py"],
    ["rust,ignore", "rust"],
    ["gherkin", "gherkin"],
    ["feature", "feature"],
    ["sc", "scala"],
    ["unknownlang", null],
    ["", null],
    ["   ", null],
    ["language-", null],
    ["mermaid", null],
    ["calm", null],
  ])("MIV-FR-ALPJ: the info string %j resolves to %j", (info, expected) => {
    expect(fenceLanguage(info)).toBe(expected);
  });

  it("MIV-FR-IVGV: no info string resolves to nothing, and nothing is autodetected", () => {
    const auto = vi.spyOn(hljs, "highlightAuto");
    expect(fenceLanguage(undefined)).toBeNull();
    expect(fenceLanguage(null)).toBeNull();
    expect(fenceLanguage("prose")).toBeNull();
    expect(auto).not.toHaveBeenCalled();
    auto.mockRestore();
  });
});

describe("the Gherkin grammar (ESH-FR-LMNI, MIV-FR-EDFS)", () => {
  it("ESH-FR-LMNI: Gherkin is registered with the aliases gherkin and feature, and takes no part in autodetection", () => {
    expect(hljs.getLanguage("gherkin")).toBeDefined();
    expect(hljs.getLanguage("feature")).toBe(hljs.getLanguage("gherkin"));
    expect(hljs.getLanguage("gherkin")?.disableAutodetect).toBe(true);
  });

  it("ESH-FR-QBZJ: a prose file of Given, When, and Then lines is never detected as Gherkin", () => {
    const prose = [
      "Feature: release notes",
      "Given the team agrees, we ship on Friday.",
      "When the build is green, the release goes out.",
      "Then we write the notes and close the milestone.",
    ].join("\n").repeat(4);
    expect(detectLanguage(prose)).not.toBe("gherkin");
    expect(highlightFile("notes.md", prose)).toBeNull();
  });

  it("MIV-FR-EDFS: the step keywords take the keyword role, and a comment takes the comment role", () => {
    const text = "Scenario: pay\n  # note\n  Given a cart\n  Then it is paid";
    const spans = tokenize(text, "gherkin")!;
    const roleOf = (word: string) => spans.find((s) => text.slice(s.start, s.end).includes(word))?.role;
    expect(roleOf("Given")).toBe("keyword");
    expect(roleOf("Then")).toBe("keyword");
    expect(roleOf("Scenario")).toBe("keyword");
    expect(roleOf("# note")).toBe("comment");
  });
});
