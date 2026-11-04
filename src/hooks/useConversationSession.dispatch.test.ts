import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const logWarn = vi.fn();
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logError: vi.fn(),
  logWarn: (...a: unknown[]) => logWarn(...a),
}));

import { dispatchTurnsFor, postReply } from "./useConversationSession";
import { beginSpeechPost, finishSpeechPost, resetSpeechPostsForTest } from "../state/speechPosts";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
} from "../state/discussionSession";
import type { Comment, Discussion, ProjectAgent } from "../types";
import { expectBackendOrigin, noteDiscussionOrigin } from "../test/origins";

const human = { kind: "human", login: "raver119" } as const;

function comment(id: string, body: string): Comment {
  return { id, author: human, body, quotes: [], attachments: [], createdAt: "2026-02-01T00:00:00Z" };
}

function discussion(body: string): Discussion {
  return {
    id: "d1",
    target: { kind: "note", noteId: "n1" },
    fragmentTarget: null,
    comments: [comment("c1", body)],
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

/** `dispatch_agent_turn` refuses the nicknames in `refused`, as the backend would. */
function serve(refused: Set<string>) {
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd !== "dispatch_agent_turn") return undefined;
    const { nickname, origin, triggerCommentId } = args as {
      nickname: string;
      origin: unknown;
      triggerCommentId: string;
    };
    if (refused.has(nickname)) throw "agent_not_found";
    return {
      id: `turn-${nickname}`,
      agentId: nickname,
      nickname,
      origin: expectBackendOrigin(origin),
      triggerCommentId,
      state: "running",
      failure: null,
      retryPermitted: false,
      activeToolCalls: [],
      startedAt: "2026-02-01T00:00:00Z",
      endedAt: null,
    };
  });
}

const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  invokeMock.mockReset();
  logWarn.mockReset();
  clearAllDiscussionSessions();
  resetSpeechPostsForTest();
});

describe("the comment that ends a speech", () => {
  it("CTA-FR-SZOC, CTA-FR-QWLB, VOM-FR-MVLA: skips the agents the speech reached and gives the comment id", async () => {
    serve(new Set());
    beginSpeechPost("d1", ["arch"], "@arch @sec look");
    dispatchTurnsFor(discussion("@arch @sec look"), "@arch @sec look", AGENTS);
    expect(finishSpeechPost()).toBe("c1");
    await settle();
    const asked = invokeMock.mock.calls
      .filter(([cmd]) => cmd === "dispatch_agent_turn")
      .map(([, args]) => (args as { nickname: string }).nickname);
    expect(asked).toEqual(["sec"]);
  });

  it("CTA-FR-SZOC: a typed comment is dispatched as before", async () => {
    serve(new Set());
    dispatchTurnsFor(discussion("@arch look"), "@arch look", AGENTS);
    await settle();
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "dispatch_agent_turn")).toHaveLength(1);
  });

  it("CTA-FR-SZOC: a typed comment during a speech post is dispatched to every agent it names", async () => {
    serve(new Set());
    beginSpeechPost("d1", ["arch"], "@arch the spoken words");
    dispatchTurnsFor(discussion("@arch a typed comment"), "@arch a typed comment", AGENTS);
    await settle();
    const asked = invokeMock.mock.calls
      .filter(([cmd]) => cmd === "dispatch_agent_turn")
      .map(([, args]) => (args as { nickname: string }).nickname);
    expect(asked).toEqual(["arch"]);
    // The speech comment itself still claims its post afterwards.
    dispatchTurnsFor(discussion("@arch the spoken words"), "@arch the spoken words", AGENTS);
    expect(finishSpeechPost()).toBe("c1");
  });

  it("CTA-FR-SZOC: a speech post of another discussion skips nobody here", async () => {
    serve(new Set());
    beginSpeechPost("d9", ["arch"], "@arch look");
    dispatchTurnsFor(discussion("@arch look"), "@arch look", AGENTS);
    await settle();
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "dispatch_agent_turn")).toHaveLength(1);
    expect(finishSpeechPost()).toBeNull();
  });
});

describe("dispatching from the shared surface (CTA-FR-QUXJ, CTA-FR-UUXA)", () => {
  it("AGC-FR-05, CVP-FR-47: sends the origin the backend reads and stands the pending turn", async () => {
    serve(new Set());
    dispatchTurnsFor(discussion("@arch thoughts?"), "@arch thoughts?", AGENTS);
    await settle();

    const [call] = invokeMock.mock.calls.filter((c) => c[0] === "dispatch_agent_turn");
    expect(call[1]).toEqual({
      nickname: "arch",
      origin: noteDiscussionOrigin("d1", "n1"),
      triggerCommentId: "c1",
    });
    expect(getDiscussionSession("d1").turns.map((t) => t.id)).toEqual(["turn-arch"]);
  });

  it("CTA-FR-UUXA: a refusal stands beside another agent's acceptance, and the next message retires it", async () => {
    serve(new Set(["arch"]));
    dispatchTurnsFor(discussion("@arch @sec ?"), "@arch @sec ?", AGENTS);
    await settle();
    expect(getDiscussionSession("d1").turnFailure).toBe("agent_not_found");
    expect(logWarn).toHaveBeenCalledWith(
      ["ai", "frontend"],
      "an agent turn could not be dispatched",
      { threadId: "d1", nickname: "arch", failure: "agent_not_found" },
    );

    serve(new Set());
    dispatchTurnsFor(discussion("@sec only"), "@sec only", AGENTS);
    expect(getDiscussionSession("d1").turnFailure).toBeUndefined();
  });

  it("CTA-FR-UUXA: an argument error is shown but logged as unexpected", async () => {
    const raw = 'invalid args `origin`: unknown variant "a private passage"';
    invokeMock.mockRejectedValue(new Error(raw));
    dispatchTurnsFor(discussion("@arch ?"), "@arch ?", AGENTS);
    await settle();

    expect(getDiscussionSession("d1").turnFailure).toBe(raw);
    expect(JSON.stringify(logWarn.mock.calls)).not.toContain("a private passage");
    expect(logWarn.mock.calls[0][2]).toMatchObject({ failure: "unexpected" });
  });
});

describe("the transcript of a translated speech comment", () => {
  it("VOM-FR-QWMK, CMS-FR-UYVK: postReply sends the transcript of the speech body, and of no other", async () => {
    resetSpeechPostsForTest();
    clearAllDiscussionSessions();
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "add_comment") return discussion((args as { body: string }).body);
      return undefined;
    });
    beginSpeechPost("d1", [], "held text\nLook here.", "Mira aquí.");
    await postReply({ noteId: "n1" }, "d1", "held text\nLook here.", [], [], []);
    expect(invokeMock).toHaveBeenCalledWith(
      "add_comment",
      expect.objectContaining({ body: "held text\nLook here.", spokenOriginal: "Mira aquí." }),
    );
    finishSpeechPost();
    invokeMock.mockClear();
    await postReply({ noteId: "n1" }, "d1", "A typed comment.", [], [], []);
    const [, args] = invokeMock.mock.calls.find(([cmd]) => cmd === "add_comment")!;
    expect((args as { spokenOriginal?: string }).spokenOriginal).toBeUndefined();
  });
});
