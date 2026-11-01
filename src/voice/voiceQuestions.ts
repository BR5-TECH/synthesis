/**
 * What voice mode knows of a pending question set (`VOM-voice-mode.md`
 * VOM-FR-HBLE, VOM-FR-PINR). The block lends a handle while it is mounted, so a
 * transcript that comes later asks who stands at that moment.
 */
import type { PendingQuestion } from "../types";
import { speakableText } from "./speakableText";

/** What a mounted question block lends to voice mode. */
export interface VoiceQuestionBlock {
  setId: string;
  /** The question that shows now. */
  showing(): PendingQuestion | null;
  /** DQA-FR-SUUU, DQA-FR-ITCZ, DQA-FR-YMGD: the transcript answers this position. */
  answer(position: number, transcript: string): void;
}

/** VOM-FR-PINR: the question that showed when a recording started. */
export interface AnswerTarget {
  setId: string;
  position: number;
}

const blocks = new Map<string, VoiceQuestionBlock[]>();

/** The block that stands for a discussion. The newest mount wins. */
export function questionBlockOf(discussionId: string | null): VoiceQuestionBlock | null {
  const mounted = discussionId === null ? undefined : blocks.get(discussionId);
  return mounted?.[mounted.length - 1] ?? null;
}

export function registerQuestionBlock(
  discussionId: string,
  block: VoiceQuestionBlock,
): () => void {
  blocks.set(discussionId, [...(blocks.get(discussionId) ?? []), block]);
  return () => {
    const left = (blocks.get(discussionId) ?? []).filter((b) => b !== block);
    if (left.length > 0) blocks.set(discussionId, left);
    else blocks.delete(discussionId);
  };
}

/** VOM-FR-PINR: a recording that starts now answers this, if a set stands. */
export function answerTargetOf(discussionId: string | null): AnswerTarget | null {
  const block = questionBlockOf(discussionId);
  const question = block?.showing() ?? null;
  return block && question ? { setId: block.setId, position: question.position } : null;
}

/**
 * VOM-FR-HBLE, VOM-FR-NWGL: the question text, then each option label in
 * recorded order. No description, and no word for the own-answer row.
 */
export function questionSpeech(question: PendingQuestion): string {
  return speakableText([question.text, ...question.options.map((o) => o.value)].join("\n"));
}

export function resetVoiceQuestionsForTest(): void {
  blocks.clear();
}
