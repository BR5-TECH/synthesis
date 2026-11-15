/**
 * The question sets of a discussion (`CMS-comments-storage.md` CMS-FR-XWDA ..
 * CMS-FR-TXRB, `DQA-discussion-question-answering.md`). They stand beside the
 * log of a discussion and are not comments.
 */
import type { Discussion, Participant } from "./comments";

/**
 * CMS-FR-XWDA: the durable question set a discussion holds while it waits on the
 * author (`ADQ-ask-discussion-questions-tool.md`).
 *
 * It is **not** a comment and occupies no position in the conversation's history
 * (DQA-FR-TVMH): it stands beside the log until the author submits, and only
 * then does the discussion gain lines. It carries no selected option and no note
 * — a restart finds the questions and none of the answers (ADQ-FR-PZWD).
 */
export interface PendingQuestionSet {
  setId: string;
  discussionId: string;
  /**
   * CMS-FR-41: the asking agent's participant snapshot, taken when the set was
   * recorded. It is what the block names and what the generated question
   * comments are stamped with (DQA-FR-PDLN).
   */
  askedBy: Participant;
  askedAt: string;
  questions: PendingQuestion[];
}

/** One question of a set, at the position the application assigned it. */
export interface PendingQuestion {
  /**
   * ADQ-FR-TWNS: 1-based, stable, ascending. Every later act names this rather
   * than the text, because two questions may read alike.
   */
  position: number;
  text: string;
  options: QuestionOption[];
}

/** One proposed answer, at the position it was recorded in. */
export interface QuestionOption {
  position: number;
  /** The option's label: what the author picks (DQA-FR-QWTB). */
  value: string;
  /**
   * CMS-FR-TVOF: what choosing the option means. Absent on a set recorded
   * without descriptions, which renders in one column (DQA-FR-EWUG).
   */
  description?: string;
}

/**
 * DQA-FR-KDVU: one entry of a submission, each field named separately.
 *
 * The value travels beside the position and is checked against the record
 * (CMS-FR-PJBV), so a block answering a set that has since changed is refused
 * rather than recording whichever option now stands there.
 */
export interface QuestionAnswerInput {
  questionPosition: number;
  /**
   * CMS-FR-GNTB: an entry names a chosen option or the author's own words,
   * never both and never neither.
   */
  optionPosition?: number;
  optionValue?: string;
  /** DQA-FR-FCZL: what the author wrote where no recorded option suited. */
  ownAnswer?: string;
  /** DQA-FR-JJON: the addition to a chosen option, and to nothing else. */
  note?: string;
}

/** CMS-FR-TXRB: what an accepted submission returns. */
export interface QuestionAnswersSubmitted {
  discussion: Discussion;
  /**
   * DQA-FR-CIRK: the comment a fresh turn names as its trigger — the **final**
   * answer of the set, so one submission dispatches one turn per active agent
   * rather than one per question (DQA-FR-QMEA).
   */
  finalAnswerCommentId: string;
}
