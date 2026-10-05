/**
 * Plain talk, and the author's own message
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-QJFE,
 * DDS-FR-LWPC, DDS-FR-VTKD, DDS-FR-BRHN, DDS-FR-FKZL).
 *
 * Neither is boxed (DDS-FR-BRHN). A message the author wrote carries a rail and
 * an indent and nothing else — no fill, no border, no side of its own
 * (DDS-FR-VTKD) — so the accent keeps the one meaning DDS-FR-CVLM gives it.
 *
 * Consecutive messages by one author are **one block** with one author line
 * (DDS-FR-QJFE): the block takes the whole run and stacks the bodies, so a
 * paragraph that follows another does not repeat a name the reader just read.
 */
import { CommentMarkdown } from "../../CommentMarkdown";
import { CommentAttachmentList } from "../../CommentAttachments";
import { formatRelative } from "../../ProjectPicker";
import { Icon } from "../../icons";
import type { AgentRoster } from "../../agentTags";
import { stripCitationMarkers } from "../../../text/citationMarkers";
import {
  participantTitle,
  type Comment,
} from "../../../types";
import { useParticipantLabel } from "../../../state/projectIdentity";

export interface MessageBlockProps {
  /** DDS-FR-QJFE: the whole run by one author, oldest first. */
  comments: Comment[];
  /** DDS-FR-VTKD: the rail is drawn for a message the reader wrote. */
  own: boolean;
  roster: AgentRoster;
  threadId: string;
  draftId?: string;
  /**
   * DDS-FR-FKZL: **Quote**, at the trailing end of the author line. Absent where
   * the conversation offers no composer to seed — a locked or resolved thread,
   * or one holding a standing question set (per
   * `../../../../specifications/ui/CMT-comments.md` CMT-FR-12, and
   * `../../../../specifications/ui/DQA-discussion-question-answering.md`
   * DQA-FR-VJHT).
   */
  onQuote?: (commentId: string, host: HTMLElement | null) => void;
  /** The index of the run's first comment in the thread, for Quote's own name. */
  firstIndex: number;
  /** CMT-FR-13: who an earlier quoted message was by. */
  authorOf: (commentId: string) => string;
  /**
   * CMT-FR-15 / CMT-FR-16: the thread's own menu, which the message the thread
   * opens with carries. Absent on every other block.
   */
  actions?: React.ReactNode;
}

/**
 * The message of a grouped run the reader has selected text in, or `null` where
 * the selection lies outside the block entirely.
 *
 * DDS-FR-QJFE merges a run under one author line, so one Quote control stands
 * for several comments. Which one it quotes is decided by where the selection
 * is, and falls back to the block's own head where there is no selection at all
 * — exactly what a block of one message does.
 */
function partOfSelection(block: Element | null): HTMLElement | null {
  if (block === null) return null;
  const selection = typeof window === "undefined" ? null : window.getSelection();
  if (!selection || selection.rangeCount === 0) return null;
  if (selection.isCollapsed) return null;
  const node = selection.getRangeAt(0).commonAncestorContainer;
  const from = node instanceof Element ? node : node.parentElement;
  const part = from?.closest<HTMLElement>(".dds-message__part[data-comment-id]");
  return part && block.contains(part) ? part : null;
}

export function MessageBlock({
  comments,
  own,
  roster,
  threadId,
  draftId,
  onQuote,
  firstIndex,
  authorOf,
  actions,
}: MessageBlockProps) {
  const participantLabel = useParticipantLabel();
  const head = comments[0];
  const title = participantTitle(head.author);

  return (
    <div
      className="dds-message"
      data-own={own || undefined}
      data-testid="dds-message"
    >
      {/* DDS-FR-LWPC: one row — name, role, time. The time is never emphasised
          and never floated into a corner; the row's own order is what puts it
          after the name. */}
      <div className="dds-message__head">
        <span
          className="dds-message__author"
          data-agent={head.author.kind === "agent" || undefined}
        >
          {participantLabel(head.author)}
        </span>
        {/* CTA-FR-KFUF / CTA-FR-KYPK: the role the agent answered under, from
            this comment's own snapshot. Absent and empty both render nothing. */}
        {title && <span className="dds-message__role">{title}</span>}
        <span className="dds-stream__time" title={head.createdAt}>
          {formatRelative(head.createdAt)}
        </span>
        {/* DDS-FR-FKZL: the actions sit at the trailing end of this row. The
            pointer reveals them; the keyboard reaches them at all times, which
            is why they are in the document rather than mounted on hover. */}
        <span className="dds-message__actions">
          {onQuote && (
            <button
              type="button"
              className="btn btn--ghost btn--icon-xs dds-message__action"
              aria-label={`Quote comment ${firstIndex + 1} by ${participantLabel(head.author)}`}
              title="Quote"
              // Without this the button's own mousedown collapses the selection
              // before the click lands, and quoting an excerpt could never work.
              onMouseDown={(e) => e.preventDefault()}
              onClick={(e) => {
                // CMT-FR-13: the quote names the comment it came out of, which
                // in a grouped run is whichever of the run's messages the reader
                // selected in — never the one whose name heads the block. A
                // reply three messages later still has to name what it answered.
                const part = partOfSelection(
                  e.currentTarget.closest(".dds-message"),
                );
                onQuote(
                  part?.dataset.commentId ?? head.id,
                  part ?? e.currentTarget.closest(".dds-message"),
                );
              }}
            >
              <Icon.Quote size={12} />
            </button>
          )}
          {actions}
        </span>
      </div>

      {/* DDS-FR-QJFE: the run's bodies, stacked under the one author line. */}
      {comments.map((comment) => (
        <div
          className="dds-message__part"
          key={comment.id}
          data-comment-id={comment.id}
        >
          {/* CMT-FR-13: an earlier message this one quotes, as an attributed
              block above the body that quotes it. */}
          {comment.quotes.map((quote, at) => (
            <div className="dds-message__quote" key={at}>
              <span className="dds-message__quote-label t-eyebrow">
                quoting {authorOf(quote.commentId)}
              </span>
              {/* CMT-FR-AWIE: an excerpt is part of a body, so a provider
                  citation marker in it is hidden on the same terms. */}
              <span className="dds-message__quote-text">
                {stripCitationMarkers(quote.excerpt)}
              </span>
            </div>
          ))}
          {/* DDS-FR-GBWP: the body's tone is the stylesheet's; AGT-FR-29: a tag
              that matches an enrolled agent is marked in it. */}
          <CommentMarkdown
            body={comment.body}
            agentRoster={roster}
            className="dds-message__body"
          />
          {/* CMT-FR-48: attachments below the body they belong to. A change
              reference is not among them — DDS-FR-XHRB draws that as a row of
              the column instead. */}
          <CommentAttachmentList
            threadId={threadId}
            draftId={draftId}
            attachments={comment.attachments.filter(
              (a) => a.kind !== "proposal" && a.kind !== "promptProposal",
            )}
          />
        </div>
      ))}
    </div>
  );
}
