/**
 * What the author has entered against a question set and not yet sent
 * (`../../specifications/ui/DQA-discussion-question-answering.md` DQA-FR-EGZS).
 *
 * Held **here**, outside the component's lifetime, for the reason
 * `./escalationDrafts.ts` holds an escalation's: a discussion is rendered in
 * five presentations and the author moves it between them (CVP-FR-47), the set
 * is re-read whenever the backend announces one (DQA-FR-JWEF), and a re-render
 * follows every arriving comment. None of those has anything to redraw a
 * half-answered set from, so a draft kept in the component would be lost to the
 * first event that arrived.
 *
 * It is keyed by the **set's id** together with each question's **recorded
 * position** (DQA-FR-CBQK) — never by the question's text and never by the index
 * of the page showing. Keying by the set id is also what makes a set that
 * replaces another start clean (DQA-FR-GVSA): a new set has a new id, so it
 * reads none of the entries of the one before it.
 *
 * It is **memory and not storage** (DQA-FR-ZUPB): written to no preference and
 * to no backend operation, held for the life of the running application, and no
 * draft of one set is ever read for another. A restart therefore finds the
 * questions and none of the answers (ADQ-FR-PZWD).
 *
 * The rule earns its place at the refusal. An author who has answered nine
 * questions and been refused on the whole set has done the work once, and a
 * surface that redrew itself from the store would have them do it again
 * (DQA-FR-WKTP).
 */

/** DQA-FR-EGZS: the parts of one question's unsent answer. */
export interface QuestionDraftAnswer {
  /**
   * DQA-FR-QWTB: the recorded **position** of the option the author selected,
   * or null where they have selected none — no option is preselected, so null
   * is the state every question starts in.
   *
   * Held by position rather than by value so a re-read that changed an option's
   * text still restores the same row (DQA-FR-DYFR).
   */
  selected: number | null;
  /**
   * DQA-FR-FCZL: whether the author answered in their **own words** rather than
   * choosing one of the recorded options.
   *
   * DQA-FR-HLDS: the own answer is a row of the same group, so this and
   * `selected` are two readings of one choice — where this is true the own words
   * are the answer, whatever `selected` still holds. The selection is kept
   * rather than cleared, so an author who tries their own words and then goes
   * back to an option finds the option they had chosen.
   */
  ownWords: boolean;
  /** DQA-FR-FCZL: what they wrote in the own-answer field, spaces and all. */
  typed: string;
  /**
   * DQA-FR-ROXG: what they typed in this question's own note field, exactly as
   * they left it — spaces and all.
   *
   * DQA-FR-UAKC: a note is never an answer. A question carrying a note and no
   * answer is unanswered, so this field never stands in for a choice.
   *
   * DQA-FR-JJON: it is kept while the author answers in their own words and is
   * not sent, so choosing an option again brings back what they had written.
   */
  note: string;
}

const EMPTY: QuestionDraftAnswer = {
  selected: null,
  ownWords: false,
  typed: "",
  note: "",
};

/** set id -> recorded position -> what was entered against it. */
const bySet = new Map<string, Map<number, QuestionDraftAnswer>>();

/** DQA-FR-EGZS: what the author entered against one question, if anything. */
export function recallAnswer(setId: string, position: number): QuestionDraftAnswer {
  return bySet.get(setId)?.get(position) ?? EMPTY;
}

/** Every position this set holds an entry for. */
export function draftedPositions(setId: string): number[] {
  return [...(bySet.get(setId)?.keys() ?? [])];
}

/**
 * DQA-FR-EGZS: record what the author entered against one question.
 *
 * An entry holding nothing at all — no answer and no text — is removed rather
 * than kept as an empty one, so a question the author only paged through leaves
 * no trace. Clearing a note they had typed leaves the question exactly as
 * answered as its answer makes it (DQA-FR-SEBN).
 */
export function rememberAnswer(
  setId: string,
  position: number,
  answer: QuestionDraftAnswer,
): void {
  const held = bySet.get(setId) ?? new Map<number, QuestionDraftAnswer>();
  // The own-answer row selected with nothing typed yet is a real state and is
  // kept: dropping the entry would put the radio back to no answer chosen
  // (DQA-FR-FCZL).
  if (
    answer.selected === null &&
    !answer.ownWords &&
    answer.typed === "" &&
    answer.note === ""
  ) {
    held.delete(position);
  } else {
    held.set(position, answer);
  }
  if (held.size === 0) bySet.delete(setId);
  else bySet.set(setId, held);
}

/**
 * DQA-FR-DYFR: a set re-read while the block stands is **reconciled rather than
 * replaced**.
 *
 * Every entry whose position the re-read set still records is kept against that
 * position, and an entry for a position the set no longer records is dropped
 * with it. Nothing is moved: an entry belongs to a position rather than to a
 * place in a list.
 */
export function reconcile(setId: string, positions: Iterable<number>): void {
  const held = bySet.get(setId);
  if (!held) return;
  const alive = new Set(positions);
  for (const position of [...held.keys()]) {
    if (!alive.has(position)) held.delete(position);
  }
  if (held.size === 0) bySet.delete(setId);
}

/**
 * DQA-FR-HCTM, DQA-FR-IPFD: the draft is discarded when the set it belongs to is
 * submitted and accepted, and when the discussion stops holding that set by any
 * other route.
 */
export function forgetSet(setId: string): void {
  bySet.delete(setId);
}

/** Test seam: the whole of what is held, dropped. */
export function forgetEveryDraft(): void {
  bySet.clear();
}
