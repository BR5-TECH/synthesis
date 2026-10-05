import { describe, expect, it } from "vitest";

import {
  ANSWER_PREFIX,
  OWN_ANSWER_PREFIX,
  isQuestionAnswerPair,
  pairedHalves,
} from "./questionPairs";
import type { Comment, Participant } from "../../types";

const agent: Participant = {
  kind: "agent",
  agentId: "a1",
  handle: "arch",
  title: "Architect",
};
const human: Participant = { kind: "human", login: "raver119" };

function comment(id: string, author: Participant, body: string): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-09-10T12:00:00Z",
  };
}

const question = (id: string) =>
  comment(id, agent, "One spec or two?\n\n1. one\n2. two");
const answer = (id: string) =>
  comment(id, human, `${ANSWER_PREFIX} two\n\n**Note:** keep the diagram`);
/** ADQ-FR-RECR: the other answer body — the author's own words, and no note. */
const ownAnswer = (id: string) =>
  comment(id, human, `${OWN_ANSWER_PREFIX} three, one per layer`);

describe("pairing a submitted question with its answer", () => {
  it("DQA-FR-FBWO: an agent question followed by a human answer is one entry", () => {
    const halves = pairedHalves([question("q1"), answer("a1")]);
    expect(halves.get("q1")).toBe("question");
    expect(halves.get("a1")).toBe("answer");
  });

  it("DQA-FR-FBWO: a whole submitted set pairs, in order", () => {
    const comments = [
      comment("opening", human, "@arch thoughts?"),
      question("q1"),
      answer("a1"),
      question("q2"),
      answer("a2"),
    ];
    const halves = pairedHalves(comments);
    expect([...halves.entries()]).toEqual([
      ["q1", "question"],
      ["a1", "answer"],
      ["q2", "question"],
      ["a2", "answer"],
    ]);
    expect(halves.has("opening")).toBe(false);
  });

  it("DQA-FR-GRUV: the pairing never skips another comment", () => {
    // A comment between the question and the answer breaks the adjacency, so
    // both render on their own.
    const halves = pairedHalves([
      question("q1"),
      comment("interjection", human, "wait"),
      answer("a1"),
    ]);
    expect(halves.size).toBe(0);
  });

  it("DQA-FR-GRUV: an answer is consumed with its own question", () => {
    // Without this, the answer would also open a pair with the question after
    // it and one comment would render twice.
    const halves = pairedHalves([question("q1"), answer("a1"), question("q2")]);
    expect(halves.get("q1")).toBe("question");
    expect(halves.get("a1")).toBe("answer");
    // The trailing question opens no pair of its own.
    expect(halves.has("q2")).toBe(false);
  });

  it("DQA-FR-GRUV: an ordinary human reply to an agent is not a pair", () => {
    expect(
      isQuestionAnswerPair(question("q1"), comment("r", human, "sounds right")),
    ).toBe(false);
  });

  it("DQA-FR-GRUV: two agent comments are not a pair", () => {
    expect(isQuestionAnswerPair(question("q1"), question("q2"))).toBe(false);
  });

  it("DQA-FR-GRUV: two human comments are not a pair", () => {
    expect(isQuestionAnswerPair(answer("a1"), answer("a2"))).toBe(false);
  });

  it("ADQ-FR-RECR: the recognised opening lines are the ones the submission composes", () => {
    // The whole of what makes a pair recognisable without a marker in the body
    // (DQA-FR-TSJD, ADQ-FR-NUEB), so these must be exactly this text.
    expect(ANSWER_PREFIX).toBe("**Selected option:**");
    expect(OWN_ANSWER_PREFIX).toBe("**Own answer:**");
  });

  it("DQA-FR-FBWO, ADQ-FR-RECR: an own answer pairs with its question too", () => {
    const halves = pairedHalves([question("q1"), ownAnswer("a1")]);
    expect(halves.get("q1")).toBe("question");
    expect(halves.get("a1")).toBe("answer");
  });

  it("DQA-FR-FBWO: a set answered both ways pairs throughout", () => {
    const halves = pairedHalves([
      question("q1"),
      answer("a1"),
      question("q2"),
      ownAnswer("a2"),
    ]);
    expect([...halves.entries()]).toEqual([
      ["q1", "question"],
      ["a1", "answer"],
      ["q2", "question"],
      ["a2", "answer"],
    ]);
  });

  it("DQA-FR-GRUV: an own answer is consumed with its own question", () => {
    const halves = pairedHalves([question("q1"), ownAnswer("a1"), question("q2")]);
    // The pair has to be found first, or `q2` is absent for the wrong reason —
    // an implementation that recognised no own answer would leave it out too.
    expect(halves.get("q1")).toBe("question");
    expect(halves.get("a1")).toBe("answer");
    // And the trailing question opens no pair of its own.
    expect(halves.has("q2")).toBe(false);
  });

  it("DQA-FR-TSJD: the pairing carries ids alone and rewrites no body", () => {
    // Presentation and nothing else: what comes back names which half each
    // comment is and holds no comment, no body, and no stored relation.
    const comments = [question("q1"), answer("a1")];
    const before = comments.map((c) => c.body);
    const halves = pairedHalves(comments);
    expect([...halves.values()].every((v) => v === "question" || v === "answer")).toBe(true);
    expect(comments.map((c) => c.body)).toEqual(before);
  });

  it("a conversation with no submission is every comment on its own", () => {
    const comments = [
      comment("c1", human, "@arch thoughts?"),
      comment("c2", agent, "Here is what I found."),
      comment("c3", human, "thanks"),
    ];
    expect(pairedHalves(comments).size).toBe(0);
  });
});
