/**
 * The proposal attachment's rendering (`CMT-comments.md` CMT-FR-48, CMT-FR-49,
 * CMT-FR-67).
 *
 * It is the one attachment whose rendering is an **action** rather than a
 * preview: it holds no content to show, and what it names is a decision the
 * author has to make. So it renders a control, it fetches nothing, and its state
 * comes from the proposals store rather than from the immutable log line.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CommentAttachmentList } from "./CommentAttachments";
import {
  noteProposalChanged,
  resetDraftProposals,
} from "../state/draftProposals";
import {
  draftDiscussionState,
  resetDraftDiscussions,
} from "../state/draftDiscussion";
import type { Attachment, DraftChangeProposal } from "../types";

const DRAFT = "d1";

/** Every reading the rail has asked the backend for. */
const reads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_draft_change_proposals");

const reference: Attachment = {
  kind: "proposal",
  proposalId: "p1",
  draftId: DRAFT,
  path: "ui/spec.md",
};

function proposal(over: Partial<DraftChangeProposal> = {}): DraftChangeProposal {
  return {
    id: "p1",
    draftId: DRAFT,
    path: "ui/spec.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: 1,
    counts: { pending: 1, accepted: 0, rejected: 0, discussing: 0 },
    ledger: [
      { id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 },
    ],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function renderList(attachments: Attachment[] = [reference]) {
  return render(
    <CommentAttachmentList
      threadId="t1"
      draftId={DRAFT}
      attachments={attachments}
    />,
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetDraftProposals();
  resetDraftDiscussions();
});

afterEach(cleanup);

describe("a proposal reference (CMT-FR-48, CMT-FR-49)", () => {
  it("renders a control rather than a thumbnail or a chip, and fetches nothing", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    expect(control.tagName).toBe("BUTTON");
    expect(control).toHaveTextContent("ui/spec.md");
    expect(screen.queryByTestId("comment-attachment-image")).toBeNull();
    expect(screen.queryByTestId("comment-attachment-chip")).toBeNull();

    // CMT-FR-49: no content is read for it — a conversation carrying a proposal
    // costs the rail exactly what one carrying a sentence costs.
    expect(invokeMock).not.toHaveBeenCalledWith(
      "read_comment_attachment",
      expect.anything(),
    );
  });

  it("opens the review on activation", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();
    const control = await screen.findByTestId("comment-attachment-proposal");
    await waitFor(() => expect(control).toBeEnabled());

    await userEvent.click(control);

    // The store is what the New Artifact tab's modal reads; the rail posts
    // nothing and invokes nothing on its own.
    const { useReviewing } = await import("../state/draftProposals");
    let seen: string | null = null;
    function Probe() {
      seen = useReviewing();
      return null;
    }
    render(<Probe />);
    expect(seen).toBe("p1");
  });
});

describe("what the control says (CMT-FR-67)", () => {
  it("follows the proposal's state rather than the immutable attachment", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    await waitFor(() => expect(control).toHaveAttribute("data-state", "pending"));
    expect(control).toHaveTextContent(/review proposed change/i);

    // The very same log line, after the author accepted.
    await act(async () => {
      noteProposalChanged({
        draftId: DRAFT,
        proposal: proposal({ state: "accepted" }),
      });
    });
    expect(control).toHaveAttribute("data-state", "accepted");
    expect(control).toHaveTextContent(/change accepted/i);

    await act(async () => {
      noteProposalChanged({
        draftId: DRAFT,
        proposal: proposal({ state: "rejected" }),
      });
    });
    expect(control).toHaveTextContent(/change rejected/i);
  });

  it("says so and opens nothing when the proposal no longer resolves", async () => {
    // Its draft has since been graduated or deleted, which takes its proposals
    // with it (DCP-FR-02) while the committed comment log outlives them.
    invokeMock.mockRejectedValue("draft_not_found");
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    await waitFor(() =>
      expect(control).toHaveAttribute("data-state", "missing"),
    );
    expect(control).toBeDisabled();
    expect(control).toHaveTextContent(/no longer available/i);
    // A draft that is not there answers every reference into it at once, so it
    // takes one reading rather than a further ask nothing could heal.
    expect(reads()).toHaveLength(1);
  });
});

/**
 * CTA-FR-SACG, CMT-FR-67 / CTA-FR-XVNC, CTA-FR-LOOE: what a control says before a reading has answered, and
 * the reading it asks for.
 *
 * The bug this closes: a control that renders "no longer available" for a
 * proposal nobody has looked for is not a cosmetic slip — it is the rail's only
 * route to the review (DCR-FR-02), so an author who sees it cannot decide a
 * proposal that is standing, and the agent that made it is refused another
 * while it stands (DCP-FR-04). The conversation then has nothing able to move
 * it.
 */
describe("a control the reading does not hold (CTA-FR-UKIG, CTA-FR-XVNC)", () => {
  it("is inert and claims nothing while the reading is still in flight", async () => {
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    expect(control).toHaveAttribute("data-state", "unresolved");
    expect(control).toBeDisabled();
    expect(control).toHaveTextContent("ui/spec.md");
    expect(control).not.toHaveTextContent(/no longer available/i);
    expect(control).not.toHaveTextContent(/review|accepted|rejected/i);

    await act(async () => {
      release([proposal()]);
      await Promise.resolve();
    });
    await waitFor(() =>
      expect(control).toHaveAttribute("data-state", "pending"),
    );
    expect(control).toHaveTextContent(/review proposed change/i);
    expect(control).toBeEnabled();
  });

  it("asks again while a reading fails, to a bound, and never says it is gone", async () => {
    invokeMock.mockRejectedValue(new Error("no project open"));
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    // CTA-FR-XVNC: asked again whenever a previous ask failed — the retry the
    // author never has to restart the application for — and bounded, so a
    // backend that refuses everything costs the rail three calls and not a
    // call per render.
    await waitFor(() => expect(reads()).toHaveLength(3));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(reads()).toHaveLength(3);
    // A reading that failed is not the session's answer, and it is certainly
    // not evidence that the proposal is gone.
    expect(control).toHaveAttribute("data-state", "unresolved");
    expect(control).not.toHaveTextContent(/no longer available/i);
    expect(control).toBeDisabled();
  });

  it("asks again after a reading that came back without the proposal", async () => {
    // The reading was taken before the agent recorded the proposal, and the
    // event that announced it never reached this window. This is the wedge the
    // change exists to close: without the further ask the author sees a dead
    // control, cannot decide, and the agent is refused every further proposal.
    let release: (v: unknown[]) => void = () => {};
    invokeMock
      .mockResolvedValueOnce([])
      .mockReturnValue(new Promise((r) => (release = r)));
    renderList();

    const control = await screen.findByTestId("comment-attachment-proposal");
    // The first reading completed without it and the further ask is away — and
    // in between, with one completed reading that did not return it, the
    // control still makes no claim.
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(control).toHaveAttribute("data-state", "unresolved");
    expect(control).not.toHaveTextContent(/no longer available/i);

    await act(async () => {
      release([proposal()]);
      await Promise.resolve();
    });
    await waitFor(() =>
      expect(control).toHaveAttribute("data-state", "pending"),
    );
    expect(control).toHaveTextContent(/review proposed change/i);
    expect(control).toBeEnabled();
    expect(reads()).toHaveLength(2);
  });

  it("costs one reading for many controls on one draft, and settles", async () => {
    // One ask is in flight per draft however many controls want it, and each
    // reference's further ask is made once: one shared reading plus one per
    // reference, and then silence.
    let inFlight = 0;
    let peak = 0;
    invokeMock.mockImplementation(async (name: string) => {
      if (name !== "list_draft_change_proposals") return [];
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      await Promise.resolve();
      inFlight -= 1;
      return [];
    });
    renderList([
      reference,
      { ...reference, proposalId: "p2" },
      { ...reference, proposalId: "p3" },
    ]);

    const controls = await screen.findAllByTestId(
      "comment-attachment-proposal",
    );
    expect(controls).toHaveLength(3);
    await waitFor(() =>
      controls.forEach((c) =>
        expect(c).toHaveAttribute("data-state", "missing"),
      ),
    );

    const settled = reads().length;
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(reads()).toHaveLength(settled);
    expect(settled).toBe(4);
    expect(peak).toBe(1);
  });

  it("reads nothing at all for a reference carrying no draft", async () => {
    invokeMock.mockResolvedValue([]);
    render(
      <CommentAttachmentList
        threadId="t1"
        attachments={[{ ...reference, draftId: "" }]}
      />,
    );

    const control = await screen.findByTestId("comment-attachment-proposal");
    expect(control).toHaveAttribute("data-state", "unresolved");
    expect(control).toBeDisabled();
    // CTA-FR-UKIG: no word about the proposal anywhere on it — the tooltip
    // included, which is the one place a claim could hide.
    expect(control).toHaveAttribute("title", "ui/spec.md");
    expect(control).not.toHaveTextContent(/no longer available/i);
    expect(reads()).toHaveLength(0);
  });
});

describe("beside the other kinds", () => {
  it("renders each attachment in its own form", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    renderList([
      reference,
      {
        kind: "url",
        url: "https://example.test/a.pdf",
        mediaType: "application/pdf",
        label: "spec v2",
      },
    ]);

    expect(
      await screen.findByTestId("comment-attachment-proposal"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("comment-attachment-chip")).toHaveTextContent(
      "spec v2",
    );
  });

  it("DCR-FR-HVXK: the card summarises how much the proposal changes and where it is read", async () => {
    // The card is what the author reads the offer from, so it says how much
    // there is to decide before they go and decide it — and it says where,
    // because the review is in the document column beside this conversation
    // rather than in something that opens over it.
    invokeMock.mockResolvedValue([
      proposal({
        hunkCount: 3,
        counts: { pending: 2, accepted: 1, rejected: 0, discussing: 0 },
        ledger: [
          { id: "h1", kind: "replace", state: "accepted", edited: false, revision: 0 },
          { id: "h2", kind: "add", state: "pending", edited: false, revision: 0 },
          { id: "h3", kind: "del", state: "pending", edited: false, revision: 0 },
        ],
      }),
    ]);
    renderList();

    const card = await screen.findByTestId("comment-attachment-proposal");
    expect(card).toHaveTextContent("3 changes");
    expect(card).toHaveTextContent("Review on the left");

    // DCR-FR-03: activating it takes the author to the first change still to
    // decide, rather than opening anything over the draft.
    await userEvent.click(card);
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h2");
  });

});