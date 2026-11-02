import { describe, expect, it } from "vitest";

import type { QuestionDraftAnswer } from "../../state/questionAnswerDrafts";
import {
  answeredCountOf,
  isQuestionAnswered,
  nextUnansweredOf,
  progressLineOf,
} from "./answers";

// DQA-discussion-question-answering.md: the rules every host of the question
// block shares (DQA-FR-RTWE).

const EMPTY: QuestionDraftAnswer = { selected: null, ownWords: false, typed: "", note: "" };
const withOptions = { position: 1, options: [{ position: 1 }, { position: 2 }] };
const alone = { position: 2, options: [] };

describe("what counts as answered", () => {
  it("DQA-FR-SEBN: a recorded option chosen answers the question", () => {
    expect(isQuestionAnswered(withOptions, { ...EMPTY, selected: 2 })).toBe(true);
    expect(isQuestionAnswered(withOptions, EMPTY)).toBe(false);
  });

  it("DQA-FR-SEBN, DQA-FR-DYFR: a selection naming an option the set no longer records answers nothing", () => {
    expect(isQuestionAnswered(withOptions, { ...EMPTY, selected: 3 })).toBe(false);
  });

  it("DQA-FR-SEBN: the own answer answers only once its text is not blank", () => {
    expect(isQuestionAnswered(withOptions, { ...EMPTY, ownWords: true, typed: "  " })).toBe(false);
    expect(isQuestionAnswered(withOptions, { ...EMPTY, ownWords: true, typed: "x" })).toBe(true);
    // The own answer chosen stands over an option still held in the draft.
    expect(isQuestionAnswered(withOptions, { ...EMPTY, selected: 1, ownWords: true })).toBe(false);
  });

  it("DQA-FR-ZNOQ: with no options, the text answers whether or not a row was pressed", () => {
    expect(isQuestionAnswered(alone, { ...EMPTY, typed: "yes" })).toBe(true);
    expect(isQuestionAnswered(alone, { ...EMPTY, typed: " \n " })).toBe(false);
  });

  it("DQA-FR-UAKC: a note alone answers nothing", () => {
    expect(isQuestionAnswered(withOptions, { ...EMPTY, note: "later" })).toBe(false);
    expect(isQuestionAnswered(alone, { ...EMPTY, note: "later" })).toBe(false);
  });
});

describe("the counts", () => {
  const drafts: Record<number, QuestionDraftAnswer> = { 2: { ...EMPTY, typed: "yes" } };
  const recall = (position: number) => drafts[position] ?? EMPTY;

  it("DQA-FR-MJPV: the progress line counts over the set and names what is left", () => {
    expect(answeredCountOf([withOptions, alone], recall)).toBe(1);
    expect(progressLineOf([withOptions, alone], recall, 0)).toBe(
      "Question 1 of 2 · 1 answered · 1 still needs an answer",
    );
    expect(progressLineOf([withOptions, alone, { position: 3, options: [] }], recall, 2)).toBe(
      "Question 3 of 3 · 1 answered · 2 still need an answer",
    );
    expect(progressLineOf([alone], recall, 0)).toBe("Question 1 of 1 · 1 answered");
  });

  it("DQA-FR-ITCZ: the next unanswered question is found after the one given, then from the start", () => {
    const third = { position: 3, options: [] };
    expect(nextUnansweredOf([withOptions, alone, third], recall, 2)).toBe(2);
    expect(nextUnansweredOf([withOptions, alone, third], recall, 3)).toBe(0);
    expect(nextUnansweredOf([alone], recall, 2)).toBeNull();
  });
});
