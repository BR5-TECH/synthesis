/**
 * Discuss on a proposed draft (`DCR-draft-change-review.md` DCR-FR-AMLR) and
 * the opening composer it starts (`OpenDiscussion`).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useHunkDiscussion } from "./useHunkDiscussion";
import { useNewDraftDiscussRoute } from "./useNewDraftDiscussRoute";
import { discussNewDraft } from "../../state/decompositionRoutes";
import { noteNewDraftChanged, resetNewDraftProposals } from "../../state/newDraftProposals";
import { resetDraftDiscussions } from "../../state/draftDiscussion";
import { clearAllDiscussionSessions, getDiscussionSession } from "../../state/discussionSession";
import { resetLogBufferForTest } from "../../logging";
import { otherCommand } from "../../test/invokeMocks";
import { DraftPreviewView } from "../DraftPreview";
import type { Discussion, NewDraftProposal } from "../../types";

const PROPOSAL: NewDraftProposal = {
  id: "p1",
  setId: "s",
  draftId: "d1",
  name: "relay-auth",
  position: 2,
  total: 3,
  agent: { kind: "agent", agentId: "a1", handle: "arch" },
  rationale: "why",
  threadId: "t1",
  commentId: "c1",
  state: "pending",
  revision: 1,
  createdAt: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => otherCommand(cmd));
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetNewDraftProposals();
  resetLogBufferForTest();
});
afterEach(cleanup);

function mount(discussions: Discussion[] = []) {
  const selectDiscussion = vi.fn();
  const focusComposer = vi.fn();
  const hook = renderHook(() => {
    const h = useHunkDiscussion({
      draftId: "d1",
      proposal: undefined,
      discussions,
      selectDiscussion,
      focusComposer,
    });
    useNewDraftDiscussRoute("d1", h.discussNewDraft);
    return h;
  });
  return { hook, selectDiscussion, focusComposer };
}

describe("Discuss on a proposed draft", () => {
  it("DCR-FR-AMLR: opens the composer of a new discussion seeded with the agent's tag and a new_draft_subject", () => {
    noteNewDraftChanged({ draftId: "d1", proposal: PROPOSAL, cause: "recorded" });
    const { hook, focusComposer } = mount();
    act(() => discussNewDraft("d1", "p1"));
    expect(hook.result.current.opening).toEqual({
      reference: { kind: "newDraftSubject", proposalId: "p1", draftId: "d1" },
      position: 2,
      total: 3,
    });
    expect(getDiscussionSession("draft:d1").body).toBe("@arch ");
    expect(focusComposer).toHaveBeenCalled();
    expect(invokeMock.mock.calls.filter((c) => c[0] !== "append_log_records")).toHaveLength(0);
  });

  it("DCR-FR-AMLR: a discussion that exists is shown instead of a second", () => {
    noteNewDraftChanged({ draftId: "d1", proposal: PROPOSAL, cause: "recorded" });
    const existing: Discussion = {
      id: "t9",
      target: { kind: "draft", draftId: "d1" },
      fragmentTarget: null,
      resolved: true,
      createdAt: "",
      updatedAt: "",
      comments: [
        {
          id: "c",
          author: { kind: "agent", agentId: "a1", handle: "arch" },
          body: "x",
          quotes: [],
          attachments: [{ kind: "newDraftSubject", proposalId: "p1", draftId: "d1" }],
          createdAt: "",
        },
      ],
    };
    const { hook, selectDiscussion } = mount([existing]);
    act(() => discussNewDraft("d1", "p1"));
    expect(selectDiscussion).toHaveBeenCalledWith("t9");
    expect(hook.result.current.opening).toBeNull();
  });
});

describe("The draft preview tab", () => {
  it("DCR-FR-HWLA, DCR-FR-RJZP, NAW-FR-ASOA: shows the body read-only, re-reads on an update, and creates nothing", async () => {
    let body = "# First body";
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "load_new_draft_proposal_body" ? Promise.resolve({ body }) : otherCommand(cmd),
    );
    noteNewDraftChanged({ draftId: "d1", proposal: PROPOSAL, cause: "recorded" });
    render(<DraftPreviewView proposalId="p1" label="relay-auth" />);
    expect(await screen.findByText("First body")).toBeInTheDocument();
    expect(screen.getByTestId("draft-preview")).toHaveTextContent("Part 2 of 3");
    expect(screen.getByRole("document")).toHaveAttribute("aria-readonly", "true");
    body = "# Second body";
    act(() =>
      noteNewDraftChanged({ draftId: "d1", proposal: { ...PROPOSAL, revision: 2 }, cause: "updated" }),
    );
    expect(await screen.findByText("Second body")).toBeInTheDocument();
    const commands = invokeMock.mock.calls.map((c) => c[0]);
    expect(commands.filter((c) => c === "create_draft")).toHaveLength(0);
  });
});
