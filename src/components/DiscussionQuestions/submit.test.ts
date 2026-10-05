import { beforeEach, describe, expect, it, vi } from "vitest";

const submitDiscussionQuestionAnswers = vi.fn();
const dispatchAgentTurn = vi.fn();
const listProjectAgents = vi.fn();
vi.mock("../../api", () => ({
  submitDiscussionQuestionAnswers: (a: unknown) => submitDiscussionQuestionAnswers(a),
  dispatchAgentTurn: (a: unknown) => dispatchAgentTurn(a),
  listProjectAgents: () => listProjectAgents(),
  readDiscussion: vi.fn(),
  readDiscussionQuestionSet: vi.fn(),
}));

const logWarn = vi.fn();
vi.mock("../../logging", () => ({
  logInfo: vi.fn(),
  logWarn: (...a: unknown[]) => logWarn(...a),
}));

import { submitQuestionAnswers } from "./submit";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
} from "../../state/discussionSession";
import { publishSet, questionSetOf, clearQuestionSets } from "../../state/questionSets";
import { threadById, clearConversationThreads } from "../../state/conversationThreads";
import type { Discussion, Comment, Participant } from "../../types";
import {
  artifactDiscussionOrigin,
  draftDiscussionOrigin,
  expectBackendOrigin,
  noteDiscussionOrigin,
} from "../../test/origins";

const human: Participant = { kind: "human", login: "raver119" };
const arch: Participant = { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" };
const sec: Participant = { kind: "agent", agentId: "a2", handle: "sec", title: "Security" };

function comment(id: string, author: Participant, body: string): Comment {
  return { id, author, body, quotes: [], attachments: [], createdAt: "2026-09-10T12:00:00Z" };
}

/** A draft discussion whose active agents are whoever the human comments name. */
function thread(comments: Comment[]): Discussion {
  return {
    id: "t1",
    target: { kind: "draft", draftId: "d-1" },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-09-10T11:00:00Z",
    updatedAt: "2026-09-10T12:00:00Z",
  };
}

/** One enrolled agent, in the shape `listProjectAgents` returns. */
function enrolled(nickname: string) {
  return {
    agent: { id: nickname, nickname, model: "m", title: "t", instructions: "" },
    availability: "ready" as const,
  };
}

const answers = [
  { questionPosition: 1, optionPosition: 1, optionValue: "one" },
  { questionPosition: 2, optionPosition: 2, optionValue: "two" },
];

beforeEach(() => {
  submitDiscussionQuestionAnswers.mockReset();
  // Refuses an origin the backend's serde would refuse (AGC-FR-05).
  dispatchAgentTurn.mockReset().mockImplementation(({ origin }: { origin: unknown }) => {
    expectBackendOrigin(origin);
    return Promise.resolve(undefined);
  });
  logWarn.mockReset();
  clearAllDiscussionSessions();
  listProjectAgents.mockReset().mockResolvedValue([enrolled("arch"), enrolled("sec")]);
  clearQuestionSets();
  clearConversationThreads();
});

describe("submitting a set of answers", () => {
  it("DQA-FR-KDVU: sends the whole ordered set in one call", async () => {
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread([comment("c0", human, "@arch thoughts?")]),
      finalAnswerCommentId: "s1:02-2a",
    });

    await submitQuestionAnswers("t1", "s1", answers);

    expect(submitDiscussionQuestionAnswers).toHaveBeenCalledTimes(1);
    expect(submitDiscussionQuestionAnswers).toHaveBeenCalledWith({
      discussionId: "t1",
      setId: "s1",
      answers,
    });
  });

  it("DQA-FR-NKAX, DQA-FR-IPFD: publishes the appended thread and drops the set", async () => {
    publishSet("t1", {
      setId: "s1",
      discussionId: "t1",
      askedBy: arch,
      askedAt: "2026-09-10T12:00:00Z",
      questions: [],
    });
    const appended = thread([
      comment("c0", human, "@arch thoughts?"),
      comment("s1:01-1q", arch, "One spec or two?"),
      comment("s1:01-2a", human, "**Selected option:** one"),
    ]);
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: appended,
      finalAnswerCommentId: "s1:01-2a",
    });

    await submitQuestionAnswers("t1", "s1", answers);

    expect(threadById("t1")).toEqual(appended);
    expect(questionSetOf("t1")).toBeNull();
  });

  it("DQA-FR-CIRK, DQA-FR-QMEA: one turn per active agent for the whole submission", async () => {
    // Ten questions, two agents the conversation is being held with.
    const comments = [comment("c0", human, "@arch @sec thoughts?")];
    for (let position = 1; position <= 10; position += 1) {
      const pad = String(position).padStart(2, "0");
      comments.push(comment(`s1:${pad}-1q`, arch, `Question ${position}?`));
      comments.push(comment(`s1:${pad}-2a`, human, "**Selected option:** one"));
    }
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread(comments),
      finalAnswerCommentId: "s1:10-2a",
    });

    await submitQuestionAnswers("t1", "s1", answers);

    // Ten questions cost one turn per agent, not ten.
    expect(dispatchAgentTurn).toHaveBeenCalledTimes(2);
    const dispatched = dispatchAgentTurn.mock.calls.map(([a]) => a);
    expect(dispatched.map((d) => d.nickname).sort()).toEqual(["arch", "sec"]);
    for (const call of dispatched) {
      // DQA-FR-CIRK: the final answer is each turn's trigger, so the turn's
      // input carries every earlier question and answer.
      expect(call.triggerCommentId).toBe("s1:10-2a");
      expect(call.origin).toEqual(draftDiscussionOrigin("t1", "d-1"));
    }
  });

  it("DQA-FR-CIRK: the agents are resolved once, after the append has committed", async () => {
    const appended = thread([
      comment("c0", human, "@arch thoughts?"),
      comment("s1:01-2a", human, "**Selected option:** one"),
    ]);
    let submittedAt = -1;
    let rosterAt = -1;
    let tick = 0;
    submitDiscussionQuestionAnswers.mockImplementation(() => {
      submittedAt = tick++;
      return Promise.resolve({ discussion: appended, finalAnswerCommentId: "s1:01-2a" });
    });
    listProjectAgents.mockImplementation(() => {
      rosterAt = tick++;
      return Promise.resolve([enrolled("arch")]);
    });

    await submitQuestionAnswers("t1", "s1", answers);

    expect(submittedAt).toBeLessThan(rosterAt);
    expect(listProjectAgents).toHaveBeenCalledTimes(1);
  });

  it("DQA-FR-XBTL: a refused dispatch rolls nothing back", async () => {
    const appended = thread([
      comment("c0", human, "@arch @sec thoughts?"),
      comment("s1:01-2a", human, "**Selected option:** one"),
    ]);
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: appended,
      finalAnswerCommentId: "s1:01-2a",
    });
    // One agent's dispatch fails; the other's must still go.
    dispatchAgentTurn.mockImplementation(({ nickname }: { nickname: string }) =>
      nickname === "arch" ? Promise.reject(new Error("no")) : Promise.resolve(undefined),
    );

    await expect(submitQuestionAnswers("t1", "s1", answers)).resolves.toBeUndefined();

    expect(dispatchAgentTurn).toHaveBeenCalledTimes(2);
    // The comments are committed and the set is gone regardless.
    expect(threadById("t1")).toEqual(appended);
    expect(questionSetOf("t1")).toBeNull();
  });

  it("DQA-FR-XBTL, CTA-FR-UUXA: a refused dispatch shows its refusal at the foot of the discussion", async () => {
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread([
        comment("c0", human, "@arch thoughts?"),
        comment("s1:01-2a", human, "**Selected option:** one"),
      ]),
      finalAnswerCommentId: "s1:01-2a",
    });
    dispatchAgentTurn.mockRejectedValue("agent_not_found");

    await submitQuestionAnswers("t1", "s1", answers);

    await vi.waitFor(() =>
      expect(getDiscussionSession("t1").turnFailure).toBe("agent_not_found"),
    );
    expect(logWarn).toHaveBeenCalledWith(
      ["ai", "frontend"],
      "could not tell an agent the answers",
      { threadId: "t1", nickname: "arch", failure: "agent_not_found" },
    );
  });

  it.each([
    ["the refusal settles first", 0, 5],
    ["the acceptance settles first", 5, 0],
  ])(
    "DQA-FR-XBTL, CTA-FR-UUXA: a refusal stands when another agent of the same batch is accepted (%s)",
    async (_order, refuseAfter, acceptAfter) => {
      submitDiscussionQuestionAnswers.mockResolvedValue({
        discussion: thread([comment("c0", human, "@arch @sec thoughts?")]),
        finalAnswerCommentId: "s1:01-2a",
      });
      dispatchAgentTurn.mockImplementation(
        ({ nickname }: { nickname: string }) =>
          new Promise((resolve, reject) =>
            nickname === "arch"
              ? setTimeout(() => reject("agent_not_found"), refuseAfter)
              : setTimeout(() => resolve(undefined), acceptAfter),
          ),
      );

      await submitQuestionAnswers("t1", "s1", answers);
      await new Promise((r) => setTimeout(r, 20));

      expect(dispatchAgentTurn).toHaveBeenCalledTimes(2);
      expect(getDiscussionSession("t1").turnFailure).toBe("agent_not_found");
    },
  );

  it("DQA-FR-XBTL: a refusal that is not a typed failure is shown but not logged as text", async () => {
    // An argument error can quote the value it refused, and an origin can hold
    // a quoted passage of the author's text.
    const raw = 'invalid args `origin`: unknown variant "a private passage"';
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread([comment("c0", human, "@arch thoughts?")]),
      finalAnswerCommentId: "s1:01-2a",
    });
    dispatchAgentTurn.mockRejectedValue(new Error(raw));

    await submitQuestionAnswers("t1", "s1", answers);

    await vi.waitFor(() => expect(getDiscussionSession("t1").turnFailure).toBe(raw));
    expect(logWarn).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(logWarn.mock.calls[0])).not.toContain("a private passage");
    expect(logWarn.mock.calls[0][2]).toMatchObject({ failure: "unexpected" });
  });

  it("CTA-FR-UUXA: a dispatch that goes through clears a refusal shown before", async () => {
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread([comment("c0", human, "@arch thoughts?")]),
      finalAnswerCommentId: "s1:01-2a",
    });
    dispatchAgentTurn.mockRejectedValueOnce("agent_unavailable");
    await submitQuestionAnswers("t1", "s1", answers);
    await vi.waitFor(() =>
      expect(getDiscussionSession("t1").turnFailure).toBe("agent_unavailable"),
    );

    await submitQuestionAnswers("t1", "s2", answers);
    await vi.waitFor(() => expect(getDiscussionSession("t1").turnFailure).toBeUndefined());
  });

  it("DQA-FR-WKTP: a refused submission rejects and changes nothing", async () => {
    const standing = {
      setId: "s1",
      discussionId: "t1",
      askedBy: arch,
      askedAt: "2026-09-10T12:00:00Z",
      questions: [],
    };
    publishSet("t1", standing);
    submitDiscussionQuestionAnswers.mockRejectedValue("question_answers_incomplete");

    await expect(submitQuestionAnswers("t1", "s1", answers)).rejects.toBe(
      "question_answers_incomplete",
    );

    // The set still stands and nothing was dispatched.
    expect(questionSetOf("t1")).toEqual(standing);
    expect(dispatchAgentTurn).not.toHaveBeenCalled();
  });

  it("a submission nobody is addressed in dispatches nothing", async () => {
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: thread([comment("s1:01-2a", human, "**Selected option:** one")]),
      finalAnswerCommentId: "s1:01-2a",
    });
    listProjectAgents.mockResolvedValue([]);

    await submitQuestionAnswers("t1", "s1", answers);

    expect(dispatchAgentTurn).not.toHaveBeenCalled();
  });

  it("an artifact discussion dispatches with an artifact origin", async () => {
    const artifactThread: Discussion = {
      ...thread([comment("c0", human, "@arch thoughts?")]),
      target: { kind: "artifact", artifactId: "specifications/ui/EDT-editor.md" },
    };
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: artifactThread,
      finalAnswerCommentId: "s1:01-2a",
    });

    await submitQuestionAnswers("t1", "s1", answers);

    expect(dispatchAgentTurn.mock.calls[0][0].origin).toEqual(artifactDiscussionOrigin("t1", "specifications/ui/EDT-editor.md"));
  });

  it("a note discussion dispatches with a note origin", async () => {
    const noteThread: Discussion = {
      ...thread([comment("c0", human, "@sec thoughts?")]),
      target: { kind: "note", noteId: "n-2" },
    };
    submitDiscussionQuestionAnswers.mockResolvedValue({
      discussion: noteThread,
      finalAnswerCommentId: "s1:01-2a",
    });

    await submitQuestionAnswers("t1", "s1", answers);

    expect(dispatchAgentTurn.mock.calls[0][0].origin).toEqual(noteDiscussionOrigin("t1", "n-2"));
  });
});
