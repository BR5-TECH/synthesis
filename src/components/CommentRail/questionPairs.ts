/**
 * Pairing a submitted question with the answer beneath it
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-FBWO, DQA-FR-GRUV, DQA-FR-TSJD).
 *
 * An accepted submission appends two comments per question: the agent-authored
 * question, then the human-authored answer. They read as one exchange, so the
 * history draws them as **one entry** with a single rule between the halves
 * rather than as two stacked cards.
 *
 * ## Why the pairing is recognised rather than stored
 *
 * DQA-FR-TSJD makes the pairing presentation and nothing else: no stored
 * relation joins the two comments, neither body is rewritten to render them
 * together, and a surface that draws them separately loses nothing but the
 * grouping. ADQ-FR-NUEB forbids a marker in either body — no routing tag, no
 * identifier, no machine-readable field — so there is deliberately nothing to
 * key on but what the comments **are**: adjacent, one agent-authored and the
 * next human-authored, and the second opening with one of the two fixed lines
 * the submission composes (ADQ-FR-RECR).
 *
 * That is as far as the recognition goes, and it is meant to. A person who
 * writes a comment beginning `**Selected option:** ` or `**Own answer:** `
 * immediately after an agent's gets it drawn as a pair; the cost is a heading
 * that names them both,
 * and the alternative — a marker in the body — is the thing the spec forbids.
 * Nobody should "fix" this by adding one.
 *
 * ## Why adjacency is strict
 *
 * DQA-FR-GRUV: a question comment with any other comment between it and the next
 * answer renders on its own, and so does the answer. Two people talking across a
 * submission is a conversation rather than a form, and joining halves that are
 * not actually adjacent would put words in the wrong exchange.
 */
import type { Comment } from "../../types";

/**
 * ADQ-FR-RECR: the fixed line an answer comment opens with where the author
 * chose one of the recorded options.
 *
 * The submission composes it and nothing else does, which is what makes it
 * recognisable without a marker.
 */
export const ANSWER_PREFIX = "**Selected option:**";

/**
 * ADQ-FR-RECR: the other fixed line — the one an answer opens with where the
 * author wrote their own words instead of choosing a recorded option
 * (DQA-FR-FCZL).
 *
 * Both lines make an answer. A surface that knew only the first would draw
 * every own answer as an ordinary comment and break the entry it belongs to
 * (DQA-FR-FBWO).
 */
export const OWN_ANSWER_PREFIX = "**Own answer:**";

/** Which half of a submitted exchange a comment is, where it is one. */
export type PairedHalf = "question" | "answer";

/**
 * DQA-FR-FBWO: which of a thread's comments are the two halves of one submitted
 * exchange, by comment id.
 *
 * The one implementation, called by the card that renders the history. It
 * returns a map rather than regrouped entries because the pairing is
 * **presentation and nothing else** (DQA-FR-TSJD): the two halves keep their own
 * heading, their own timestamp and their own Quote — they are ordinary comments
 * of the conversation from the moment they land (DQA-FR-NKAX) — and what the map
 * changes is only that they are drawn inside one entry with a single rule
 * between them. A surface that ignored it would render two ordinary comments and
 * lose the grouping and nothing else.
 */
export function pairedHalves(
  comments: readonly Comment[],
): Map<string, PairedHalf> {
  const halves = new Map<string, PairedHalf>();
  for (let index = 0; index < comments.length; index += 1) {
    const question = comments[index];
    const answer = comments[index + 1];
    if (answer && isQuestionAnswerPair(question, answer)) {
      halves.set(question.id, "question");
      halves.set(answer.id, "answer");
      // DQA-FR-GRUV: an answer is consumed with its own question, so it can
      // never also open a pair with the comment after it.
      index += 1;
    }
  }
  return halves;
}

/**
 * Whether these two adjacent comments are a submitted question and its answer.
 *
 * All three conditions are required, and each rules out a real case: an agent
 * comment followed by another agent's is two agents talking; a human comment
 * followed by a human's is two people talking; and an ordinary human reply to an
 * agent is the commonest thing in any conversation, which is why the fixed
 * opening line has to be there as well.
 */
export function isQuestionAnswerPair(question: Comment, answer: Comment): boolean {
  return (
    question.author.kind === "agent" &&
    answer.author.kind === "human" &&
    isAnswerBody(answer.body)
  );
}

/**
 * ADQ-FR-RECR: whether a body opens with one of the two lines a submitted answer
 * opens with.
 *
 * The two lines are the whole of the recognition. ADQ-FR-NUEB forbids a marker
 * in the body, so there is nothing else to key on, and DQA-FR-FBWO holds for
 * both kinds of answer alike.
 */
export function isAnswerBody(body: string): boolean {
  return body.startsWith(ANSWER_PREFIX) || body.startsWith(OWN_ANSWER_PREFIX);
}
