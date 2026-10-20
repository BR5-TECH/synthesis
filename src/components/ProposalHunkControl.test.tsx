/**
 * A `proposal_hunk` reference renders as a control naming one change
 * (`CMT-comments.md` CMT-FR-48; `DCR-draft-change-review.md` DCR-FR-17;
 * `CMS-comments-storage.md` CMS-FR-TQVB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CommentAttachmentList } from "./CommentAttachments";
import {
  noteProposalChanged,
  resetDraftProposals,
  reviewingProposal,
} from "../state/draftProposals";
import { draftDiscussionState, resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLogBufferForTest } from "../logging";
import type { DraftChangeProposal, HunkState, ProposalHunkReference } from "../types";

const reference: ProposalHunkReference = {
  kind: "proposalHunk",
  proposalId: "p1",
  hunkId: "h2",
  draftId: "d1",
  path: "Spec.md",
};

function proposal(
  states: HunkState[],
  state: DraftChangeProposal["state"] = "pending",
): DraftChangeProposal {
  return {
    id: "p1",
    draftId: "d1",
    path: "Spec.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state,
    candidateEdited: false,
    legacy: false,
    hunkCount: states.length,
    counts: { pending: 0, accepted: 0, rejected: 0, discussing: 0 },
    ledger: states.map((s, at) => ({
      id: `h${at + 1}`,
      kind: "replace",
      state: s,
      edited: false,
      revision: 0,
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  // The reading this control asks for answers with the proposal the case seeds.
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records" ? Promise.resolve() : Promise.reject(new Error(cmd)),
  );
  resetDraftProposals();
  resetDraftDiscussions();
  resetLogBufferForTest();
});

afterEach(cleanup);

function mount() {
  render(<CommentAttachmentList threadId="disc-1" draftId="d1" attachments={[reference]} />);
  return screen.getByTestId("comment-attachment-proposal-hunk");
}

describe("CMT-FR-48: the control a change reference renders", () => {
  it("CMT-FR-48, CMS-FR-TQVB: names the change by its place and says how it stands", () => {
    act(() => noteProposalChanged({ draftId: "d1", proposal: proposal(["pending", "rejected", "pending"]) }));
    const control = mount();
    expect(control).toHaveTextContent("Change 2 of 3");
    expect(control).toHaveTextContent("rejected");
    expect(control).toBeEnabled();
  });

  it("CMT-FR-48: activating it puts the review on that change and asks the document to show it, each time", () => {
    act(() => noteProposalChanged({ draftId: "d1", proposal: proposal(["pending", "pending"]) }));
    const control = mount();
    fireEvent.click(control);
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h2");
    expect(draftDiscussionState("d1").hunkReveal).toBe(1);
    expect(reviewingProposal()).toBe("p1");
    // The review is on the change already, and the document is asked again.
    fireEvent.click(control);
    expect(draftDiscussionState("d1").hunkReveal).toBe(2);
  });

  it("CMT-FR-48, DCR-FR-07: an accepted change of an open proposal has no decoration to go to", () => {
    act(() => noteProposalChanged({ draftId: "d1", proposal: proposal(["pending", "accepted"]) }));
    const control = mount();
    expect(control).toHaveTextContent("accepted");
    expect(control).toBeDisabled();
  });

  it("DCR-FR-17: once the proposal closes there is nothing to go to, so the control only says how the change went", () => {
    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: proposal(["accepted", "accepted"], "accepted"),
      }),
    );
    const control = mount();
    expect(control).toHaveTextContent("accepted");
    expect(control).toBeDisabled();
  });
});
