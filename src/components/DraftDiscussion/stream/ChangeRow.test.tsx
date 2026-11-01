/**
 * What a change row says about how much its reference changes
 * (`../../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-HVXK,
 * DCR-FR-OYRR, DCR-FR-OZVU; `CMT-comments.md` CMT-FR-48).
 *
 * The row is the column's form of the control a card renders below a body, so
 * it says the same words: the count of changes for a recording, and the counts
 * of the update for an update comment.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { ChangeRow } from "./ChangeRow";
import { noteProposalChanged, resetDraftProposals } from "../../../state/draftProposals";
import { resetDraftDiscussions } from "../../../state/draftDiscussion";
import { resetLogBufferForTest } from "../../../logging";
import type { DraftChangeProposal, ProposalUpdateCounts } from "../../../types";
import type { ChangeReference } from "./streamItems";

const PROPOSAL: DraftChangeProposal = {
  id: "p1",
  draftId: "d1",
  path: "prompt.md",
  agent: { kind: "agent", agentId: "a1", handle: "helga" },
  rationale: "why",
  threadId: "t1",
  commentId: "c1",
  state: "pending",
  candidateEdited: false,
  legacy: false,
  hunkCount: 3,
  counts: { pending: 2, accepted: 0, rejected: 0, discussing: 0, withdrawn: 1 },
  ledger: [
    { id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 },
    { id: "h2", kind: "replace", state: "withdrawn", edited: false, revision: 0 },
    { id: "h3", kind: "add", state: "pending", edited: false, revision: 0 },
  ],
  createdAt: "2026-01-01T00:00:00Z",
};

function reference(update?: ProposalUpdateCounts): ChangeReference {
  return { kind: "proposal", proposalId: "p1", draftId: "d1", path: "prompt.md", update };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "list_draft_change_proposals") return [PROPOSAL];
    throw new Error(`unexpected command ${cmd}`);
  });
  resetLogBufferForTest();
  resetDraftProposals();
  resetDraftDiscussions();
});

afterEach(cleanup);

describe("the summary on a change row (DCR-FR-HVXK, DCR-FR-OYRR)", () => {
  it("DCR-FR-HVXK, DCR-FR-OZVU: a recording counts the changes that are not withdrawn", async () => {
    render(<ChangeRow reference={reference()} createdAt="2026-01-01T00:00:00Z" />);
    act(() => noteProposalChanged({ draftId: "d1", proposal: PROPOSAL, cause: "recorded" }));
    const row = screen.getByTestId("dds-change-row");
    await waitFor(() => expect(row).toHaveTextContent("2 changes"));
    expect(row).toHaveAccessibleName("prompt.md, 2 changes, pending");
  });

  it("DCR-FR-OYRR, CMT-FR-48: an update comment says what the update did in place of the count", async () => {
    render(
      <ChangeRow
        reference={reference({ changed: 2, added: 0, withdrawn: 1 })}
        createdAt="2026-01-01T00:00:00Z"
      />,
    );
    act(() => noteProposalChanged({ draftId: "d1", proposal: PROPOSAL, cause: "updated" }));
    const row = screen.getByTestId("dds-change-row");
    await waitFor(() => expect(row).toHaveAttribute("data-state", "pending"));
    expect(row).toHaveTextContent("2 changed, 0 added, 1 withdrawn");
    expect(row).not.toHaveTextContent("2 changes");
    expect(row).toHaveAccessibleName("prompt.md, 2 changed, 0 added, 1 withdrawn, pending");
  });
});
