/**
 * The **prompt proposal** attachment's rendering (`CMT-comments.md` CMT-FR-48,
 * CMT-FR-49, CMT-FR-67, CTA-FR-UKIG, CTA-FR-XVNC).
 *
 * A reference to a change an agent proposed to a prompt artifact the project
 * already holds, rendered as a control on exactly the terms its `proposal`
 * sibling is: rendered alike, naming its file alike, activated alike — and
 * scrolling its file's Editor tab to the first undecided change (PCR-FR-RFJC,
 * PCR-FR-18).
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
} from "../state/promptProposals";
import {
  artifactReviewKey,
  draftDiscussionState,
  resetDraftDiscussions,
} from "../state/draftDiscussion";
import { resetLogBufferForTest } from "../logging";
import { proposal as fixture } from "../test/promptProposalFixtures";
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
  return fixture({
    id: "p1",
    hunkCount: 3,
    counts: { pending: 2, accepted: 1, rejected: 0, discussing: 0 },
    ledger: [
      { id: "h1", kind: "replace", state: "accepted", edited: false, revision: 0 },
      { id: "h2", kind: "add", state: "pending", edited: false, revision: 0 },
      { id: "h3", kind: "del", state: "pending", edited: false, revision: 0 },
    ],
    ...over,
  });
}

function renderList(attachments: Attachment[] = [reference]) {
  return render(
    <CommentAttachmentList threadId="t1" attachments={attachments} />,
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetLogBufferForTest();
  resetPromptProposals();
  resetDraftProposals();
  resetDraftDiscussions();
  configurePromptReviewRoute(null);
});

afterEach(() => {
  cleanup();
  configurePromptReviewRoute(null);
});

// ---------------------------------------------------------------------------
// PCR-FR-RFJC, PCR-FR-18 — the control (CMT-FR-48, CMT-FR-49, CMT-FR-67)
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

  it("CMT-FR-48, PCR-FR-RFJC: shows how much the proposal changes", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();
    const control = await screen.findByTestId("comment-attachment-prompt-proposal");
    await waitFor(() => expect(control).toHaveTextContent("3 changes"));
    expect(control).toHaveTextContent("Review in the file");
  });

  it("CMT-FR-48, PCR-FR-RFJC: opens or focuses the tab and scrolls to the first undecided change, opening no modal", async () => {
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
    // open, and its document scrolls to the first undecided change.
    expect(opened).toEqual([ARTIFACT]);
    const state = draftDiscussionState(artifactReviewKey(ARTIFACT));
    expect(state.focusedHunkId).toBe("h2");
    expect(state.hunkReveal).toBe(1);
    expect(screen.queryByRole("dialog")).toBeNull();
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

// ---------------------------------------------------------------------------
// PCR-FR-RFJC — a reference to one change of a file's proposal
// ---------------------------------------------------------------------------

describe("a hunk reference of a file's proposal (PCR-FR-RFJC, CMS-FR-TQVB)", () => {
  it("PCR-FR-RFJC: names the change and scrolls the Editor tab to it", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    const opened: string[] = [];
    configurePromptReviewRoute((id) => opened.push(id));
    renderList([
      { kind: "proposalHunk", proposalId: "p1", hunkId: "h3", artifactId: ARTIFACT, path: ARTIFACT },
    ]);
    const control = await screen.findByTestId("comment-attachment-proposal-hunk");
    await waitFor(() => expect(control).toHaveTextContent("Change 3 of 3"));
    // No draft store is asked about a reference whose target is an artifact.
    expect(draftReads()).toHaveLength(0);
    await act(async () => {
      await userEvent.click(control);
    });
    expect(opened).toEqual([ARTIFACT]);
    expect(draftDiscussionState(artifactReviewKey(ARTIFACT)).focusedHunkId).toBe("h3");
  });
});
