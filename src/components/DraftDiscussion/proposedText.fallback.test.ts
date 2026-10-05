/**
 * A proposed text the Editor cannot render is still the author's to read
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-09).
 *
 * A file of its own because the editor is mocked to fail, and `vi.mock` holds
 * for the whole file it is written in.
 */
import { describe, expect, it, vi } from "vitest";

const { logWarn } = vi.hoisted(() => ({ logWarn: vi.fn() }));

vi.mock("@tiptap/react", () => ({
  Extension: { create: () => ({}) },
  Editor: class {
    constructor() {
      throw new TypeError("cannot build");
    }
  },
}));
vi.mock("../../logging", () => ({ logWarn }));
vi.mock("../markdownFidelity", () => ({
  getMarkdown: () => "",
  markdownExtensions: () => [],
}));

import { mountProposedText } from "./proposedText";

describe("a proposed text the Editor cannot render (DCR-FR-09)", () => {
  it("DCR-FR-09: is shown as written, read-only, and the failure is logged without the text", () => {
    const host = document.createElement("div");
    const teardown = mountProposedText(host, "\n\nThe *secret* body\n\nA second line\n", {
      editable: true,
      label: "Proposed text of change 1 of 1",
      onChange: () => {},
    });
    expect(host.textContent).toBe("The *secret* body\n\nA second line");
    // Its line breaks are kept on screen by this class (style invariants).
    expect(host.classList.contains("hunk__plain")).toBe(true);
    expect(host.querySelector("[contenteditable]")).toBeNull();
    expect(logWarn).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(logWarn.mock.calls[0])).not.toContain("secret");
    expect(() => teardown()).not.toThrow();
  });
});
