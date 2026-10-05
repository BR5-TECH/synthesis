/**
 * A change proposed to one of the draft's files, as the New Artifact workspace
 * reviews it (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-35).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  PROMPT,
  draft,
  freshHistory,
  makeStubs,
  renderWorkspace,
  editSource,
} from "../test/newArtifactFixtures";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import {
  closeReview,
  noteProposalChanged,
  resetDraftProposals,
} from "../state/draftProposals";
import {
  draftDiscussionState,
  resetDraftDiscussions,
} from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";

const { stub } = makeStubs(invokeMock);

const savedBodies = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_draft_file_contents")
    .map((c) => {
      const { path, body } = c[1] as { path: string; body: string };
      return { path, body };
    });

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // DCR-FR-25: a candidate buffer outlives every surface by design, so it
  // carries from one test into the next unless a suite drops it.
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// NAW-FR-36, NAW-FR-38, DHS-FR-07, NAW-FR-35, NAW-FR-42, DCR-FR-12, DCR-FR-13, DCR-FR-24 / NAW-FR-30, DCR-FR-01, DCR-FR-22 — a change proposed to one of the draft's files
// ---------------------------------------------------------------------------

describe("a proposed change (NAW-FR-35)", () => {
  const PROPOSAL = {
    id: "p1",
    draftId: "d1",
    path: PROMPT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "The Intent buries what the tab is.",
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
  };

  /** DCR-FR-05: the one change the proposal above holds, and where it lands. */
  const HUNKS = {
    hunks: [
      {
        id: "h1",
        kind: "replace",
        revision: 0,
        before: "the original text",
        after: "the accepted text",
        anchor: { lead: "", trail: "", hint_start: 0, hint_end: 17 },
      },
    ],
    resolutions: [{ kind: "resolved", start: 0, end: 17 }],
    checksum: "h1",
    legacy: false,
  };

  beforeEach(() => {
    resetDraftProposals();
    resetDraftDiscussions();
    resetProposalHunks();
    hunkCandidateBuffers.clear();
  });

  afterEach(() => {
    act(() => closeReview());
    resetDraftProposals();
  });

  it("marks the tab while a proposal is undecided, whichever file is selected", async () => {
    stub({ proposals: [PROPOSAL] });
    renderWorkspace();

    // The marker is about the draft rather than the selection, and the file the
    // proposal names is not the one the tab opens on.
    const marker = await screen.findByTestId("draft-pending-proposal");
    expect(marker).toHaveTextContent(/proposed change/i);
  });

  it("renders no marker when the draft carries no undecided proposal", async () => {
    stub({ proposals: [{ ...PROPOSAL, state: "accepted" }] });
    renderWorkspace();

    await screen.findByText("artifact-window");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "list_draft_change_proposals",
        expect.anything(),
      ),
    );
    expect(screen.queryByTestId("draft-pending-proposal")).toBeNull();
  });

  it("DCR-FR-01, DCR-FR-03, DCR-FR-11: the marker takes the author to the first change still to decide, in the document", async () => {
    stub({ proposals: [PROPOSAL], hunks: HUNKS });
    renderWorkspace();

    // DCR-FR-11: the review bar is above the page from the moment the tab knows
    // there is a proposal — the author activates nothing to get it.
    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    expect(bar).toHaveTextContent(/reviewing change 1 of 1/);
    expect(within(bar).getByRole("button", { name: "Accept all" })).toBeEnabled();
    expect(within(bar).getByRole("button", { name: "Reject all" })).toBeEnabled();

    // DCR-FR-01: no modal and no scrim over the tab. Asserted as *no dialog and
    // no scrim of any kind*, because a test naming the retired component's own
    // id could not fail — including against a modal reintroduced under another
    // name, which is the thing worth guarding.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector('[class*="scrim"]')).toBeNull();
    expect(screen.getByText("artifact-window")).toBeInTheDocument();

    // DCR-FR-03: the marker takes the author to the first change still to
    // decide — which is a position in the document, not a surface that opens.
    await userEvent.click(await screen.findByTestId("draft-pending-proposal"));
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h1");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("DCR-FR-03, DCR-FR-08: an arriving proposal enters review in place, and a decided one opens nothing", async () => {
    // The author asked the agent and is waiting for the answer; the answer
    // appears where the draft is, with nothing for them to activate.
    stub({ proposals: [], hunks: HUNKS });
    renderWorkspace();
    await screen.findByText("artifact-window");
    expect(
      screen.queryByRole("region", { name: "Proposed changes" }),
    ).toBeNull();
    // DCR-FR-08: a draft holding no proposal costs no read of any changes.
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "load_draft_change_proposal_hunks",
      ),
    ).toBe(false);

    act(() => noteProposalChanged({ draftId: "d1", proposal: PROPOSAL as never }));

    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    expect(bar).toHaveTextContent(/change 1 of 1/);
    expect(screen.getByTestId("draft-pending-proposal")).toBeInTheDocument();
    // DCR-FR-01: still nothing opened over the tab.
    expect(screen.queryByRole("dialog")).toBeNull();

    // DCR-FR-17 / DCR-FR-21: resolved elsewhere, the document is left
    // undecorated and the review bar is gone. Nothing interrupts the author:
    // they decided it.
    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: { ...PROPOSAL, state: "accepted" } as never,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("draft-pending-proposal")).toBeNull(),
    );
    expect(
      screen.queryByRole("region", { name: "Proposed changes" }),
    ).toBeNull();

    // And one that was never seen at all, arriving already decided, is the same.
    act(() =>
      noteProposalChanged({
        draftId: "d1",
        proposal: { ...PROPOSAL, id: "p9", state: "rejected" } as never,
      }),
    );
    expect(
      screen.queryByRole("region", { name: "Proposed changes" }),
    ).toBeNull();
  });

  // DCR-FR-13 / NAW-FR-13, NAW-FR-42. The whole point of deciding a change inside
  // the document is that the document is already showing the passage — so it has
  // to be showing the accepted text the moment the decision lands. Nothing
  // watches a draft file for external change, and the Editor mounted on it has
  // no reason to load again, so the acceptance is the only thing that can put
  // the new bytes on screen.
  it("DCR-FR-13, NAW-FR-13, NAW-FR-42: accepting one change writes the pending buffer first and shows the accepted text", async () => {
    // The author has an unsaved edit in the prompt when they accept. The buffer
    // is written first, the backend rewrites the file, and the surface reads it
    // again — all of it while the tab stays open on the same document.
    let body = "the original text";
    let checksum = "c1";
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "open_draft":
          return draft();
        case "list_draft_history":
          return freshHistory();
        case "load_draft_file_contents":
          return { body, checksum };
        case "save_draft_file_contents":
          return { checksum: "c1-mine" };
        case "list_draft_change_proposals":
          return [PROPOSAL];
        case "load_draft_change_proposal_hunks":
          return HUNKS;
        case "accept_draft_change_hunk":
          // The backend wrote the file, so the next read answers differently.
          body = "the accepted text";
          checksum = "c2";
          return {
            proposal: {
              ...PROPOSAL,
              state: "accepted",
              counts: { pending: 0, accepted: 1, rejected: 0, discussing: 0 },
              ledger: [
                { id: "h1", kind: "replace", state: "accepted", edited: false, revision: 0 },
              ],
              decidedAt: "2026-01-02T00:00:00Z",
            },
            commentId: "decision-1",
            originKind: "draft_discussion",
          };
        case "dispatch_agent_turn":
          return { id: "turn-1", state: "running" };
        default:
          return undefined;
      }
    });
    const { drafts } = renderWorkspace();
    const key = drafts.key("d1", PROMPT);
    await waitFor(() =>
      expect(drafts.docs.get(key)?.buffer).toBe("the original text"),
    );
    // The real editing surface, on the prompt the proposal names, in the mode
    // whose text a test can read.
    const source = await editSource("the original text, half rewritten");

    // DCR-FR-11 / DCR-FR-CXZG: accepted from the review bar, which is above the
    // page rather than in a surface that had to be opened.
    const bar = await screen.findByRole("region", { name: "Proposed changes" });
    const loadsBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_draft_file_contents",
    ).length;
    await userEvent.click(
      within(bar).getByRole("button", { name: "Accept all" }),
    );

    // DCR-FR-13: one change, accepted on its own.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "accept_draft_change_hunk"),
      ).toHaveLength(1),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "accept_draft_change_hunk")![1],
    ).toEqual({ proposalId: "p1", hunkId: "h1", feedback: null });

    // The surface itself, not the record behind it: this is the whole of what
    // the author sees.
    await waitFor(() => expect(source.value).toBe("the accepted text"));
    // NAW-FR-13: their unsaved edit went to disk before the acceptance did,
    // rather than being lost to it.
    expect(savedBodies()).toContainEqual({
      path: PROMPT,
      body: "the original text, half rewritten",
    });
    // Read again rather than patched in place: only the backend knows what the
    // prompt now says.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_draft_file_contents")
        .length,
    ).toBe(loadsBefore + 1);
    expect(drafts.docs.get(key)?.baseline).toBe("c2");
  });

});
