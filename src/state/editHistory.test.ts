import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  createHistory,
  hasEdits,
  HISTORY_COALESCE_MS,
  recordEdit,
  redoStep,
  resetHistory,
  undoStep,
} from "./editHistory";

// The stack is time-sensitive (bursts coalesce), so the clock is driven
// explicitly rather than raced against. `recordEdit` reads Date.now() only.
let clock = 0;

beforeEach(() => {
  clock = 1_000;
  vi.spyOn(Date, "now").mockImplementation(() => clock);
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** Advance past the coalescing window so the next edit starts a fresh step. */
const seal = () => {
  clock += HISTORY_COALESCE_MS + 1;
};

describe("editHistory", () => {
  it("treats the reset document as the floor: undo is a no-op there", () => {
    const h = createHistory("loaded");

    expect(undoStep(h)).toBeNull();
    expect(undoStep(h)).toBeNull();
  });

  // EDT-FR-24: the floor is the artifact as loaded — never an empty document,
  // which is the bug this history exists to prevent.
  it("bottoms out at the loaded document after many edits", () => {
    const h = createHistory("loaded");

    for (let i = 0; i < 5; i++) {
      seal();
      recordEdit(h, `edit ${i}`, "wysiwyg", "body");
    }

    for (let i = 4; i >= 1; i--) {
      expect(undoStep(h)?.doc).toBe(`edit ${i - 1}`);
    }
    expect(undoStep(h)?.doc).toBe("loaded");
    expect(undoStep(h)).toBeNull();
  });

  it("carries the mode and surface of the step it traverses", () => {
    const h = createHistory("loaded");
    seal();
    recordEdit(h, "a", "wysiwyg", "frontmatter");
    seal();
    recordEdit(h, "b", "text", "source");

    // Undo reports the step being REVERSED (so the caller can reveal it there).
    expect(undoStep(h)).toEqual({
      doc: "a",
      mode: "text",
      surface: "source",
    });
    expect(undoStep(h)).toEqual({
      doc: "loaded",
      mode: "wysiwyg",
      surface: "frontmatter",
    });
    // Redo reports the step being RE-APPLIED.
    expect(redoStep(h)).toEqual({
      doc: "a",
      mode: "wysiwyg",
      surface: "frontmatter",
    });
  });

  it("is a no-op at the top of the stack", () => {
    const h = createHistory("loaded");
    seal();
    recordEdit(h, "a", "wysiwyg", "body");

    expect(redoStep(h)).toBeNull();
    expect(undoStep(h)?.doc).toBe("loaded");
    expect(redoStep(h)?.doc).toBe("a");
    expect(redoStep(h)).toBeNull();
  });

  it("drops the redo tail when a new edit is made after an undo", () => {
    const h = createHistory("loaded");
    seal();
    recordEdit(h, "a", "wysiwyg", "body");
    seal();
    recordEdit(h, "b", "wysiwyg", "body");

    undoStep(h); // back to "a"
    seal();
    recordEdit(h, "c", "wysiwyg", "body");

    // "b" is unreachable — the branch it was on was abandoned.
    expect(redoStep(h)).toBeNull();
    expect(undoStep(h)?.doc).toBe("a");
    expect(undoStep(h)?.doc).toBe("loaded");
  });

  it("ignores an edit that leaves the bytes unchanged", () => {
    const h = createHistory("loaded");
    seal();
    recordEdit(h, "a", "wysiwyg", "body");
    seal();
    recordEdit(h, "a", "wysiwyg", "body"); // same bytes

    expect(undoStep(h)?.doc).toBe("loaded");
    expect(undoStep(h)).toBeNull();
  });

  it("folds a burst on one surface into a single step", () => {
    const h = createHistory("loaded");
    recordEdit(h, "h", "text", "source");
    clock += 50;
    recordEdit(h, "he", "text", "source");
    clock += 50;
    recordEdit(h, "hel", "text", "source");

    expect(undoStep(h)?.doc).toBe("loaded");
    expect(undoStep(h)).toBeNull();
  });

  it("seals a burst once it exceeds the window, even while typing continues", () => {
    const h = createHistory("loaded");
    // Keystrokes closer together than the window, but a run longer than it.
    for (let i = 0; i < 6; i++) {
      recordEdit(h, `step ${i}`, "text", "source");
      clock += HISTORY_COALESCE_MS / 4;
    }

    // The run sealed at least once, so it is not one giant step.
    const first = undoStep(h);
    expect(first?.doc).not.toBe("loaded");
    expect(first?.doc).not.toBeUndefined();
  });

  it("does not fold edits made on different surfaces", () => {
    const h = createHistory("loaded");
    recordEdit(h, "a", "wysiwyg", "body");
    recordEdit(h, "b", "wysiwyg", "frontmatter"); // same instant

    expect(undoStep(h)?.doc).toBe("a");
    expect(undoStep(h)?.doc).toBe("loaded");
  });

  it("never folds the first edit into the floor", () => {
    const h = createHistory("loaded");
    recordEdit(h, "a", "text", "source");
    recordEdit(h, "ab", "text", "source"); // coalesces into "a"'s step

    expect(undoStep(h)?.doc).toBe("loaded");
  });

  // EDT-FR-24: adopting a load discards the buffer AND the history above it.
  it("drops the whole stack on reset, mid-burst included", () => {
    const h = createHistory("loaded");
    recordEdit(h, "a", "text", "source");
    resetHistory(h, "reloaded"); // burst still open
    recordEdit(h, "b", "text", "source");

    expect(undoStep(h)?.doc).toBe("reloaded");
    expect(undoStep(h)).toBeNull();
  });

  // The "undo depth is bounded only by the session" non-functional requirement.
  it("retains a long session's steps without truncating", () => {
    const h = createHistory("loaded");
    for (let i = 0; i < 300; i++) {
      seal();
      recordEdit(h, `v${i}`, "text", "source");
    }

    for (let i = 299; i >= 1; i--) {
      expect(undoStep(h)?.doc).toBe(`v${i - 1}`);
    }
    expect(undoStep(h)?.doc).toBe("loaded");
  });

  // EDT-FR-28: what distinguishes "the user edited this artifact" (retain the
  // session past the tab) from "they only read it" (drop it).
  it("reports whether any edit has been made since the floor", () => {
    const h = createHistory("loaded");
    expect(hasEdits(h)).toBe(false);

    recordEdit(h, "a", "text", "source");
    expect(hasEdits(h)).toBe(true);

    // Undoing back to the floor does not un-edit the artifact: the redo tail is
    // still reachable, so the session is still worth retaining.
    undoStep(h);
    expect(hasEdits(h)).toBe(true);

    resetHistory(h, "reloaded");
    expect(hasEdits(h)).toBe(false);
  });
});
