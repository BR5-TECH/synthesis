import { beforeEach, describe, expect, it, vi } from "vitest";

const readDiscussionQuestionSet = vi.fn();
vi.mock("../api", () => ({
  readDiscussionQuestionSet: (threadId: string) =>
    readDiscussionQuestionSet(threadId),
}));

import {
  clearQuestionSets,
  ensureQuestionSetLoaded,
  publishSet,
  questionSetOf,
} from "./questionSets";
import {
  forgetEveryDraft,
  recallAnswer,
  rememberAnswer,
} from "./questionAnswerDrafts";
import type { PendingQuestionSet } from "../types";

function setOf(setId: string, positions: number[]): PendingQuestionSet {
  return {
    setId,
    discussionId: "t1",
    askedBy: { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" },
    askedAt: "2026-09-10T12:00:00Z",
    questions: positions.map((position) => ({
      position,
      text: `question ${position}?`,
      options: [
        { position: 1, value: "one" },
        { position: 2, value: "two" },
      ],
    })),
  };
}

describe("the question sets on screen", () => {
  beforeEach(() => {
    clearQuestionSets();
    forgetEveryDraft();
    readDiscussionQuestionSet.mockReset();
  });

  it("DQA-FR-JWEF: reads a discussion's set once and holds what came back", async () => {
    const set = setOf("s1", [1, 2]);
    readDiscussionQuestionSet.mockResolvedValue(set);

    ensureQuestionSetLoaded("t1");
    await vi.waitFor(() => expect(questionSetOf("t1")).toEqual(set));

    // A second mount of the same discussion issues no read of its own.
    ensureQuestionSetLoaded("t1");
    expect(readDiscussionQuestionSet).toHaveBeenCalledTimes(1);
  });

  it("DQA-FR-JWEF: a discussion holding none reads as none, not as unknown", async () => {
    readDiscussionQuestionSet.mockResolvedValue(null);
    ensureQuestionSetLoaded("t1");
    await vi.waitFor(() => expect(questionSetOf("t1")).toBeNull());
  });

  it("DQA-FR-JWEF: a failed read renders as holding none rather than blocking", async () => {
    readDiscussionQuestionSet.mockRejectedValue(new Error("store_unavailable"));
    ensureQuestionSetLoaded("t1");
    await vi.waitFor(() => expect(questionSetOf("t1")).toBeNull());
  });

  it("CMS-FR-BQEN: an announced set replaces what is held", () => {
    publishSet("t1", setOf("s1", [1]));
    expect(questionSetOf("t1")?.setId).toBe("s1");
    publishSet("t1", null);
    expect(questionSetOf("t1")).toBeNull();
  });

  it("DQA-FR-DYFR: a re-read reconciles the draft against the positions it still records", () => {
    publishSet("t1", setOf("s1", [1, 2, 3]));
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "one" });
    rememberAnswer("s1", 2, { selected: 2, ownWords: false, typed: "", note: "two" });

    // The same set comes back recording one question fewer.
    publishSet("t1", setOf("s1", [1, 3]));

    expect(recallAnswer("s1", 1)).toEqual({ selected: 1, ownWords: false, typed: "", note: "one" });
    expect(recallAnswer("s1", 2)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
  });

  it("DQA-FR-IPFD, DQA-FR-HCTM: a set that goes takes its unsent draft with it", () => {
    publishSet("t1", setOf("s1", [1]));
    rememberAnswer("s1", 1, { selected: 1, ownWords: false, typed: "", note: "one" });

    publishSet("t1", null);

    expect(recallAnswer("s1", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
  });

  it("DQA-FR-GVSA: a set that replaces another does not read its answers", () => {
    publishSet("t1", setOf("s1", [1]));
    rememberAnswer("s1", 1, { selected: 2, ownWords: false, typed: "", note: "for the first set" });

    publishSet("t1", setOf("s2", [1]));

    expect(recallAnswer("s2", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
    // And the outgoing set's draft is gone rather than merely unreachable.
    expect(recallAnswer("s1", 1)).toEqual({ selected: null, ownWords: false, typed: "", note: "" });
  });

  it("CVP-FR-49: everything is dropped on a content-root change", async () => {
    readDiscussionQuestionSet.mockResolvedValue(setOf("s1", [1]));
    ensureQuestionSetLoaded("t1");
    await vi.waitFor(() => expect(questionSetOf("t1")).not.toBeUndefined());

    clearQuestionSets();

    expect(questionSetOf("t1")).toBeUndefined();
    // And the discussion is read again when it is next mounted.
    ensureQuestionSetLoaded("t1");
    expect(readDiscussionQuestionSet).toHaveBeenCalledTimes(2);
  });
});
