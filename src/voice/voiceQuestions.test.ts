import { afterEach, describe, expect, it } from "vitest";

import type { PendingQuestion } from "../types";
import {
  answerTargetOf,
  questionBlockOf,
  questionSpeech,
  registerQuestionBlock,
  resetVoiceQuestionsForTest,
  type VoiceQuestionBlock,
} from "./voiceQuestions";

function question(position: number): PendingQuestion {
  return { position, text: `Question ${position}?`, options: [] };
}

function block(setId: string, showing: PendingQuestion | null): VoiceQuestionBlock {
  return { setId, showing: () => showing, answer: () => undefined };
}

afterEach(resetVoiceQuestionsForTest);

describe("the blocks that voice mode can reach", () => {
  it("VOM-FR-PINR: the newest mount stands for the discussion, and the older one follows", () => {
    const older = block("s1", question(1));
    const newer = block("s1", question(2));
    registerQuestionBlock("d1", older);
    const leave = registerQuestionBlock("d1", newer);
    expect(questionBlockOf("d1")).toBe(newer);
    leave();
    expect(questionBlockOf("d1")).toBe(older);
  });

  it("VOM-FR-PINR: a block of another discussion is no target", () => {
    registerQuestionBlock("d2", block("s1", question(1)));
    expect(answerTargetOf("d1")).toBeNull();
    expect(answerTargetOf(null)).toBeNull();
  });

  it("VOM-FR-PINR: the target is the set and the question that shows", () => {
    registerQuestionBlock("d1", block("s1", question(2)));
    expect(answerTargetOf("d1")).toEqual({ setId: "s1", position: 2 });
  });

  it("VOM-FR-PINR: a block that shows no question is no target", () => {
    registerQuestionBlock("d1", block("s1", null));
    expect(answerTargetOf("d1")).toBeNull();
  });
});

describe("questionSpeech", () => {
  it("VOM-FR-HBLE, VOM-FR-NWGL: the text, then the labels in order, and no description", () => {
    expect(
      questionSpeech({
        position: 1,
        text: "Which **one**?",
        options: [
          { position: 1, value: "First", description: "Not spoken." },
          { position: 2, value: "Second" },
        ],
      }),
    ).toBe("Which one?\nFirst\nSecond");
  });
});
