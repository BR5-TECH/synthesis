/**
 * The count of the author's asks to see a change in the document
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-PBXN;
 * `CMT-comments.md` CMT-FR-48).
 */
import { beforeEach, describe, expect, it } from "vitest";

import {
  draftDiscussionState,
  resetDraftDiscussions,
  revealHunk,
  setFocusedHunk,
} from "./draftDiscussion";

beforeEach(() => {
  resetDraftDiscussions();
});

describe("DDS-FR-PBXN, CMT-FR-48: revealing a change", () => {
  it("DDS-FR-PBXN: puts the review on the change and counts the ask, also for the change it is on", () => {
    expect(draftDiscussionState("d1").hunkReveal).toBe(0);
    revealHunk("d1", "h2");
    expect(draftDiscussionState("d1")).toMatchObject({ focusedHunkId: "h2", hunkReveal: 1 });
    revealHunk("d1", "h2");
    expect(draftDiscussionState("d1")).toMatchObject({ focusedHunkId: "h2", hunkReveal: 2 });
  });

  it("DDS-FR-PBXN: a move of the review is not an ask to see the change", () => {
    revealHunk("d1", "h1");
    setFocusedHunk("d1", "h3");
    expect(draftDiscussionState("d1")).toMatchObject({ focusedHunkId: "h3", hunkReveal: 1 });
  });

  it("DDS-FR-PBXN: each draft counts its own asks, and a reset clears them", () => {
    revealHunk("d1", "h1");
    expect(draftDiscussionState("d2").hunkReveal).toBe(0);
    resetDraftDiscussions();
    expect(draftDiscussionState("d1").hunkReveal).toBe(0);
  });
});
