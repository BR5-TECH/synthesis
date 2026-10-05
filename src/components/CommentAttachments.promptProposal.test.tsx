/**
 * The **prompt proposal** attachment's rendering (`CMT-comments.md` CMT-FR-48,
 * CMT-FR-49, CMT-FR-67, CTA-FR-UKIG, CTA-FR-XVNC).
 *
 * A reference to a change an agent proposed to a prompt artifact the project
 * already holds, rendered as a control on exactly the terms its `proposal`
 * sibling is: rendered alike, naming its file alike, activated alike — and
 * opening its own review over its own tab (PCR-FR-03, PCR-FR-18).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CommentAttachmentList } from "./CommentAttachments";
import { resetDraftProposals } from "../state/draftProposals";
import {
  configurePromptReviewRoute,
  notePromptProposalChanged,
  resetPromptProposals,
  reviewingPromptProposal,
  closePromptReview,
} from "../state/promptProposals";
import type { Attachment, PromptChangeProposal } from "../types";

const ARTIFACT = "prompts/review.md";

const promptReads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_prompt_change_proposals");
const draftReads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_draft_change_proposals");

const reference: Attachment = {
  kind: "promptProposal",
  proposalId: "p1",
  artifactId: ARTIFACT,
  path: ARTIFACT,
};

function proposal(over: Partial<PromptChangeProposal> = {}): PromptChangeProposal {
  return {
    id: "p1",
    artifactId: ARTIFACT,
    path: ARTIFACT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function renderList(attachments: Attachment[] = [reference]) {
  return render(
    <CommentAttachmentList threadId="t1" attachments={attachments} />,
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetPromptProposals();
  resetDraftProposals();
  configurePromptReviewRoute(null);
});

afterEach(() => {
  cleanup();
  closePromptReview();
  configurePromptReviewRoute(null);
});

// ---------------------------------------------------------------------------
// PCR-FR-03, PCR-FR-18 — the control (CMT-FR-48, CMT-FR-49, CMT-FR-67)
// ---------------------------------------------------------------------------

describe("the control (CMT-FR-48, CMT-FR-49)", () => {
  it("renders a control naming the prompt rather than a thumbnail or a chip", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();

    const control = await screen.findByTestId("comment-attachment-prompt-proposal");
    expect(control.tagName).toBe("BUTTON");
    expect(control).toHaveTextContent(ARTIFACT);
    expect(screen.queryByTestId("comment-attachment-image")).toBeNull();
    expect(screen.queryByTestId("comment-attachment-chip")).toBeNull();
    // CMT-FR-49: no content was fetched for it — the reading is the proposals
    // list and nothing else.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "read_comment_attachment"),
    ).toHaveLength(0);
  });

  it("opens the review over the artifact's own tab, posting nothing", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    const opened: string[] = [];
    configurePromptReviewRoute((id) => opened.push(id));
    renderList();

    const control = await screen.findByTestId("comment-attachment-prompt-proposal");
    await waitFor(() => expect(control).toBeEnabled());
    await act(async () => {
      await userEvent.click(control);
    });

    // PCR-FR-18: the tab is opened or focused first where it is not already
    // open, and the review opens over it.
    expect(opened).toEqual([ARTIFACT]);
    expect(reviewingPromptProposal()).toBe("p1");
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "add_comment"),
    ).toHaveLength(0);
  });

  it("states whatever the proposal has since become", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();
    expect(await screen.findByText("Review proposed change")).toBeInTheDocument();

    await act(async () => {
      notePromptProposalChanged({
        artifactId: ARTIFACT,
        proposal: proposal({ state: "accepted" }),
      });
    });
    // CMT-FR-67: the comment itself is unchanged and its control says what the
    // proposal became.
    expect(await screen.findByText("Change accepted")).toBeInTheDocument();
    expect(screen.getByTestId("comment-attachment-prompt-proposal")).toHaveTextContent(
      ARTIFACT,
    );
  });
});

// ---------------------------------------------------------------------------
// CMT-FR-48, CMT-FR-67, CTA-FR-SACG — the two reference kinds side by side (CTA-FR-UKIG, CTA-FR-OWQX)
// ---------------------------------------------------------------------------

describe("the two reference kinds (CTA-FR-UKIG, CTA-FR-XVNC)", () => {
  it("renders inert with no word about the proposal until a reading answers", async () => {
    // A reading that never resolves: nothing has looked for the proposal yet.
    invokeMock.mockImplementation(() => new Promise(() => {}));
    renderList();

    const control = await screen.findByTestId("comment-attachment-prompt-proposal");
    expect(control).toBeDisabled();
    expect(control).toHaveTextContent(ARTIFACT);
    for (const word of [
      "Review proposed change",
      "Change accepted",
      "Change rejected",
      "No longer available",
    ]) {
      expect(screen.queryByText(word)).toBeNull();
    }
  });

  it("says the change is gone only after a completed reading and one further ask", async () => {
    invokeMock.mockResolvedValue([]);
    renderList();

    // CTA-FR-XVNC: the first reading, then the one further ask this reference
    // gets — and only then does the control settle on saying so.
    await waitFor(() => expect(promptReads()).toHaveLength(2));
    expect(await screen.findByText("No longer available")).toBeInTheDocument();

    // …and it is not asked after for the rest of the session.
    await act(async () => {});
    expect(promptReads()).toHaveLength(2);
  });

  it("counts the two targets separately, one ask in flight per target", async () => {
    invokeMock.mockResolvedValue([]);
    const draftReference: Attachment = {
      kind: "proposal",
      proposalId: "dp1",
      draftId: "d1",
      path: "ui/spec.md",
    };
    render(
      <CommentAttachmentList
        threadId="t1"
        draftId="d1"
        attachments={[reference, draftReference, { ...reference, proposalId: "p2" }]}
      />,
    );

    await waitFor(() => expect(promptReads().length).toBeGreaterThan(0));
    await waitFor(() => expect(draftReads().length).toBeGreaterThan(0));
    // Two controls for one artifact, and each gets its own further ask — but
    // never two readings of one target in flight at a time.
    await waitFor(() => expect(promptReads()).toHaveLength(3));
    expect(draftReads()).toHaveLength(2);

    const controls = screen.getAllByTestId("comment-attachment-prompt-proposal");
    expect(controls).toHaveLength(2);
    // CMT-FR-48: the two kinds of control are rendered and activated alike.
    const draftControl = screen.getByTestId("comment-attachment-proposal");
    expect(draftControl.className).toBe(controls[0].className);
    expect(draftControl.tagName).toBe(controls[0].tagName);
  });
});
