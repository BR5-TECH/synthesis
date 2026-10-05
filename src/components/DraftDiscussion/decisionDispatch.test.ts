import { beforeEach, describe, expect, it, vi } from "vitest";

const dispatchAgentTurn = vi.fn();
const listProjectAgents = vi.fn();
const readDiscussion = vi.fn();
vi.mock("../../api", () => ({
  dispatchAgentTurn: (a: unknown) => dispatchAgentTurn(a),
  listProjectAgents: () => listProjectAgents(),
  readDiscussion: (id: string) => readDiscussion(id),
}));
const logWarn = vi.fn();
vi.mock("../../logging", () => ({
  logInfo: vi.fn(),
  logWarn: (...a: unknown[]) => logWarn(...a),
  logDebug: vi.fn(),
}));

import { dispatchDecision } from "./decisionDispatch";
import { clearConversationThreads } from "../../state/conversationThreads";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
} from "../../state/discussionSession";
import type {
  Comment,
  DecisionOutcome,
  Discussion,
  DraftChangeProposal,
  Participant,
  ProjectAgent,
} from "../../types";
import { draftDiscussionOrigin, expectBackendOrigin } from "../../test/origins";

const human: Participant = { kind: "human", login: "raver119" };

function comment(id: string, body: string): Comment {
  return { id, author: human, body, quotes: [], attachments: [], createdAt: "2026-09-10T12:00:00Z" };
}

const fragment = {
  owner: { kind: "draft", draftId: "d1" },
  path: "prompt.md",
  start: 2,
  end: 9,
  quote: "passage",
} as const;

function thread(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "t1",
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments: [comment("c0", "@arch thoughts?"), comment("decision-1", "Accepted.")],
    locked: false,
    resolved: false,
    createdAt: "2026-09-10T11:00:00Z",
    updatedAt: "2026-09-10T12:00:00Z",
    ...over,
  };
}

const proposal = { id: "p1", draftId: "d1", threadId: "t1" } as DraftChangeProposal;
const outcome = { proposal, commentId: "decision-1", originKind: "draft_discussion" } as DecisionOutcome;
const roster: ProjectAgent[] = [
  {
    agent: { id: "arch", nickname: "arch", model: "m", title: "t", instructions: "" },
    availability: "ready",
  } as unknown as ProjectAgent,
];

beforeEach(() => {
  dispatchAgentTurn.mockReset().mockImplementation(({ origin }: { origin: unknown }) => {
    expectBackendOrigin(origin);
    return Promise.resolve(undefined);
  });
  listProjectAgents.mockReset().mockResolvedValue(roster);
  readDiscussion.mockReset();
  logWarn.mockReset();
  clearConversationThreads();
  clearAllDiscussionSessions();
});

describe("telling the conversation what was decided (DCR-FR-15)", () => {
  it("DCR-FR-15, AGC-FR-05: a whole-draft discussion dispatches with the origin the backend reads", async () => {
    readDiscussion.mockResolvedValue(thread());

    await dispatchDecision(proposal, outcome, undefined, roster, "");

    expect(dispatchAgentTurn).toHaveBeenCalledTimes(1);
    expect(dispatchAgentTurn.mock.calls[0][0]).toEqual({
      nickname: "arch",
      origin: draftDiscussionOrigin("t1", "d1"),
      triggerCommentId: "decision-1",
    });
  });

  it("DCR-FR-15, AGC-FR-05: a fragment discussion sends its own fragment target", async () => {
    readDiscussion.mockResolvedValue(thread({ fragmentTarget: fragment }));

    await dispatchDecision(proposal, outcome, undefined, roster, "");

    expect(dispatchAgentTurn.mock.calls[0][0].origin).toEqual({
      discussionId: "t1",
      target: { kind: "draft", draftId: "d1" },
      fragmentTarget: fragment,
    });
  });

  it("DCR-FR-15, AGC-FR-05: a conversation that could not be read still sends an origin the backend reads", async () => {
    readDiscussion.mockRejectedValue("comment_thread_not_found");

    // The feedback names the agent, so the decision reaches it without the thread.
    await dispatchDecision(proposal, outcome, undefined, roster, "@arch see this");

    expect(dispatchAgentTurn).toHaveBeenCalledTimes(1);
    expect(dispatchAgentTurn.mock.calls[0][0].origin).toEqual(
      draftDiscussionOrigin("t1", "d1"),
    );
  });

  it("PCR-FR-14, DCR-FR-15: a refused dispatch is logged by its typed code and not surfaced", async () => {
    readDiscussion.mockResolvedValue(thread());
    dispatchAgentTurn.mockRejectedValue(
      new Error('invalid args `origin`: unknown variant "a private passage"'),
    );

    await dispatchDecision(proposal, outcome, undefined, roster, "");

    await vi.waitFor(() => expect(logWarn).toHaveBeenCalledTimes(1));
    expect(logWarn.mock.calls[0][2]).toEqual({
      proposalId: "p1",
      nickname: "arch",
      failure: "unexpected",
    });
    expect(JSON.stringify(logWarn.mock.calls[0])).not.toContain("a private passage");
    expect(getDiscussionSession("t1").turnFailure).toBeUndefined();
  });
});
