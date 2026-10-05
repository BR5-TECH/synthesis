/**
 * What the draft discussion column draws, in the order it draws it
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-QJFE,
 * DDS-FR-NDSA, DDS-FR-XHRB, and `../../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-KYWR).
 *
 * A conversation is a flat list of comments. The column is not: consecutive
 * messages by one author read as one block, a submitted exchange reads as one
 * question card, a change reference reads as a row of its own, and a day
 * boundary reads as a divider. This module turns the one into the other.
 *
 * It is **derived when the column renders and stored nowhere** (DDS-FR-QJFE).
 * Nothing here reads a record the conversation does not already hold, and a
 * surface that ignored this module would draw the same comments ungrouped.
 */
import { isProposalAttachment, type Attachment, type Comment } from "../../../types";
import type { PairedHalf } from "../../CommentRail/questionPairs";

/** A change reference a comment carries, with the comment it came from. */
export type ChangeReference = Extract<
  Attachment,
  { kind: "proposal" | "promptProposal" }
>;

/** One thing the column draws. */
export type StreamItem =
  /** DDS-FR-NDSA: the day the messages below it were written on. */
  | { kind: "date"; key: string; label: string }
  /**
   * DDS-FR-QJFE: one or more consecutive messages by the same author, under one
   * author line.
   */
  | { kind: "message"; key: string; comments: Comment[]; own: boolean }
  /** DQA-FR-KYWR: a submitted question and the answer beside it, as one card. */
  | { kind: "question"; key: string; question: Comment; answer: Comment | null }
  /** DDS-FR-XHRB: a change reference, as one row. */
  | {
      kind: "change";
      key: string;
      comment: Comment;
      reference: ChangeReference;
    };

export interface StreamInput {
  comments: readonly Comment[];
  /** DQA-FR-FBWO: which comments are the two halves of one submitted exchange. */
  pairing: ReadonlyMap<string, PairedHalf>;
  /** CVP-FR-32: whether a comment is one the reader wrote themselves. */
  isOwn: (comment: Comment) => boolean;
  /** DDS-FR-CLBK: the first comment that is on screen; everything before it is folded. */
  visibleFrom?: number;
  /**
   * Comments the host draws something above — the unread divider of DDS-FR-GKMT
   * and the fold row of DDS-FR-CLBK.
   *
   * Grouping must not close over one of these. Two messages by one author merge
   * into a block (DDS-FR-QJFE), and a block has one edge: a divider that belongs
   * between them has nowhere to go, so the reading position is silently lost.
   * Naming them here breaks the run at exactly the point something is drawn.
   */
  breakBefore?: ReadonlySet<string>;
  /** DDS-FR-NDSA: what "today" means, so the labels are testable. */
  now?: Date;
}

/**
 * DDS-FR-NDSA: the day a divider names, in the column's own register — lowercase,
 * and a date only where the day is far enough back to need one.
 */
export function dayLabel(iso: string, now: Date): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  const days = Math.round(
    (startOfDay(now).getTime() - startOfDay(at).getTime()) / 86_400_000,
  );
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  const month = at
    .toLocaleString("en-GB", { month: "short" })
    .toLowerCase();
  return `${at.getDate()} ${month}`;
}

function startOfDay(at: Date): Date {
  return new Date(at.getFullYear(), at.getMonth(), at.getDate());
}

/** The calendar day a comment was written on, as a key a divider is keyed by. */
function dayKey(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return `${at.getFullYear()}-${at.getMonth()}-${at.getDate()}`;
}

/** Whether two comments were written by the same participant (DDS-FR-QJFE). */
function sameAuthor(one: Comment, two: Comment): boolean {
  const a = one.author;
  const b = two.author;
  if (a.kind !== b.kind) return false;
  if (a.kind === "human" && b.kind === "human") return a.login === b.login;
  if (a.kind === "agent" && b.kind === "agent") return a.agentId === b.agentId;
  return false;
}

/**
 * DDS-FR-XHRB: the change references a comment carries.
 *
 * A comment may carry a body and a reference together. The body is then an
 * ordinary message and the reference is a row after it — the row is what opens
 * the review, and a body that says why is still worth reading.
 */
function referencesOf(comment: Comment): ChangeReference[] {
  return comment.attachments.filter(isProposalAttachment);
}

/**
 * The column's items, in order.
 *
 * Grouping stops at anything that is not plain talk: a question card and a
 * change row are items of their own, so the messages on either side of one are
 * two blocks rather than one (DDS-FR-QJFE).
 */
export function streamItems(input: StreamInput): StreamItem[] {
  const { comments, pairing, isOwn } = input;
  const now = input.now ?? new Date();
  const from = input.visibleFrom ?? 0;
  const breaks = input.breakBefore ?? new Set<string>();

  const items: StreamItem[] = [];
  let day: string | null = null;
  /** The block the next plain message would join, where one is still open. */
  let open: Extract<StreamItem, { kind: "message" }> | null = null;

  const divide = (comment: Comment) => {
    const key = dayKey(comment.createdAt);
    if (key === day) return;
    day = key;
    open = null;
    const label = dayLabel(comment.createdAt, now);
    if (label !== "") {
      items.push({ kind: "date", key: `date:${key}`, label });
    }
  };

  for (let at = 0; at < comments.length; at += 1) {
    const comment = comments[at];
    // DQA-FR-KYWR: the answer half is drawn inside its question's card and never
    // as a message of its own, so the transcript states an answer once.
    //
    // Only where that question is **on screen**, though. A fold that lands
    // between the two (DDS-FR-CLBK) leaves the answer with no card to be drawn
    // in, and skipping it there would drop it from the transcript with nothing
    // to show that it had been dropped — so it renders as the ordinary comment
    // it is (DQA-FR-GRUV).
    if (pairing.get(comment.id) === "answer" && at - 1 >= from) continue;
    if (at < from) continue;

    divide(comment);

    if (pairing.get(comment.id) === "question") {
      const answer = comments[at + 1] ?? null;
      open = null;
      items.push({
        kind: "question",
        key: comment.id,
        question: comment,
        answer,
      });
      continue;
    }

    const body = comment.body.trim();
    const references = referencesOf(comment);
    /**
     * Everything the block draws besides the change rows: the body, the quotes
     * it carries, and any attachment that is not a change reference.
     *
     * A comment that announces a change with an empty body still holds all
     * three, and drawing only its row would drop them from the transcript.
     */
    const carries =
      body !== "" ||
      comment.quotes.length > 0 ||
      comment.attachments.some((a) => !isProposalAttachment(a));

    // CMT-FR-15: the comment the thread opens with carries the thread's own
    // actions, whatever it holds — so it always gets a block to carry them on,
    // exactly as it did when every comment was drawn the same way.
    if (carries || references.length === 0 || at === 0) {
      if (
        open !== null &&
        !breaks.has(comment.id) &&
        sameAuthor(open.comments[0], comment)
      ) {
        open.comments.push(comment);
      } else {
        open = {
          kind: "message",
          key: comment.id,
          comments: [comment],
          own: isOwn(comment),
        };
        items.push(open);
      }
    }

    for (const reference of references) {
      open = null;
      items.push({
        kind: "change",
        key: `${comment.id}:${referenceKey(reference)}`,
        comment,
        reference,
      });
    }
  }

  return items;
}

/** What makes one reference distinct from another on the same comment. */
function referenceKey(reference: ChangeReference): string {
  return `${reference.kind}:${reference.proposalId}`;
}

/**
 * Every comment an item draws, in the order the conversation holds them.
 *
 * What the host draws above a comment (DDS-FR-GKMT, DDS-FR-CLBK) is placed by
 * the item that covers it. A break before the answer half of a submitted
 * exchange therefore lands above the whole card: the question and the answer are
 * one exchange, so a reading position inside it is a reading position above it.
 */
export function coveredBy(item: StreamItem): string[] {
  if (item.kind === "message") return item.comments.map((c) => c.id);
  if (item.kind === "question") {
    return item.answer ? [item.question.id, item.answer.id] : [item.question.id];
  }
  if (item.kind === "change") return [item.comment.id];
  return [];
}
