/**
 * What voice mode knows of a pending question set (`VOM-voice-mode.md`
 * VOM-FR-HBLE, VOM-FR-PINR). The block lends a handle while it is mounted, so a
 * transcript that comes later asks who stands at that moment.
 */
import { isAnswered } from "../components/DiscussionQuestions/answers";
import { questionSetOf } from "../state/questionSets";
import { getVoiceState } from "../state/voiceSession";
import type { PendingQuestion } from "../types";
import { speakableText } from "./speakableText";

/** What a mounted question block lends to voice mode. */
export interface VoiceQuestionBlock {
  setId: string;
  /** The question that shows now. */
  showing(): PendingQuestion | null;
  /** False once the set was submitted, so the block is not retained. */
  isOpen?(): boolean;
  /** DQA-FR-SUUU, DQA-FR-ITCZ, DQA-FR-YMGD: the transcript answers this position. */
  answer(position: number, transcript: string): void;
}

/** VOM-FR-PINR: the question that showed when a recording started. */
export interface AnswerTarget {
  setId: string;
  position: number;
}

const blocks = new Map<string, VoiceQuestionBlock[]>();
/**
 * VOM-FR-BCRD: the block of the voice discussion while none is mounted, so a
 * spoken answer still works after the author opened another tab.
 */
const retained = new Map<string, VoiceQuestionBlock>();

/** The block that stands for a discussion. The newest mount wins. */
export function questionBlockOf(discussionId: string | null): VoiceQuestionBlock | null {
  if (discussionId === null) return null;
  const mounted = blocks.get(discussionId);
  const kept = retained.get(discussionId);
  // The retained block counts while its set still stands.
  const stands =
    kept !== undefined &&
    kept.isOpen?.() !== false &&
    questionSetOf(discussionId)?.setId === kept.setId;
  return mounted?.[mounted.length - 1] ?? (stands ? kept : null);
}

/** A block of this discussion is on screen. */
export function questionBlockMounted(discussionId: string): boolean {
  return (blocks.get(discussionId)?.length ?? 0) > 0;
}

/** Voice mode ended. */
export function forgetRetainedQuestions(): void {
  retained.clear();
}

export function registerQuestionBlock(
  discussionId: string,
  block: VoiceQuestionBlock,
): () => void {
  blocks.set(discussionId, [...(blocks.get(discussionId) ?? []), block]);
  retained.delete(discussionId);
  return () => {
    const left = (blocks.get(discussionId) ?? []).filter((b) => b !== block);
    if (left.length > 0) blocks.set(discussionId, left);
    else blocks.delete(discussionId);
    const open = block.isOpen?.() !== false;
    if (left.length === 0 && open && getVoiceState().discussionId === discussionId) {
      retained.set(discussionId, block);
    }
  };
}

/**
 * VOM-FR-PINR: a recording that starts now answers this, if a set stands.
 *
 * `taken` names a question that an earlier recording answers and whose
 * transcript has not come yet. That question shows still, so a recording that
 * starts now answers the next question, in the order of DQA-FR-ITCZ, that no
 * recording answers and that has no answer. Each answer then stays with its own
 * question, and a later answer does not replace an earlier one.
 */
export function answerTargetOf(
  discussionId: string | null,
  taken: (target: AnswerTarget) => boolean = () => false,
): AnswerTarget | null {
  const block = questionBlockOf(discussionId);
  const question = block?.showing() ?? null;
  if (!block || !question) return null;
  const shown = { setId: block.setId, position: question.position };
  if (!taken(shown)) return shown;
  const set = questionSetOf(discussionId ?? "");
  if (!set || set.setId !== block.setId) return shown;
  const count = set.questions.length;
  const from = set.questions.findIndex((q) => q.position === question.position);
  for (let step = 1; step < count; step += 1) {
    const next = set.questions[(from + step + count) % count];
    const target = { setId: set.setId, position: next.position };
    if (!isAnswered(set.setId, next.position) && !taken(target)) return target;
  }
  return shown;
}

/** set id -> the page that the last spoken answer moved the block to. */
const spokenPages = new Map<string, number>();

/**
 * DQA-FR-ITCZ, VOM-FR-BCRD: a spoken answer moved the block to this page. A
 * block that mounts again shows it, so the next spoken answer answers the
 * question that the author heard and not the first question again.
 */
export function noteSpokenPage(setId: string, page: number): void {
  spokenPages.set(setId, page);
}

/** The author turned the page by hand, so a remount follows DQA-FR-GVSA again. */
export function forgetSpokenPage(setId: string): void {
  spokenPages.delete(setId);
}

export function spokenPageOf(setId: string): number | null {
  return spokenPages.get(setId) ?? null;
}

/**
 * VOM-FR-HBLE, VOM-FR-NWGL: the question text alone. The author answers in free
 * form, and the options stay on screen. No label, no description, and no word
 * for the own-answer row.
 */
export function questionSpeech(question: PendingQuestion): string {
  return speakableText(question.text);
}

export function resetVoiceQuestionsForTest(): void {
  blocks.clear();
  retained.clear();
  spokenPages.clear();
}
