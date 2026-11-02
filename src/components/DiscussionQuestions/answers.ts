/**
 * What the author has answered in a discussion's set, and whether the set may be
 * sent (`../../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * The rules themselves are the shared ones of `../QuestionSet/answers.ts`
 * (DQA-FR-RTWE). This module binds them to the discussion's own draft store and
 * composes the discussion's own payload (DQA-FR-KDVU).
 */
import type {
  PendingQuestionSet,
  QuestionAnswerInput,
} from "../../types";
import { recallAnswer } from "../../state/questionAnswerDrafts";
import {
  answeredCountOf,
  hasDescriptions,
  isQuestionAnswered,
  nextUnansweredOf,
  progressLineOf,
  type RecallAnswer,
} from "../QuestionSet/answers";

export {
  DESCRIPTION_PLACEHOLDER,
  shownOption,
  type HoveredRow,
} from "../QuestionSet/answers";

const recallFor = (setId: string): RecallAnswer => (position) => recallAnswer(setId, position);

/** DQA-FR-SEBN: whether the question at this position of the set is answered. */
export function isAnswered(set: PendingQuestionSet, position: number): boolean {
  const question = set.questions.find((q) => q.position === position);
  if (!question) return false;
  return isQuestionAnswered(question, recallAnswer(set.setId, position));
}

/** DQA-FR-ITCZ: the index of the next unanswered question, or null. */
export function nextUnanswered(set: PendingQuestionSet, position: number): number | null {
  return nextUnansweredOf(set.questions, recallFor(set.setId), position);
}

/** DQA-FR-MJPV: how many of the recorded set are answered. */
export function answeredCount(set: PendingQuestionSet): number {
  return answeredCountOf(set.questions, recallFor(set.setId));
}

/** DQA-FR-MJPV: how many still need an answer. */
export function outstandingCount(set: PendingQuestionSet): number {
  return set.questions.length - answeredCount(set);
}

/**
 * DQA-FR-KDVU: the whole ordered set of answers, or `null` where the set is not
 * yet complete.
 *
 * `null` is what DQA-FR-WGQY holds Submit disabled on, so the enablement and the
 * payload are one decision rather than two that could disagree.
 *
 * Each entry names the question's recorded position and then **either** the
 * chosen option's recorded position with that option's recorded value and the
 * note where one was written, **or** the author's own words — as separate
 * fields (CMS-FR-GNTB). A blank note is **omitted** rather than sent as an empty
 * string, so the comment the submission appends carries no empty Note paragraph
 * (ADQ-FR-RECR).
 */
export function composeAnswers(
  set: PendingQuestionSet,
): QuestionAnswerInput[] | null {
  const answers: QuestionAnswerInput[] = [];
  for (const question of set.questions) {
    const draft = recallAnswer(set.setId, question.position);
    if (draft.ownWords) {
      // DQA-FR-JJON: own words carry no note. The note the author may have
      // written against an option stays in the draft and is not sent, so a
      // submission never asks the reader which of the two was meant.
      if (draft.typed.trim() === "") return null;
      answers.push({
        questionPosition: question.position,
        ownAnswer: draft.typed,
      });
      continue;
    }
    if (draft.selected === null) return null;
    const option = question.options.find(
      (candidate) => candidate.position === draft.selected,
    );
    // A selection naming an option the set no longer records is no selection.
    // Reachable after a re-read that changed the options (DQA-FR-DYFR), which
    // keeps the entry against its position without checking what it names.
    if (!option) return null;
    const note = draft.note.trim();
    answers.push({
      questionPosition: question.position,
      optionPosition: option.position,
      optionValue: option.value,
      ...(note === "" ? {} : { note: draft.note }),
    });
  }
  return answers;
}

/** DQA-FR-MJPV: the progress line of a discussion's set. */
export function progressLine(set: PendingQuestionSet, page: number): string {
  return progressLineOf(set.questions, recallFor(set.setId), page);
}

/** DQA-FR-MIBQ, DQA-FR-EWUG: whether the set renders two rails. */
export function setHasDescriptions(set: PendingQuestionSet): boolean {
  return hasDescriptions(set.questions);
}
