import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The event bus, so a test can deliver `"artifact changed externally"` the way
 * the backend does (PCR-FR-24).
 */
const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    const set = listeners.get(name) ?? new Set();
    set.add(handler);
    listeners.set(name, set);
    return () => set.delete(handler);
  }),
}));

import { PromptChangeReview } from "./PromptChangeReview";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetLogBufferForTest } from "../logging";
import {
  closePromptReview,
  notePromptProposalChanged,
  openPromptReview,
  resetPromptProposals,
} from "../state/promptProposals";
import { resetDiffModes } from "../state/diffModes";
import { promptCandidateBuffers } from "../state/candidateBuffers";
import { EditSessionStore } from "../state/editSessions";
import {
  clearConversationThreads,
  publishThread,
} from "../state/conversationThreads";
import {
  publishProjectAgents,
  resetAgentRegistry,
} from "../state/agentRegistry";
import type { Discussion } from "../types";
import {
  ARTIFACT,
  ROSTER,
  addressedConversation,
  agent,
  conversation,
  proposal,
} from "../test/promptProposalFixtures";

/**
 * EDT-FR-MOQW: the candidate of the prompt change review is a modal surface,
 * so its diagram fences offer no Edit button. The rest of the modal is tested
 * in the other `PromptChangeReview` files.
 */


/**
 * The conversation `"read comment thread"` answers with — the decision path
 * reads it again rather than trusting what this surface was holding, so a test
 * that seeds one has to seed both.
 */
let seeded: Discussion = addressedConversation();

/** Seed the roster and the conversation the routing rule reads. */
function seedConversation(thread: Discussion = addressedConversation()) {
  seeded = thread;
  act(() => {
    publishProjectAgents(ROSTER);
    publishThread(thread);
  });
}

/** The backend as the modal sees it, unless a test overrides a call. */
function stubBackend(over: Record<string, unknown> = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd in over) {
      const value = over[cmd];
      if (value instanceof Error) throw value;
      return value;
    }
    switch (cmd) {
      case "list_prompt_change_proposals":
        return [proposal()];
      case "load_prompt_change_proposal_content":
        return { content: FENCED_PROPOSED, checksum: "cand-1" };
      case "load_artifact_contents_by_id":
        return { body: FENCED_CURRENT, checksum: "base-1" };
      case "save_prompt_change_proposal_candidate":
        return { checksum: "cand-2" };
      case "apply_prompt_change_proposal":
        return {
          proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "decline_prompt_change_proposal":
        return {
          proposal: proposal({ state: "rejected", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "complete_prompt_change_decision":
        return {
          proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
          commentId: "decision-1",
          originKind: "artifact_discussion",
        };
      case "dispatch_agent_turn":
        return { id: "turn-1", state: "running" };
      case "read_discussion":
        return seeded;
      case "list_project_agents":
        return ROSTER;
      case "load_app_preferences":
        return { diffRenderingMode: "rich", diffVisualizationMode: "side_by_side" };
      case "append_log_records":
      case "save_app_preferences":
        return undefined;
      default:
        return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    }
  });
}

const FENCED_CURRENT = "# Review\n\n```mermaid\ngraph TD\nA-->B\n```\n";
const FENCED_PROPOSED = "# Review\n\n```mermaid\ngraph TD\nA-->C\n```\n";

let sessions: EditSessionStore;

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetPromptProposals();
  resetDiffModes();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  promptCandidateBuffers.clear();
  sessions = new EditSessionStore();
  resetAgentRegistry();
  clearConversationThreads();
  stubBackend();
  seedConversation();
});

afterEach(() => {
  cleanup();
  act(() => closePromptReview());
});

describe("diagram fences in the review candidate", () => {
  it("EDT-FR-MOQW, PCR-FR-JPKZ: the editable candidate shows its diagram fence with no Edit button and no View button", async () => {
    render(<PromptChangeReview artifactId={ARTIFACT} sessions={sessions} />);
    await act(async () => {
      openPromptReview("prop-1");
    });
    const review = await screen.findByTestId("prompt-change-review");
    await screen.findByText(ARTIFACT);
    fireEvent.click(screen.getByRole("radio", { name: "Rich" }));
    await waitFor(() =>
      expect(review.querySelector(".ProseMirror .mermaid-block")).not.toBeNull(),
    );
    // The candidate is editable, so the fence has its view toggle.
    await waitFor(() => expect(review.querySelector(".ProseMirror .mermaid-toggle")).not.toBeNull());
    expect(review.querySelector(".mermaid-edit")).toBeNull();
    expect(review.querySelector(".mermaid-view")).toBeNull();
    expect(screen.queryByRole("button", { name: "View diagram" })).toBeNull();
  });
});
