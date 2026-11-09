/**
 * The writes of the New Artifact workspace that ask for the confirmation on a
 * main branch (`../../specifications/ui/MBW-main-branch-write.md` MBW-FR-DBGA,
 * MBW-FR-XBJS, MBW-FR-ILFN): the accept of a change of the draft's proposal and
 * the rename of the draft from the title of its tab.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { PROMPT, draft, freshHistory, makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { MainBranchWriteHost } from "./MainBranchWrite";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";
import { currentMainBranchWrite } from "../state/mainBranchWrite";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftProposals } from "../state/draftProposals";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";

const { stub } = makeStubs(invokeMock);

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

const BODY = "# Spec\n\nThe first line.\n";

const PROPOSAL = {
  id: "p1",
  draftId: "d1",
  path: PROMPT,
  agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
  rationale: "One thing to tighten.",
  threadId: "t1",
  commentId: "c1",
  state: "pending",
  candidateEdited: false,
  legacy: false,
  hunkCount: 1,
  counts: { pending: 1, accepted: 0, rejected: 0, discussing: 0 },
  ledger: [{ id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 }],
  createdAt: "2026-01-01T00:00:00Z",
};

const HUNKS = {
  hunks: [
    {
      id: "h1",
      kind: "replace",
      revision: 0,
      before: "The first line.",
      after: "The opening line.",
      anchor: {
        lead: "",
        trail: "",
        hint_start: BODY.indexOf("The first line."),
        hint_end: BODY.indexOf("The first line.") + "The first line.".length,
      },
    },
  ],
  resolutions: [{ kind: "resolved", start: 0, end: 1 }],
  checksum: "hc-1",
  legacy: false,
};

function proposalBackend() {
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "open_draft":
        return draft();
      case "list_draft_history":
        return freshHistory();
      case "load_draft_file_contents":
        return { body: BODY, checksum: "c1" };
      case "save_draft_file_contents":
        return { checksum: "c2" };
      case "list_draft_change_proposals":
        return [PROPOSAL];
      case "load_draft_change_proposal_hunks":
        return HUNKS;
      case "accept_draft_change_hunk":
        return { proposal: PROPOSAL, commentId: undefined, originKind: "draft_discussion" };
      default:
        return undefined;
    }
  });
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftProposals();
  resetDraftDiscussions();
  resetProposalHunks();
  stub();
});

afterEach(cleanup);

describe("the writes of the draft workspace on a main branch (MBW-FR-DBGA)", () => {
  it("DCR-FR-13, MBW-FR-DBGA, MBW-FR-XBJS: Accept waits for the confirmation, a cancel invokes no accept, and a confirm invokes it once", async () => {
    const reads = scriptedReads(mainGuard());
    proposalBackend();
    renderWorkspace();
    render(<MainBranchWriteHost />);
    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    await waitFor(() => expect(callsTo("load_draft_change_proposal_hunks")).toHaveLength(1));

    await userEvent.click(within(bar).getByRole("button", { name: "Accept all" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to accept a change.");
    expect(callsTo("accept_draft_change_hunk")).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(currentMainBranchWrite()).toBeNull());
    expect(callsTo("accept_draft_change_hunk")).toHaveLength(0);
    expect(reads.confirms).toEqual([]);

    await userEvent.click(within(bar).getByRole("button", { name: "Accept all" }));
    await userEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(callsTo("accept_draft_change_hunk")).toHaveLength(1));
    expect(callsTo("accept_draft_change_hunk")[0]).toEqual({
      proposalId: "p1",
      hunkId: "h1",
      feedback: null,
    });
    expect(reads.confirms).toEqual(["main"]);
  });

  it("NAW-FR-04, MBW-FR-DBGA, MBW-FR-XBJS: a rename from the title waits for the confirmation, a cancel invokes no rename, and a confirm invokes it once", async () => {
    const reads = scriptedReads(mainGuard());
    const { onNameChanged } = renderWorkspace();
    render(<MainBranchWriteHost />);

    const rename = async () => {
      await userEvent.click(await screen.findByRole("button", { name: /artifact-window/ }));
      const field = screen.getByLabelText("Draft name");
      await userEvent.clear(field);
      await userEvent.type(field, "agent personas{Enter}");
    };

    await rename();
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to rename an item.");
    expect(callsTo("rename_draft")).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(currentMainBranchWrite()).toBeNull());
    expect(callsTo("rename_draft")).toHaveLength(0);
    expect(onNameChanged).not.toHaveBeenCalled();
    expect(reads.confirms).toEqual([]);

    // The field stays open with the typed text, so the author can press Enter again.
    await userEvent.type(screen.getByLabelText("Draft name"), "{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(callsTo("rename_draft")).toHaveLength(1));
    expect(reads.confirms).toEqual(["main"]);
    await waitFor(() => expect(onNameChanged).toHaveBeenCalledTimes(1));
  });
});
