/**
 * The review overlay passes the author's asks to see a change on to the
 * document's scroll (`../../../specifications/ui/DDS-draft-discussion.md`
 * DDS-FR-PBXN; `CMT-comments.md` CMT-FR-48).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

import { ReviewOverlay } from "./ReviewOverlay";
import { HUNK_ATTR } from "../DraftDiscussion/hunkDecorations";
import type { EditorReview } from "./props";

afterEach(cleanup);

/** A document holding the marks of these changes, and a scroll that is counted. */
function host(hunkIds: string[]): HTMLDivElement {
  const el = document.createElement("div");
  for (const id of hunkIds) {
    const mark = document.createElement("p");
    mark.setAttribute(HUNK_ATTR, id);
    el.appendChild(mark);
  }
  el.scrollTo = vi.fn() as unknown as HTMLElement["scrollTo"];
  return el;
}

function review(reveal: number): EditorReview {
  return {
    proposalId: "p1",
    hunks: [],
    focused: "h1",
    reveal,
    busy: false,
    onFocus: vi.fn(),
    onAccept: vi.fn(),
    onReject: vi.fn(),
    onDiscuss: vi.fn(),
    onEdit: vi.fn(),
    onPlaced: vi.fn(),
  };
}

describe("DDS-FR-PBXN, CMT-FR-48: the overlay and the document's scroll", () => {
  it("DDS-FR-PBXN, CMT-FR-48: a new ask scrolls the document to the change the review is on", () => {
    const el = host(["h1"]);
    const ref = { current: el };
    const overlay = (n: number) => (
      <ReviewOverlay review={review(n)} placed={[]} focused={null} hostRef={ref} extent={0} />
    );
    const { rerender } = render(overlay(0));
    expect(el.scrollTo).not.toHaveBeenCalled();

    rerender(overlay(1));
    expect(el.scrollTo).toHaveBeenCalledTimes(1);
  });
});
