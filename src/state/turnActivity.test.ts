/**
 * The tool list and the activity rows of agent turns
 * (`CTA-comment-agent-turns.md` CTA-FR-BJWN, CTA-FR-ZBIB, CTA-FR-QMWO,
 * CTA-FR-ZOGA, CTA-FR-GCKO, CTA-FR-MNPS).
 */
import { beforeEach, describe, expect, it } from "vitest";

import {
  activityRowLabel,
  activityRowOf,
  pendingToolCalls,
  withEndedTurn,
} from "./turnActivity";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
  patchDiscussionSession,
  removeDiscussionTurn,
  setDiscussionTurns,
} from "./discussionSession";
import type { AgentTurn, TurnToolCall } from "../types";
import { artifactDiscussionOrigin } from "../test/origins";

const call = (seq: number, tool = "read_file", keyInput: string | null = null): TurnToolCall => ({
  id: `call-${seq}`,
  tool,
  activationSeq: seq,
  state: "done",
  keyInput,
});

function turn(id: string, over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id,
    agentId: "agent-arch",
    nickname: "arch",
    origin: artifactDiscussionOrigin("d1", "a.md"),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: new Date(Date.UTC(2026, 1, 1, 0, 0, Number(id.slice(1)))).toISOString(),
    endedAt: null,
    activeToolCalls: [],
    imagesOmitted: false,
    toolCalls: [],
    ...over,
  };
}

beforeEach(() => {
  clearAllDiscussionSessions();
  // A session must exist for the store to fold a turn into it.
  patchDiscussionSession("d1", { scrollTop: 1 });
});

describe("CTA-FR-BJWN: the tool list", () => {
  it("lists the calls of the named turns, the oldest turn first, each in activation order", () => {
    const older = turn("t1", { toolCalls: [call(2, "b"), call(1, "a")] });
    const newer = turn("t4", { toolCalls: [call(1, "c")] });
    const other = turn("t2", { toolCalls: [call(1, "z")] });
    expect(pendingToolCalls([newer, other, older], ["t4", "t1"]).map((c) => c.tool)).toEqual([
      "a",
      "b",
      "c",
    ]);
  });
});

describe("CTA-FR-ZBIB / CTA-FR-QMWO / CTA-FR-ZOGA: activity rows", () => {
  it("a turn with no tool call leaves no row", () => {
    expect(activityRowOf(turn("t1"))).toBeNull();
  });

  it("names the agent and the count, one tool or several", () => {
    expect(activityRowLabel({ nickname: "arch", calls: [call(1)] })).toBe("@arch called 1 tool");
    expect(activityRowLabel({ nickname: "arch", calls: [call(1), call(2)] })).toBe(
      "@arch called 2 tools",
    );
  });

  it("adds rows in the order the turns ended, and a second record of one turn adds none", () => {
    const a = turn("t1", { toolCalls: [call(1)] });
    const b = turn("t2", { toolCalls: [call(1)] });
    const once = withEndedTurn(withEndedTurn([], b), a);
    expect(once.map((r) => r.turnId)).toEqual(["t2", "t1"]);
    expect(withEndedTurn(once, a)).toBe(once);
  });
});

describe("CTA-FR-GCKO / CTA-FR-MNPS: the rows in the session store", () => {
  const ended = (t: AgentTurn, state: AgentTurn["state"] = "delivered"): AgentTurn => ({
    ...t,
    state,
    endedAt: "2026-02-01T00:01:00Z",
  });

  it("a terminal record adds the row, and a turn that starts removes every row", () => {
    const a = turn("t1", { toolCalls: [call(1)] });
    mergeDiscussionTurn("d1", a);
    mergeDiscussionTurn("d1", ended(a));
    // The shell and the surface both fold each event.
    mergeDiscussionTurn("d1", ended(a));
    expect(getDiscussionSession("d1").activityRows.map((r) => r.turnId)).toEqual(["t1"]);
    // A later event of a turn that already started removes nothing.
    const b = turn("t2", { toolCalls: [call(1)] });
    setDiscussionTurns("d1", [b]);
    mergeDiscussionTurn("d1", b);
    expect(getDiscussionSession("d1").activityRows).toHaveLength(1);
    mergeDiscussionTurn("d1", turn("t3"));
    expect(getDiscussionSession("d1").activityRows).toHaveLength(0);
    // A late terminal record of a turn already folded adds nothing back.
    mergeDiscussionTurn("d1", ended(a));
    expect(getDiscussionSession("d1").activityRows).toHaveLength(0);
  });

  it("a cancel the surface made first still leaves the row of the cancelled turn", () => {
    const a = turn("t1", { toolCalls: [call(1)] });
    mergeDiscussionTurn("d1", a);
    removeDiscussionTurn("d1", "t1");
    mergeDiscussionTurn("d1", ended(a, "cancelled"));
    expect(getDiscussionSession("d1").activityRows.map((r) => r.turnId)).toEqual(["t1"]);
  });

  it("a project change discards every row", () => {
    const a = turn("t1", { toolCalls: [call(1)] });
    mergeDiscussionTurn("d1", a);
    mergeDiscussionTurn("d1", ended(a));
    clearAllDiscussionSessions();
    expect(getDiscussionSession("d1").activityRows).toHaveLength(0);
  });
});
