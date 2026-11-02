/**
 * What the author has answered in a question set, whatever surface asked it
 * (`../../../specifications/ui/DQA-discussion-question-answering.md` DQA-FR-HSBK).
 *
 * Pure, and separated from the block so that what counts as answered is one
 * rule for every host (DQA-FR-RTWE): the dots, the progress line and Submit
 * cannot disagree, and a discussion and an escalation cannot disagree either.
 */
import type { QuestionDraftAnswer } from "../../state/questionAnswerDrafts";

/** An option as the rules below read it: its recorded position and description. */
export interface AnswerableOption {
  position: number;
  description?: string | null;
}

/** A question as the rules below read it. */
export interface AnswerableQuestion {
  position: number;
  options: readonly AnswerableOption[];
}

/** What a host's draft holds against one recorded position. */
export type RecallAnswer = (position: number) => QuestionDraftAnswer;

/**
 * DQA-FR-SEBN: a question is answered when one of its recorded options is
 * selected, or when the own answer is selected and its text is not blank.
 *
 * DQA-FR-ZNOQ: a question with no options has the own-answer field alone, so
 * its text answers it whether or not a row was ever pressed.
 *
 * DQA-FR-UAKC: the note is never an answer, so it is not read here.
 */
export function isQuestionAnswered(
  question: AnswerableQuestion,
  draft: QuestionDraftAnswer,
): boolean {
  if (question.options.length === 0 || draft.ownWords) return draft.typed.trim() !== "";
  // A selection naming an option a re-read no longer records is no selection
  // (DQA-FR-DYFR), so the dots and Submit cannot disagree about it.
  return question.options.some((option) => option.position === draft.selected);
}

/** DQA-FR-MJPV: how many of the recorded set are answered. */
export function answeredCountOf(
  questions: readonly AnswerableQuestion[],
  recall: RecallAnswer,
): number {
  return questions.filter((question) => isQuestionAnswered(question, recall(question.position)))
    .length;
}

/**
 * DQA-FR-ITCZ: the index of the next unanswered question, in recorded order after
 * `position` and then from the start of the set, or null when none is left.
 */
export function nextUnansweredOf(
  questions: readonly AnswerableQuestion[],
  recall: RecallAnswer,
  position: number,
): number | null {
  const count = questions.length;
  const from = questions.findIndex((q) => q.position === position);
  for (let step = 1; step <= count; step += 1) {
    const question = questions[(from + step + count) % count];
    if (!isQuestionAnswered(question, recall(question.position))) {
      return (from + step + count) % count;
    }
  }
  return null;
}

/**
 * DQA-FR-MJPV: the line naming which question is showing and how many still
 * need an answer.
 */
export function progressLineOf(
  questions: readonly AnswerableQuestion[],
  recall: RecallAnswer,
  page: number,
): string {
  const answered = answeredCountOf(questions, recall);
  const outstanding = questions.length - answered;
  const head = `Question ${page + 1} of ${questions.length} · ${answered} answered`;
  if (outstanding === 0) return head;
  return `${head} · ${outstanding === 1 ? "1 still needs" : `${outstanding} still need`} an answer`;
}

/** DQA-FR-TKUM: what the right rail says while it has no description to show. */
export const DESCRIPTION_PLACEHOLDER = "The description appears once you select an option.";

/**
 * DQA-FR-MIBQ, DQA-FR-EWUG: whether the set renders two rails. A set recorded
 * without descriptions renders in one column.
 */
export function hasDescriptions(questions: readonly AnswerableQuestion[]): boolean {
  return questions.some((question) =>
    question.options.some((option) => (option.description ?? "").trim() !== ""),
  );
}

/**
 * What the pointer or the keyboard is on (DQA-FR-TKUM, DQA-FR-KBJA): an option
 * by its recorded position, the own-answer row, or nothing.
 */
export type HoveredRow = number | "own" | null;

/**
 * The recorded position of the option whose description the right rail shows,
 * or `null` where it shows the placeholder (DQA-FR-TKUM, DQA-FR-USTA).
 */
export function shownOption(
  question: AnswerableQuestion,
  hovered: HoveredRow,
  draft: QuestionDraftAnswer,
): number | null {
  if (hovered === "own") return null;
  const position = hovered ?? (draft.ownWords ? null : draft.selected);
  if (position === null) return null;
  const description = question.options.find((option) => option.position === position)
    ?.description;
  return description && description.trim() !== "" ? position : null;
}
