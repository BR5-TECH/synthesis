/**
 * A comment to an agent whose turn is running (`CTA-comment-agent-turns.md`
 * CTA-FR-ZVKL, CTA-FR-BEHA, CTA-FR-ODDY, CTA-FR-BXNF).
 *
 * The surface dispatches as for any agent. The backend gives the comment to
 * the running turn and answers with that turn, so the discussion still holds
 * one turn for the agent and shows no error.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logError: vi.fn(),
  logWarn: vi.fn(),
}));

import { dispatchTurnsFor } from "./useConversationSession";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
  patchDiscussionSession,
} from "../state/discussionSession";
import type { AgentTurn, Comment, Discussion, ProjectAgent } from "../types";
import { noteDiscussionOrigin } from "../test/origins";

const human = { kind: "human", login: "raver119" } as const;

function comment(id: string, body: string): Comment {
  return { id, author: human, body, quotes: [], attachments: [], createdAt: "2026-02-01T00:00:00Z" };
}

function discussion(...bodies: string[]): Discussion {
  return {
    id: "d1",
    target: { kind: "note", noteId: "n1" },
    fragmentTarget: null,
    comments: bodies.map((b, i) => comment(`c${i + 1}`, b)),
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

const enrolled = (nickname: string) =>
  ({
    agent: { id: nickname, nickname, model: "m", title: "t", instructions: "" },
    availability: "ready",
  }) as unknown as ProjectAgent;
const AGENTS = [enrolled("arch"), enrolled("sec")];

const archRunning: AgentTurn = {
  id: "turn-arch",
  agentId: "arch",
  nickname: "arch",
  origin: noteDiscussionOrigin("d1", "n1"),
  triggerCommentId: "c1",
  state: "running",
  failure: null,
  retryPermitted: false,
  activeToolCalls: [],
  imagesOmitted: false,
  startedAt: "2026-02-01T00:00:00Z",
  endedAt: null,
};

beforeEach(() => {
  invokeMock.mockReset();
  clearAllDiscussionSessions();
  patchDiscussionSession("d1", { scrollTop: 1 });
});

describe("CTA-FR-BEHA / CTA-FR-ODDY: a comment for a running turn steers it", () => {
  it("dispatches for the working agent as for any other, and holds one turn for it with no error", async () => {
    mergeDiscussionTurn("d1", archRunning);
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd !== "dispatch_agent_turn") throw new Error(`unexpected ${cmd}`);
      const { nickname, triggerCommentId } = args as { nickname: string; triggerCommentId: string };
      // The backend answers with the turn that took the comment.
      if (nickname === "arch") return { ...archRunning, deadlineAt: "2026-02-01T00:05:00Z" };
      return { ...archRunning, id: "turn-sec", agentId: "sec", nickname, triggerCommentId };
    });

    // CTA-FR-ZVKL: the follow-up reaches both, without waiting for arch's turn.
    dispatchTurnsFor(
      discussion("@arch thoughts?", "@arch @sec focus on the tests"),
      "@arch @sec focus on the tests",
      AGENTS,
    );
    await vi.waitFor(() => expect(getDiscussionSession("d1").turns).toHaveLength(2));

    const nicknames = invokeMock.mock.calls
      .filter(([cmd]) => cmd === "dispatch_agent_turn")
      .map(([, a]) => (a as { nickname: string }).nickname);
    expect(nicknames).toEqual(["arch", "sec"]);
    const held = getDiscussionSession("d1").turns;
    expect(held.filter((t) => t.agentId === "arch").map((t) => t.id)).toEqual(["turn-arch"]);
    // CVP-FR-XZTZ's input: the steered turn's new deadline replaces the old.
    expect(held.find((t) => t.id === "turn-arch")?.deadlineAt).toBe("2026-02-01T00:05:00Z");
    expect(getDiscussionSession("d1").turnFailure).toBeUndefined();
    expect(getDiscussionSession("d1").error).toBeUndefined();
  });
});
