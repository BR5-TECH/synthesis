/**
 * The outstanding turns the discussion session store holds
 * (`CVP-conversation-presentation.md` CVP-FR-HWTN; `CTA-comment-agent-turns.md`
 * CTA-FR-ZOLW).
 *
 * A turn's dispatch result, its events, and a read of the outstanding turns
 * arrive in no fixed order. These tests put them in the orders that once left a
 * finished turn drawn as pending.
 */
import { describe, expect, it } from "vitest";

import {
  clearAllDiscussionSessions,
  clearDiscussionSession,
  followAgentTurn,
  getDiscussionSession,
  hasTurnEnded,
  mergeDiscussionTurn,
  pendingTurnsOf,
  removeDiscussionTurn,
  setComposerBody,
  setDiscussionTurns,
  upsertDiscussionTurn,
} from "./discussionSession";
import { artifactDiscussionOrigin } from "../test/origins";
import type { AgentTurn, AgentTurnState } from "../types";

function turn(
  id: string,
  state: AgentTurnState = "running",
  discussionId = "d1",
): AgentTurn {
  return {
    id,
    agentId: "agent-arch",
    nickname: "arch",
    origin: artifactDiscussionOrigin(discussionId, "a.md"),
    triggerCommentId: "c1",
    state,
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: state === "running" ? null : "2026-02-01T00:01:00Z",
    activeToolCalls: [],
  } as unknown as AgentTurn;
}

const ids = (key: string) => getDiscussionSession(key).turns.map((t) => t.id);

describe("CVP-FR-HWTN: a turn that ended never draws as pending again", () => {
  it.each(["delivered", "failed", "cancelled"] as const)(
    "a dispatch result that lands after the %s event adds nothing",
    (state) => {
      mergeDiscussionTurn("d1", turn("t1"));
      mergeDiscussionTurn("d1", turn("t1", state));
      upsertDiscussionTurn("d1", turn("t1"));
      expect(ids("d1")).toEqual([]);
      expect(hasTurnEnded("t1")).toBe(true);
    },
  );

  it("a dispatch result for a turn still running is held", () => {
    upsertDiscussionTurn("d1", turn("t1"));
    expect(ids("d1")).toEqual(["t1"]);
    expect(hasTurnEnded("t1")).toBe(false);
  });

  it("a read that was in flight while the turn ended keeps only the live turns", () => {
    mergeDiscussionTurn("d1", turn("t1", "delivered"));
    setDiscussionTurns("d1", [turn("t1"), turn("t2")]);
    expect(ids("d1")).toEqual(["t2"]);
    expect(getDiscussionSession("d1").turnsLoaded).toBe(true);
  });

  it("a running event that arrives after the terminal one is ignored", () => {
    mergeDiscussionTurn("d1", turn("t1"));
    mergeDiscussionTurn("d1", turn("t1", "delivered"));
    const before = getDiscussionSession("d1");
    mergeDiscussionTurn("d1", turn("t1"));
    expect(getDiscussionSession("d1")).toBe(before);
    expect(ids("d1")).toEqual([]);
  });

  it("an optimistic cancel stops a later running record, also for a turn not held", () => {
    removeDiscussionTurn("d1", "t1");
    mergeDiscussionTurn("d1", turn("t1"));
    upsertDiscussionTurn("d1", turn("t1"));
    expect(ids("d1")).toEqual([]);
  });

  it("ending one turn does not drop another", () => {
    mergeDiscussionTurn("d1", turn("t1", "delivered"));
    upsertDiscussionTurn("d1", turn("t2"));
    expect(ids("d1")).toEqual(["t2"]);
  });

  it("awaiting_reply ends the turn but is still held, and renders no placeholder", () => {
    mergeDiscussionTurn("d1", turn("t1"));
    mergeDiscussionTurn("d1", turn("t1", "awaiting_reply"));
    expect(ids("d1")).toEqual(["t1"]);
    expect(hasTurnEnded("t1")).toBe(true);
    expect(pendingTurnsOf(getDiscussionSession("d1").turns)).toEqual([]);
    // A read keeps it: only a stale running record is dropped.
    setDiscussionTurns("d1", [turn("t1", "awaiting_reply")]);
    expect(ids("d1")).toEqual(["t1"]);
  });

  it("closing one discussion keeps the record of ended turns", () => {
    mergeDiscussionTurn("d1", turn("t1", "delivered"));
    clearDiscussionSession("d1");
    upsertDiscussionTurn("d1", turn("t1"));
    expect(ids("d1")).toEqual([]);
  });

  it("clearing every session forgets the ended turns", () => {
    mergeDiscussionTurn("d1", turn("t1", "delivered"));
    clearAllDiscussionSessions();
    expect(hasTurnEnded("t1")).toBe(false);
  });
});

describe("CVP-FR-HWTN: the store follows turn events with no surface mounted", () => {
  it("a terminal event removes the turn from a held session", () => {
    upsertDiscussionTurn("d1", turn("t1"));
    followAgentTurn(turn("t1", "delivered"));
    expect(ids("d1")).toEqual([]);
  });

  it("a running event updates a held session", () => {
    setComposerBody("d1", "draft text");
    followAgentTurn(turn("t1"));
    expect(ids("d1")).toEqual(["t1"]);
  });

  it("a running event for a discussion with no session creates none", () => {
    const before = getDiscussionSession("d9");
    followAgentTurn(turn("tx", "running", "d9"));
    expect(getDiscussionSession("d9")).toBe(before);
    expect(hasTurnEnded("tx")).toBe(false);
  });

  it("a terminal event for a discussion with no session is still remembered", () => {
    followAgentTurn(turn("ty", "delivered", "d9"));
    expect(hasTurnEnded("ty")).toBe(true);
    setDiscussionTurns("d9", [turn("ty", "running", "d9")]);
    expect(ids("d9")).toEqual([]);
  });

  it("an event with no discussion id is folded into no session", () => {
    const orphan = { ...turn("tz"), origin: { ...turn("tz").origin, discussionId: "" } };
    followAgentTurn(orphan);
    expect(getDiscussionSession("").turns).toEqual([]);
  });
});
