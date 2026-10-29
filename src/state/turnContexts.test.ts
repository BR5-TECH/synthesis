import { beforeEach, describe, expect, it } from "vitest";

import {
  clearTurnContexts,
  getTurnContext,
  noteTurnContext,
  resetTurnContextsForTest,
} from "./turnContexts";
import { clearAllDiscussionSessions, followAgentTurn } from "./discussionSession";
import type { AgentTurn, TurnContext } from "../types";

const CONTEXT: TurnContext = {
  window: 1000,
  inputTokens: 100,
  split: { instructions: 1, document: 2, history: 3, currentComment: 4, toolResults: 5 },
  document: { lines: 1, characters: 8, estimatedTokens: 2 },
  limitTokens: 300,
  aboveLimit: false,
};

function turn(over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id: "t1",
    agentId: "a1",
    nickname: "arch",
    origin: { discussionId: "d1", target: { kind: "draft", draftId: "x" } },
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
    imagesOmitted: false,
    context: CONTEXT,
    ...over,
  };
}

beforeEach(() => {
  resetTurnContextsForTest();
  clearAllDiscussionSessions();
});

describe("the latest turn's context, per discussion", () => {
  it("DDS-FR-KXWQ: keeps the figures of a turn event, with its turn and agent", () => {
    noteTurnContext(turn());
    expect(getTurnContext("d1")).toMatchObject({ turnId: "t1", agentId: "a1", context: CONTEXT });
  });

  it("DDS-FR-KXWQ: an event without figures changes nothing", () => {
    noteTurnContext(turn());
    noteTurnContext(turn({ id: "t2", startedAt: "2026-01-02T00:00:00Z", context: null }));
    expect(getTurnContext("d1")?.turnId).toBe("t1");
  });

  it("DDS-FR-KXWQ, DDS-FR-TSPL: a later turn replaces an earlier one, and not the other way", () => {
    noteTurnContext(turn({ id: "t2", agentId: "a2", startedAt: "2026-01-02T00:00:00Z" }));
    noteTurnContext(turn({ id: "t1", startedAt: "2026-01-01T00:00:00Z" }));
    expect(getTurnContext("d1")).toMatchObject({ turnId: "t2", agentId: "a2" });
  });

  it("DDS-FR-KXWQ: the shell's turn listener keeps figures for a discussion no surface holds", () => {
    followAgentTurn(turn({ state: "delivered", endedAt: "2026-01-01T00:00:09Z" }));
    expect(getTurnContext("d1")?.context).toBe(CONTEXT);
  });

  it("DDS-FR-KXWQ: a project change discards every figure", () => {
    noteTurnContext(turn());
    clearAllDiscussionSessions();
    expect(getTurnContext("d1")).toBeNull();
    noteTurnContext(turn());
    clearTurnContexts();
    expect(getTurnContext("d1")).toBeNull();
  });
});
