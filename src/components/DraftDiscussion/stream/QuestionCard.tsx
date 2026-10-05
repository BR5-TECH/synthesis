/**
 * A submitted exchange, as one card
 * (`../../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-KYWR, DQA-FR-ZPGM, DQA-FR-BHXT).
 *
 * The question, its recorded options, the option the author chose marked **in
 * place**, and the note or the author's own words in the footer. The answer is
 * stated once: the column renders no second message for it (DQA-FR-KYWR).
 *
 * It is **read-only** (DQA-FR-BHXT). This is history — the two comments behind
 * it are committed and immutable (per `../../../../specifications/ui/CMT-comments.md`
 * CMT-FR-14) — so the card offers no control that changes the answer and none
 * that reopens the question. Where either body is not one `./questionParse.ts`
 * can read, the caller draws the two ordinary comments instead and loses the
 * grouping alone (DQA-FR-TSJD).
 */
import { CommentMarkdown } from "../../CommentMarkdown";
import { Icon } from "../../icons";
import { TypeChip } from "./TypeChip";
import { formatRelative } from "../../ProjectPicker";
import type { QuestionCardModel } from "./questionParse";
import type { AgentRoster } from "../../agentTags";
import { stripCitationMarkers } from "../../../text/citationMarkers";
import {
  participantName,
  participantTitle,
  type Comment,
} from "../../../types";

export interface QuestionCardProps {
  card: QuestionCardModel;
  /** The agent-authored half, which the card is headed by and timed from. */
  question: Comment;
  roster: AgentRoster;
  /**
   * CMT-FR-12: **Quote**, on the terms every other message in the column offers
   * it (DDS-FR-FKZL).
   *
   * DQA-FR-BHXT makes the card read-only about the **answer** and about nothing
   * else: the question is still an ordinary comment of the conversation
   * (DQA-FR-NKAX), so it is still quotable wherever a composer stands to seed.
   */
  onQuote?: (commentId: string, host: HTMLElement | null) => void;
  /** The question's position in the thread, for Quote's own name (CMT-FR-35). */
  index: number;
  /**
   * CMT-FR-15 / CMT-FR-16 / CMT-FR-HQNV: the thread's own menu, where this card
   * draws the comment the thread opens with. Absent on every other card.
   */
  actions?: React.ReactNode;
}

export function QuestionCard({
  card,
  question,
  roster,
  onQuote,
  index,
  actions,
}: QuestionCardProps) {
  return (
    <article className="dds-question" data-testid="dds-question-card">
      <div className="dds-question__head">
        {/* DDS-FR-MCUP: what kind of row this is. */}
        <TypeChip kind="question" />
        {/* DDS-FR-LWPC: name then time, on one row, the time never emphasised. */}
        <span className="dds-question__author">
          {participantName(question.author)}
        </span>
        {/* CTA-FR-KFUF / CTA-FR-KYPK: the role the agent asked under, from this
            comment's own snapshot. A card states it on the same terms every
            other message in the column does (DDS-FR-LWPC). */}
        {participantTitle(question.author) && (
          <span className="dds-message__role">
            {participantTitle(question.author)}
          </span>
        )}
        <span className="dds-stream__time" title={question.createdAt}>
          {formatRelative(question.createdAt)}
        </span>
        {(onQuote || actions) && (
          <span className="dds-message__actions">
            {onQuote && (
            <button
              type="button"
              className="btn btn--ghost btn--icon-xs dds-message__action"
              aria-label={`Quote comment ${index + 1} by ${participantName(question.author)}`}
              title="Quote"
              // Without this the button's own mousedown collapses the selection
              // before the click lands, and quoting an excerpt could never work.
              onMouseDown={(e) => e.preventDefault()}
              onClick={(e) =>
                onQuote(question.id, e.currentTarget.closest(".dds-question"))
              }
            >
              <Icon.Quote size={12} />
            </button>
            )}
            {actions}
          </span>
        )}
      </div>

      {/* DQA-FR-XQOR: the recorded text, exactly as it was recorded. */}
      <div className="dds-question__ask">
        <CommentMarkdown
          body={card.text}
          agentRoster={roster}
          className="dds-question__ask-body"
        />
      </div>

      {/* DQA-FR-KYWR: the recorded options, with the chosen one marked among
          them rather than restated below them. */}
      <ol className="dds-question__options">
        {card.options.map((option) => (
          <li
            key={option.number}
            className="dds-question__option"
            data-chosen={option.chosen || undefined}
            // DQA-FR-EWLB: the choice is carried by more than the fill. A reader
            // who cannot tell the two grounds apart still hears which one was
            // taken.
            aria-current={option.chosen ? "true" : undefined}
          >
            <span className="dds-question__number" aria-hidden="true">
              {option.number}
            </span>
            {/* DQA-FR-JADB: a provider citation marker is hidden. */}
            <span className="dds-question__value">
              {stripCitationMarkers(option.value)}
            </span>
          </li>
        ))}
      </ol>

      {/* DQA-FR-ZPGM: the note beside a chosen option, or the author's own
          words where they answered in them. Both stand in the same footer, in
          the same shape; only the label differs. */}
      {card.footer && (
        <div className="dds-question__footer" data-kind={card.footer.kind}>
          <span className="dds-question__footer-label t-eyebrow">
            {card.footer.kind === "note" ? "your note" : "your answer"}
          </span>
          <span className="dds-question__footer-text">{card.footer.text}</span>
        </div>
      )}
    </article>
  );
}
