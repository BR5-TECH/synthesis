/** CMT-FR-MPKL: a new-draft reference outside the discussion stream. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CommentAttachmentList } from "./CommentAttachments";
import { configureDecompositionRoutes } from "../state/decompositionRoutes";
import { noteNewDraftChanged, resetNewDraftProposals } from "../state/newDraftProposals";
import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "list_new_draft_proposals" ? new Promise(() => {}) : otherCommand(cmd),
  );
  resetLogBufferForTest();
  resetNewDraftProposals();
});
afterEach(() => {
  cleanup();
  configureDecompositionRoutes(null);
});

describe("the new-draft reference control", () => {
  it("CMT-FR-MPKL: names Part N of M and the name, and opens the original draft's tab", () => {
    const opened: string[] = [];
    configureDecompositionRoutes({
      openDraft: (id) => opened.push(id),
      draftOf: () => undefined,
      openPreview: () => {},
      closePreview: () => {},
    });
    noteNewDraftChanged({
      draftId: "d1",
      cause: "recorded",
      proposal: {
        id: "p1",
        setId: "s",
        draftId: "d1",
        name: "relay-auth",
        position: 1,
        total: 2,
        agent: { kind: "agent", agentId: "a", handle: "arch" },
        rationale: "",
        threadId: "t",
        commentId: "c",
        state: "pending",
        revision: 1,
        createdAt: "",
      },
    });
    render(
      <CommentAttachmentList
        threadId="t"
        draftId="d1"
        attachments={[
          { kind: "newDraftProposal", proposalId: "p1", draftId: "d1" },
          { kind: "newDraftSubject", proposalId: "p1", draftId: "d1" },
        ]}
      />,
    );
    const control = screen.getByTestId("comment-attachment-new-draft");
    expect(control).toHaveTextContent("Part 1 of 2 · relay-auth");
    // A `new_draft_subject` renders no control.
    expect(screen.getAllByRole("button")).toHaveLength(1);
    fireEvent.click(control);
    expect(opened).toEqual(["d1"]);
  });
});
