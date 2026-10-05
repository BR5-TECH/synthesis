/**
 * The chronological group chat of one discussion
 * (`CVP-conversation-presentation.md` CVP-FR-32; `CMT-comments.md` CMT-FR-08,
 * CMT-FR-09, CMT-FR-10, CMT-FR-13, CMT-FR-48; `CTA-comment-agent-turns.md`
 * CTA-FR-ZOLW, CTA-FR-YOGW).
 *
 * Moved from the message list of `../CommentRail/ThreadCard.tsx` without change
 * to what a message says: the author line, the agent title, the quotes, the
 * Markdown body, the attachments, the Quote action, and the pending
 * contribution with its Cancel and its activity status.
 */
import { Fragment } from "react";

import { Icon } from "../icons";
import { CommentAttachmentList } from "../CommentAttachments";
import { CommentMarkdown } from "../CommentMarkdown";
import { formatRelative } from "../ProjectPicker";
import { activityStatus } from "../../state/agentActivity";
import type { AgentRoster } from "../agentTags";
import {
  discussionDraftId,
  participantName,
  participantTitle,
  type AgentTurn,
  type Discussion,
  type Participant,
} from "../../types";
import type { PairedHalf } from "../CommentRail/questionPairs";

/**
 * CVP-FR-32: whether this message is one the author wrote.
 *
 * Compared on the participant the backend stamped rather than on anything held
 * locally (CMT-FR-10). An agent is never the author's own, whatever it is
 * handled: a human logged in as `claude` and an agent handled `claude` must not
 * read identically.
 */
export function isOwnComment(
  author: Participant,
  identity: Participant | null | undefined,
): boolean {
  if (!identity || identity.kind !== "human" || author.kind !== "human") {
    return false;
  }
  return author.login === identity.login;
}

export interface DiscussionMessagesProps {
  discussion: Discussion;
  roster: AgentRoster;
  identity?: Participant | null;
  /** The two columns of the conversation tab read the author's own from the trailing side. */
  embedded: boolean;
  pairing: ReadonlyMap<string, PairedHalf>;
  pendingTurns: readonly AgentTurn[];
  onCancelTurn: (turnId: string) => void;
  /** Absent where the composer is absent, so no control does visibly nothing. */
  onQuote?: (commentId: string, host: HTMLElement | null) => void;
  authorOf: (commentId: string) => string;
  /** The thread's own actions, carried by the message the thread opens with. */
  threadActions: React.ReactNode;
  beforeComment?: (commentId: string, index: number) => React.ReactNode;
  visibleFrom: number;
  /** The unread divider of a first-unread that is not a comment, drawn after the last one. */
  afterComments?: React.ReactNode;
  footer: React.ReactNode;
}

export function DiscussionMessages({
  discussion: thread,
  roster,
  identity,
  embedded,
  pairing,
  pendingTurns,
  onCancelTurn,
  onQuote,
  authorOf,
  threadActions,
  beforeComment,
  visibleFrom,
  afterComments,
  footer,
}: DiscussionMessagesProps) {
  return (
    <>
      {thread.comments.map((comment, index) => (
        <Fragment key={comment.id}>
          {beforeComment?.(comment.id, index)}
          {index >= visibleFrom && (
            <div
              className="comment"
              data-comment-id={comment.id}
              /* CVP-FR-32: a message the author wrote reads from the trailing
                 side and everything said to them from the leading side, human
                 and agent alike. */
              data-own={embedded && isOwnComment(comment.author, identity)}
              /* DQA-FR-FBWO: which half of a submitted exchange this is. */
              data-pair={pairing.get(comment.id)}
            >
              <div className="comment__head">
                <span
                  className="comment__author"
                  data-agent={comment.author.kind === "agent"}
                >
                  {/* CMT-FR-10. `kit.css` `.comment__title::before` carries a
                      hidden copy of this marker, so the two must stay in step. */}
                  {comment.author.kind === "agent" && "✦ "}
                  {participantName(comment.author)}
                </span>
                <span className="comment__time" title={comment.createdAt}>
                  {formatRelative(comment.createdAt)}
                </span>
                {/* CMT-FR-12 / DQA-FR-VJHT: Quote goes wherever the composer
                    goes. */}
                {onQuote && (
                  <button
                    className="btn btn--ghost btn--icon-xs comment__quote-button"
                    // Position in the thread, not just the author: two messages
                    // by one person would otherwise share a name (CMT-FR-35).
                    aria-label={`Quote comment ${index + 1} by ${participantName(comment.author)}`}
                    title="Quote"
                    // Without this the button's mousedown collapses the
                    // selection before the click lands.
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={(e) =>
                      onQuote(comment.id, e.currentTarget.closest(".comment"))
                    }
                  >
                    <Icon.Quote size={12} />
                  </button>
                )}
                {/* CMT-FR-15 / CMT-FR-16: the thread's own menu, carried by the
                    message the thread opens with. */}
                {index === 0 && threadActions}
              </div>
              {/* CTA-FR-YOGW: the role the agent answered under, from that
                  comment's own participant and from nowhere else. */}
              {participantTitle(comment.author) && (
                <div className="comment__title" data-testid="comment-agent-title">
                  {participantTitle(comment.author)}
                </div>
              )}
              {comment.quotes.map((q, i) => (
                <div key={i} className="comment__quoted">
                  ❝ {authorOf(q.commentId)}: {q.excerpt}
                </div>
              ))}
              {/* AGT-FR-29: a tag that matches an enrolled agent is rendered
                  distinguishably. One that matches nobody stays ordinary text. */}
              <CommentMarkdown body={comment.body} agentRoster={roster} />
              {/* CMT-FR-48: below the body, an image as a thumbnail and anything
                  else as a named chip. */}
              <CommentAttachmentList
                threadId={thread.id}
                draftId={discussionDraftId(thread)}
                attachments={comment.attachments}
              />
            </div>
          )}
        </Fragment>
      ))}
      {afterComments}

      {/* CTA-FR-ZOLW: each outstanding turn renders as a pending contribution
          attributed to the agent it is waiting on, in the position its answer
          will take, with a control that cancels it. */}
      {pendingTurns.map((turn) => (
        <div
          key={turn.agentId}
          className="comment comment--pending"
          data-own={false}
          data-testid="comment-pending"
          data-turn-id={turn.id}
          aria-live="polite"
        >
          <div className="comment__head">
            <span className="comment__author" data-agent="true">
              ✦ {turn.nickname}
            </span>
            <button
              className="btn btn--ghost btn--icon-xs"
              aria-label={`Cancel ${turn.nickname}'s reply`}
              title="Cancel"
              data-testid="comment-pending-cancel"
              onClick={() => onCancelTurn(turn.id)}
            >
              <Icon.X size={11} />
            </button>
          </div>
          {/* CTA-FR-XMCQ / CTA-FR-FBJR: the activity status, worded as a
              participant who has been asked something and has not answered yet. */}
          <p
            className="comment__body comment__body--pending"
            data-testid="comment-pending-status"
          >
            {activityStatus(turn)}
          </p>
        </div>
      ))}
      {footer}
    </>
  );
}
