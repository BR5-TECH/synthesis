import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import type { AgentGroup, AgentTurn, ConversationOrigin } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  agentGroupSwitchOn,
  dispatchComment,
  refreshAgentGroupSwitch,
  resetAgentGroupSwitch,
} from "./agentGroupSwitch";
import { pendingContributionsOf } from "./activeAgents";
import { activityStatus } from "./agentActivity";
import { isQuietGroupTurn } from "./turnActivity";

const origin = { discussionId: "t1" } as unknown as ConversationOrigin;

function turn(over: Partial<AgentTurn> & { id: string; nickname: string }): AgentTurn {
  return {
    agentId: over.nickname,
    origin: { discussionId: "t1" } as ConversationOrigin,
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
    imagesOmitted: false,
    ...over,
  } as AgentTurn;
}

let enabled: boolean;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetAgentGroupSwitch();
  invokeMock.mockReset();
  enabled = false;
  invokeMock.mockImplementation((cmd: string, args?: { nicknames?: string[]; nickname?: string }) => {
    if (cmd === "load_agent_group_settings") {
      return Promise.resolve({ enabled, joinTimeoutSecs: 60 });
    }
    if (cmd === "dispatch_agent_turn") {
      return Promise.resolve(turn({ id: `turn-${args?.nickname}`, nickname: args?.nickname ?? "" }));
    }
    if (cmd === "dispatch_agent_group") {
      const group: AgentGroup = {
        mode: "synthesis",
        groupId: "group-1",
        driver: "arch",
        turns: (args?.nicknames ?? []).map((n) => turn({ id: `turn-${n}`, nickname: n })),
      };
      return Promise.resolve(group);
    }
    return otherCommand(cmd);
  });
});

afterEach(() => {
  resetAgentGroupSwitch();
});

describe("the agent group switch", () => {
  it("CTA-FR-QOLD: the switch is off until a read says otherwise, and a failed read leaves it off", async () => {
    expect(agentGroupSwitchOn()).toBe(false);
    invokeMock.mockImplementation((cmd: string) => otherCommand(cmd));
    await refreshAgentGroupSwitch();
    expect(agentGroupSwitchOn()).toBe(false);
  });

  it("CTA-FR-QOLD: with the switch off a comment is one dispatch for each agent", async () => {
    await refreshAgentGroupSwitch();
    const seen: string[] = [];
    dispatchComment(
      { nicknames: ["arch", "sec"], origin, triggerCommentId: "c1" },
      { onTurn: (t) => seen.push(t.nickname), onError: () => seen.push("error") },
    );
    await vi.waitFor(() => expect(seen.sort()).toEqual(["arch", "sec"]));
    expect(calls("dispatch_agent_turn")).toHaveLength(2);
    expect(calls("dispatch_agent_group")).toHaveLength(0);
  });

  it("CTA-FR-UGBA, AGT-FR-CIOK: with the switch on a comment is one group dispatch of the tagged agents, whose turns are handed on", async () => {
    enabled = true;
    await refreshAgentGroupSwitch();
    expect(agentGroupSwitchOn()).toBe(true);
    const seen: string[] = [];
    dispatchComment(
      { nicknames: ["arch", "sec"], origin, triggerCommentId: "c1" },
      { onTurn: (t) => seen.push(t.nickname), onError: () => seen.push("error") },
    );
    await vi.waitFor(() => expect(seen).toEqual(["arch", "sec"]));
    expect(calls("dispatch_agent_group")).toHaveLength(1);
    expect(calls("dispatch_agent_group")[0][1]).toMatchObject({
      nicknames: ["arch", "sec"],
      triggerCommentId: "c1",
    });
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("CTA-FR-UGBA: a refused group dispatch reports the refusal once, naming no one agent", async () => {
    enabled = true;
    await refreshAgentGroupSwitch();
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "dispatch_agent_group" ? Promise.reject("agent_unavailable") : otherCommand(cmd),
    );
    const errors: Array<[unknown, string | null]> = [];
    dispatchComment(
      { nicknames: ["arch"], origin, triggerCommentId: "c1" },
      { onTurn: () => {}, onError: (e, nickname) => errors.push([e, nickname]) },
    );
    await vi.waitFor(() => expect(errors).toEqual([["agent_unavailable", null]]));
  });
});

describe("the rows of an agent group", () => {
  const advisor = turn({
    id: "turn-sec",
    nickname: "sec",
    group: { groupId: "g", role: "advisor", skipped: null },
  });
  const meta = turn({
    id: "turn-arch",
    nickname: "arch",
    group: { groupId: "g", role: "meta", skipped: null },
    activeToolCalls: [{ id: "call-1", tool: "search_drafts", activationSeq: 1, advisor: "sec" }],
  });

  it("CTA-FR-QXXR: an advisor turn has no pending contribution, and the Meta turn keeps its own", () => {
    const rows = pendingContributionsOf([meta, advisor]);
    expect(rows.map((row) => row.turn.nickname)).toEqual(["arch"]);
  });

  it("CTA-FR-QXXR: the Meta row names the advisor that requested the tool call it shows", () => {
    expect(activityStatus(meta)).toBe("@sec: Searching drafts…");
    expect(activityStatus({ ...meta, activeToolCalls: [{ id: "c", tool: "search_drafts", activationSeq: 1 }] })).toBe(
      "Searching drafts…",
    );
  });

  it("CTA-FR-QXXR, CTA-FR-LZDE: an advisor, a timed out member, and a cancelled member end quietly; a failed member does not", () => {
    expect(isQuietGroupTurn({ ...advisor, state: "failed" })).toBe(true);
    const member = (skipped: "timed_out" | "failed" | "cancelled") =>
      turn({ id: "m", nickname: "m", group: { groupId: "g", role: "participant", skipped } });
    expect(isQuietGroupTurn(member("timed_out"))).toBe(true);
    expect(isQuietGroupTurn(member("cancelled"))).toBe(true);
    expect(isQuietGroupTurn(member("failed"))).toBe(false);
    expect(isQuietGroupTurn(turn({ id: "x", nickname: "x" }))).toBe(false);
  });
});
