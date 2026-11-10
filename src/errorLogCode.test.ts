/** The code of a rejection, for a log record (`LOG-logs.md` LOG-FR-20). */
import { describe, expect, it } from "vitest";

import { errorLogCode } from "./errorLogCode";

describe("errorLogCode (LOG-FR-20)", () => {
  it("LOG-FR-20: a backend error gives its code and never its message", () => {
    expect(errorLogCode({ code: "github_unreachable", message: "GitHub at host x did not answer." })).toBe(
      "github_unreachable",
    );
  });

  it("LOG-FR-20: a string error gives the code before its colon", () => {
    expect(errorLogCode("main_branch_write_unconfirmed: {\"branch\":\"main\"}")).toBe(
      "main_branch_write_unconfirmed",
    );
    expect(errorLogCode("no_project_open")).toBe("no_project_open");
  });

  it("LOG-FR-20: an Error gives the code of its message, or its code property", () => {
    expect(errorLogCode(new Error("not_found: plans/a.flow"))).toBe("not_found");
    expect(errorLogCode(Object.assign(new Error("text"), { code: "io_error" }))).toBe("io_error");
  });

  it("LOG-FR-20: a rejection with no code reads as unknown, so no text reaches the log", () => {
    expect(errorLogCode("The disk at /home/me is full")).toBe("unknown");
    expect(errorLogCode(new Error("Something failed for Tokenize card"))).toBe("unknown");
    expect(errorLogCode({ code: "Not a code!", message: "x" })).toBe("unknown");
    expect(errorLogCode(undefined)).toBe("unknown");
    expect(errorLogCode(42)).toBe("unknown");
  });
});
