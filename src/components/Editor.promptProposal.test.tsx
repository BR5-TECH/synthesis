/**
 * The Editor tab's part in a prompt change review (`EDT-editor.md` EDT-FR-84,
 * EDT-FR-16).
 *
 * What this spec owns is the **hosting** alone: the pending indication in the
 * tab's action cluster, and the modal anchored within the tab rather than in the
 * window. The modal's own behaviour is `PCR-prompt-change-review.md`'s and is
 * tested in `PromptChangeReview.test.tsx`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import {
  closePromptReview,
  notePromptProposalChanged,
  resetPromptProposals,
} from "../state/promptProposals";
import { promptCandidateBuffers } from "../state/candidateBuffers";
import { recordEdit, sealBurst } from "../state/editHistory";
import { resetDiffModes } from "../state/diffModes";
import type { PromptChangeProposal } from "../types";

const ARTIFACT = "prompts/review.md";
const BODY = "# Review\n\nThe original line.\n";

function proposal(over: Partial<PromptChangeProposal> = {}): PromptChangeProposal {
  return {
    id: "prop-1",
    artifactId: ARTIFACT,
    path: ARTIFACT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "The instructions bury the important step.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

let proposals: PromptChangeProposal[] = [];

function wire() {
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: BODY, checksum: "ck-1" };
      case "list_prompt_change_proposals":
        return proposals;
      case "load_prompt_change_proposal_content":
        return { content: "# Review\n\nBetter.\n", checksum: "cand-1" };
      case "list_discussions":
      case "list_agent_turns":
      case "list_recoverable_agent_turn_failures":
      case "list_agents":
        return [];
      case "load_app_preferences":
        return {};
      default:
        return null;
    }
  });
}

function renderTab(sessions: EditSessionStore) {
  return render(
    <Editor
      artifactId={ARTIFACT}
      artifactName="review.md"
      sessions={sessions}
      showComments={false}
      showActions={false}
    />,
  );
}

let sessions: EditSessionStore;

beforeEach(() => {
  invokeMock.mockReset();
  proposals = [];
  resetPromptProposals();
  resetDiffModes();
  promptCandidateBuffers.clear();
  sessions = new EditSessionStore();
  wire();
});

afterEach(() => {
  cleanup();
  act(() => closePromptReview());
});

// ---------------------------------------------------------------------------
// EDT-FR-84 — the indication and the hosting (EDT-FR-16, EDT-FR-84)
// ---------------------------------------------------------------------------

describe("the pending indication (EDT-FR-16, PCR-FR-16)", () => {
  it("renders no indication for an artifact carrying no pending proposal", async () => {
    renderTab(sessions);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "list_prompt_change_proposals",
        expect.anything(),
      ),
    );
    expect(screen.queryByTestId("editor-prompt-proposal-indication")).toBeNull();
  });

  it("carries the indication beside the cluster's other controls, and no Save", async () => {
    proposals = [proposal()];
    renderTab(sessions);

    const indication = await screen.findByTestId(
      "editor-prompt-proposal-indication",
    );
    // EDT-FR-16: a graphical icon control in the tab's action cluster.
    expect(indication.closest(".editor__actions")).not.toBeNull();
    expect(indication).toHaveAccessibleName(
      "Review the proposed change to this prompt",
    );
    expect(screen.queryByRole("button", { name: /^save$/i })).toBeNull();
  });

  it("opens the review anchored within the tab when the indication is activated", async () => {
    proposals = [proposal()];
    const { container } = renderTab(sessions);
    const indication = await screen.findByTestId(
      "editor-prompt-proposal-indication",
    );

    await act(async () => {
      fireEvent.click(indication);
    });

    const modal = await screen.findByTestId("prompt-change-review");
    // EDT-FR-84 / PCR-FR-01: anchored within this tab rather than in the
    // window, so its scrim covers the tab's own bounds and no more.
    expect(container.querySelector(".editor-tab")?.contains(modal)).toBe(true);
    // …and the tab's own chrome is still there around it.
    expect(container.querySelector(".editor__actions")).not.toBeNull();
  });

  it("clears the indication the moment the proposal is decided", async () => {
    proposals = [proposal()];
    renderTab(sessions);
    await screen.findByTestId("editor-prompt-proposal-indication");

    await act(async () => {
      notePromptProposalChanged({
        artifactId: ARTIFACT,
        proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
      });
    });

    // PCR-FR-26: without the author refreshing anything.
    await waitFor(() =>
      expect(screen.queryByTestId("editor-prompt-proposal-indication")).toBeNull(),
    );
  });

  it("leaves an undo issued in the review to the candidate's own history", async () => {
    // EDT-FR-84, EDT-FR-22, PCR-FR-22 / PCR-FR-20: the candidate is **not** this artifact's editing
    // session, so one undo issued in the tab and one issued in the review
    // reverse two different edits.
    //
    // The defect this pins: the Editor claims ⌘Z in the **capture** phase on the
    // tab's own root, which is an ancestor of the modal (PCR-FR-01 anchors it
    // there) — so without a guard it ran first, prevented the default, and
    // undid the artifact while the author was looking at the candidate.
    proposals = [proposal()];
    renderTab(sessions);
    const indication = await screen.findByTestId(
      "editor-prompt-proposal-indication",
    );
    await act(async () => {
      fireEvent.click(indication);
    });
    await screen.findByTestId("prompt-change-review");

    // The artifact's own buffer holds an edit with a history behind it.
    const session = sessions.ensure(ARTIFACT);
    session.buffer = "# Review\n\nThe author's own edit.\n";
    recordEdit(session.history, session.buffer, "wysiwyg", "body");
    sessions.update(ARTIFACT, { dirty: true });

    // …and so does the candidate.
    await waitFor(() =>
      expect(promptCandidateBuffers.get("prop-1")).toBeDefined(),
    );
    act(() => {
      promptCandidateBuffers.edit("prop-1", "# Review\n\nOne.\n");
      sealBurst(promptCandidateBuffers.get("prop-1")!.history);
      promptCandidateBuffers.edit("prop-1", "# Review\n\nTwo.\n");
    });

    await act(async () => {
      // Dispatched from inside the modal, as a keystroke in the candidate is.
      fireEvent.keyDown(screen.getByRole("dialog"), { key: "z", metaKey: true });
    });

    expect(promptCandidateBuffers.get("prop-1")?.text).toContain("One.");
    expect(sessions.get(ARTIFACT)?.buffer).toBe(
      "# Review\n\nThe author's own edit.\n",
    );
  });

  it("still claims an undo issued in the tab itself", async () => {
    // The guard is about where the event came from and nothing else: the tab's
    // own surface keeps the claim EDT-FR-22 gives it.
    proposals = [proposal()];
    const { container } = renderTab(sessions);
    await screen.findByTestId("editor-prompt-proposal-indication");

    const session = sessions.ensure(ARTIFACT);
    session.buffer = "# Review\n\nThe author's own edit.\n";
    recordEdit(session.history, session.buffer, "wysiwyg", "body");
    sessions.update(ARTIFACT, { dirty: true });

    await act(async () => {
      fireEvent.keyDown(container.querySelector(".editor-tab")!, {
        key: "z",
        metaKey: true,
      });
    });

    expect(sessions.get(ARTIFACT)?.buffer).not.toBe(
      "# Review\n\nThe author's own edit.\n",
    );
  });

  it("gives focus back to the control that opened it", async () => {
    // A dismissal that dropped focus to `<body>` would restart a keyboard
    // user's next Tab from the top of the document rather than from the
    // indication they came in through.
    proposals = [proposal()];
    renderTab(sessions);
    const indication = await screen.findByTestId(
      "editor-prompt-proposal-indication",
    );
    indication.focus();
    await act(async () => {
      fireEvent.click(indication);
    });
    const modal = await screen.findByTestId("prompt-change-review");
    expect(modal.contains(document.activeElement)).toBe(true);

    await act(async () => {
      fireEvent.keyDown(window, { key: "Escape" });
    });

    await waitFor(() =>
      expect(screen.queryByTestId("prompt-change-review")).toBeNull(),
    );
    expect(document.activeElement).toBe(
      screen.getByTestId("editor-prompt-proposal-indication"),
    );
  });

  it("opens the review on itself when a proposal arrives in an open tab", async () => {
    renderTab(sessions);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "list_prompt_change_proposals",
        expect.anything(),
      ),
    );

    await act(async () => {
      notePromptProposalChanged({ artifactId: ARTIFACT, proposal: proposal() });
    });

    // PCR-FR-16: with nothing for the author to activate, and the indication
    // rendered beside it as the way back.
    expect(await screen.findByTestId("prompt-change-review")).toBeInTheDocument();
    expect(
      screen.getByTestId("editor-prompt-proposal-indication"),
    ).toBeInTheDocument();
  });
});
