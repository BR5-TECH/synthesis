/**
 * The marks of the fragment discussions in the Source of a change file
 * (`../../../specifications/ui/CMT-comments.md` CMT-FR-28, CMT-FR-18,
 * CMT-FR-AYSL).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import type { Discussion, DiscussionTarget } from "../../types";
import { markedText, marksIn, type FragmentMarks } from "./fragmentMarks";

const OWNER: DiscussionTarget = { kind: "change", streamId: "s1", change: "add-logout" };
const TEXT = "# Why\n\nUsers cannot log out.\n";

function discussion(id: string, path: string, start: number, quote: string, resolved = false): Discussion {
  return {
    id,
    target: OWNER,
    fragmentTarget: { owner: OWNER, path, start, end: start + quote.length, quote },
    comments: [],
    resolved,
    createdAt: "2026-11-12T00:00:00Z",
    updatedAt: "2026-11-12T00:00:00Z",
  };
}

afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
});

describe("marksIn (CMT-FR-18, CMT-FR-28)", () => {
  it("CMT-FR-28, CMT-FR-18: an open fragment discussion of the file is placed, also where its text moved", () => {
    const moved = discussion("t1", "proposal.md", 0, "cannot log out");
    expect(marksIn("proposal.md", TEXT, [moved])).toEqual([
      { threadId: "t1", start: TEXT.indexOf("cannot"), end: TEXT.indexOf("cannot") + 14 },
    ]);
  });

  it("CMT-FR-28: a resolved, an orphaned, a whole-change and another file's discussion show no mark", () => {
    const whole: Discussion = { ...discussion("w", "proposal.md", 0, "# Why"), fragmentTarget: null };
    const found = marksIn("proposal.md", TEXT, [
      discussion("r", "proposal.md", 0, "# Why", true),
      discussion("o", "proposal.md", 0, "not in the text"),
      whole,
      discussion("x", "design.md", 0, "# Why"),
    ]);
    expect(found).toEqual([]);
  });
});

function draw(placed: Parameters<typeof markedText>[3], marks: FragmentMarks) {
  render(<pre data-testid="text">{markedText(TEXT, 0, TEXT.length, placed, marks)}</pre>);
}

describe("markedText (CMT-FR-28, CMT-FR-AYSL)", () => {
  it("CMT-FR-AYSL: the marks add no character, so the text reads the same", () => {
    const marks = { discussions: [], focused: null, onActivate: vi.fn() };
    draw([{ threadId: "t1", start: 0, end: 5 }], marks);
    expect(screen.getByTestId("text").textContent).toBe(TEXT);
  });

  it("CMT-FR-28: each mark carries the anchor class and a label that names its discussion; the focused one is distinct", () => {
    const marks = { discussions: [], focused: "t2", onActivate: vi.fn() };
    draw(
      [
        { threadId: "t1", start: 0, end: 5 },
        { threadId: "t2", start: 13, end: 19 },
      ],
      marks,
    );
    const [first, second] = screen.getAllByTestId("openspec-fragment-mark");
    expect(first).toHaveClass("comment-anchor");
    expect(first).not.toHaveClass("comment-anchor--focused");
    expect(first).toHaveAttribute("aria-label", "Fragment of discussion t1");
    expect(first).toHaveAttribute("data-comment-thread", "t1");
    expect(second).toHaveClass("comment-anchor--focused");
    expect(second).toHaveAttribute("aria-label", "Fragment of discussion t2 (focused)");
    expect(second).toHaveTextContent("cannot");
  });

  it("CMT-FR-28: where two ranges overlap, the shared part names the focused discussion", () => {
    const marks = { discussions: [], focused: "b", onActivate: vi.fn() };
    draw(
      [
        { threadId: "a", start: 7, end: 19 },
        { threadId: "b", start: 13, end: 25 },
      ],
      marks,
    );
    const parts = screen.getAllByTestId("openspec-fragment-mark");
    expect(parts.map((p) => [p.getAttribute("data-comment-thread"), p.textContent])).toEqual([
      ["a", "Users "],
      ["b", "cannot"],
      ["b", " log o"],
    ]);
  });

  it("CMT-FR-28: a click on a mark activates its discussion, and a click that ends a selection does not", () => {
    const onActivate = vi.fn();
    draw([{ threadId: "t1", start: 0, end: 5 }], { discussions: [], focused: null, onActivate });
    const mark = screen.getByTestId("openspec-fragment-mark");
    fireEvent.click(mark);
    expect(onActivate).toHaveBeenCalledWith("t1");

    onActivate.mockReset();
    const range = document.createRange();
    range.selectNodeContents(mark);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);
    fireEvent.click(mark);
    expect(onActivate).not.toHaveBeenCalled();
    window.getSelection()!.removeAllRanges();
  });
});
