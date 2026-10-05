/**
 * What the author has answered, and whether the set may be sent
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * Pure, and separated from the block for the reason `../../state/graduation`'s
 * own helpers are: DQA-FR-SEBN, DQA-FR-WGQY and DQA-FR-KDVU are claims about
 * what counts as answered and what is sent, and proving them through a render
 * would test the markup rather than the rule.
 */
import type {
  PendingQuestionSet,
  QuestionAnswerInput,
} from "../../types";
import { recallAnswer } from "../../state/questionAnswerDrafts";

/**
 * DQA-FR-SEBN: a question is answered when one of its recorded options is
 * selected, or when the own answer is selected and its text is not blank.
 *
 * DQA-FR-UAKC: the note is never an answer row and never stands in for a
 * choice, so it is not consulted here at all.
 */
export function isAnswered(setId: string, position: number): boolean {
  const draft = recallAnswer(setId, position);
  // DQA-FR-FCZL: the own-answer row selected with nothing written is a row
  // pressed rather than a question answered.
  if (draft.ownWords) return draft.typed.trim() !== "";
  return draft.selected !== null;
}

/** DQA-FR-MJPV: how many of the recorded set are answered. */
export function answeredCount(set: PendingQuestionSet): number {
  return set.questions.filter((question) => isAnswered(set.setId, question.position))
    .length;
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

/**
 * DQA-FR-MJPV: the line naming which question is showing and how many still
 * need an answer.
 *
 * Composed here rather than in the block so the wording is provable without a
 * render, and so the two halves cannot drift apart.
 */
export function progressLine(set: PendingQuestionSet, page: number): string {
  const answered = answeredCount(set);
  const outstanding = set.questions.length - answered;
  const head = `Question ${page + 1} of ${set.questions.length} · ${answered} answered`;
  if (outstanding === 0) return head;
  return `${head} · ${outstanding === 1 ? "1 still needs" : `${outstanding} still need`} an answer`;
}
