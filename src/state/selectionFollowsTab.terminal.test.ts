// SNV-FR-66 for the Terminal tab (`SNV-shell-navigation.md`, `TRM-terminals.md`).
import { describe, expect, it } from "vitest";
import { followTargetForTab } from "./selectionFollowsTab";

describe("selection follows a Terminal tab", () => {
  it("SNV-FR-66, TRM-FR-RCCE: a Terminal tab selects in the Terminals panel by its terminal id", () => {
    expect(
      followTargetForTab({ id: "terminal:t9", label: "Host shell 1", kind: "terminal", terminalId: "t9" }),
    ).toEqual({ panel: "terminals", id: "t9" });
  });

  it("SNV-FR-66: a Terminal tab with no terminal id selects nowhere", () => {
    expect(followTargetForTab({ id: "terminal:", label: "x", kind: "terminal" })).toBeNull();
  });
});
