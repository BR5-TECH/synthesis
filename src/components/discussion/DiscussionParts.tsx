/**
 * The small parts every discussion surface draws
 * (`CMT-comments.md` CMT-FR-15, CMT-FR-16, CMT-FR-WZTE, CMT-FR-RPLC;
 * `CTA-comment-agent-turns.md` CTA-FR-MGVJ, CTA-FR-ARBB;
 * `CVP-conversation-presentation.md` CVP-FR-44, CVP-FR-45).
 *
 * Moved from `../CommentRail/ThreadCardParts.tsx` without change to what they
 * say. `ThreadActions` no longer carries Detach and Maximize, because there are
 * no presentation modes to move between (CVP-FR-HBKV).
 */
import { Icon } from "../icons";
import { isFragmentTargeted } from "../../types";
import type { AgentTurn, Discussion } from "../../types";
import { failedTurnDetail } from "../CommentRail/messages";

export interface ThreadActionsProps {
  thread: Discussion;
  blocked: boolean;
  menuOpen: boolean;
  onToggleMenu: () => void;
  onCloseMenu: () => void;
  onSetLock: (threadId: string, locked: boolean) => Promise<void>;
  onSetResolved: (threadId: string, resolved: boolean) => Promise<void>;
}

/** CMT-FR-15 / CMT-FR-16: the thread's own actions and its menu. */
export function ThreadActions({
  thread,
  blocked,
  menuOpen,
  onToggleMenu,
  onCloseMenu,
  onSetLock,
  onSetResolved,
}: ThreadActionsProps) {
  return (
    <>
      <button
        className="btn btn--ghost btn--icon comment-card__menu-button"
        aria-label={`Thread actions for ${thread.id}`}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onClick={(e) => {
          e.stopPropagation();
          onToggleMenu();
        }}
      >
        ⋯
      </button>
      {menuOpen && (
        <div className="comment-card__menu" role="menu" data-surface-open="true">
          {/* CMT-FR-15 / CMT-FR-16: exactly two entries, each reversible and
              each independent of the other. */}
          <button
            role="menuitem"
            disabled={blocked}
            onClick={() => {
              onCloseMenu();
              void onSetLock(thread.id, !thread.locked);
            }}
          >
            {thread.locked ? "Unlock thread" : "Lock thread"}
          </button>
          <button
            role="menuitem"
            disabled={blocked}
            onClick={() => {
              onCloseMenu();
              void onSetResolved(thread.id, !thread.resolved);
            }}
          >
            {thread.resolved ? "Reopen thread" : "Mark resolved"}
          </button>
        </div>
      )}
    </>
  );
}

export interface TurnOutcomesProps {
  /** CTA-FR-ARBB: the turn whose images the model could not take. */
  imageNotice?: AgentTurn;
  /** CTA-FR-RQQJ: this conversation's current recoverable failure. */
  failedTurn?: AgentTurn;
  thread: Discussion;
  retryingTurnIds?: ReadonlySet<string>;
  onRetryTurn?: (turnId: string) => void;
}

/** CTA-FR-MGVJ / CTA-FR-ARBB: what a turn left behind, at the foot of the messages. */
export function TurnOutcomes({
  imageNotice,
  failedTurn,
  thread,
  retryingTurnIds,
  onRetryTurn,
}: TurnOutcomesProps) {
  return (
    <>
      {/* CTA-FR-ARBB: a status and not an error. It is announced through a
          status role, reachable from the keyboard, and legible without colour. */}
      {imageNotice && (
        <p
          className="comment__image-notice"
          data-testid="comment-image-notice"
          data-turn-id={imageNotice.id}
          role="status"
          tabIndex={0}
        >
          Images were not sent; text and image metadata were sent instead
        </p>
      )}
      {/* CTA-FR-MGVJ: a failed contribution in the place the answer would have
          taken, with its own marker and its own wording. */}
      {failedTurn && (
        <div
          className="comment comment--failed"
          data-own={false}
          data-testid="comment-failed"
          data-turn-id={failedTurn.id}
          aria-live="polite"
        >
          <div className="comment__head">
            <span className="comment__author" data-agent="true">
              ✦ {failedTurn.nickname}
            </span>
            <span
              className="comment__failed-marker"
              role="img"
              aria-label="Failed"
            >
              <Icon.AlertTriangle size={11} />
            </span>
          </div>
          <p className="comment__body comment__body--failed">
            {failedTurn.nickname} could not produce a response.
          </p>
        {failedTurnDetail(failedTurn) && (
          <p
            className="comment__body comment__body--failed"
            data-testid="comment-failed-tls"
          >
            {failedTurnDetail(failedTurn)}
          </p>
        )}
          {/* CTA-FR-XZUO: a locked thread takes no further turn, so it offers no
              Retry. A resolved one keeps it. */}
          {!thread.locked && (
            <div className="comment__failed-actions">
              <button
                className="btn btn--ghost btn--sm comment__retry"
                data-testid="comment-failed-retry"
                /* CTA-FR-ESNJ: `aria-disabled` rather than `disabled`, so the
                   control keeps keyboard focus while one dispatch is in flight.
                   The guarantee of one turn per activation is the hook's
                   synchronous guard. */
                aria-disabled={retryingTurnIds?.has(failedTurn.id) ?? false}
                onClick={() => {
                  if (retryingTurnIds?.has(failedTurn.id)) return;
                  onRetryTurn?.(failedTurn.id);
                }}
              >
                <Icon.Refresh size={11} /> Retry
              </button>
            </div>
          )}
        </div>
      )}
    </>
  );
}

/** Where a discussion stands against the thing it is about. */
export type OwnerAvailability = "available" | "orphaned" | "unavailable";

/**
 * CMT-FR-WZTE / CVP-FR-44: the state words of a discussion.
 *
 * Every state is a word. **Fragment** or **Whole** says whether it has a
 * fragment target, and the lifecycle (Locked, Resolved) and the owner state
 * (Orphaned, Owner unavailable) follow it. None rests on a colour.
 */
export function DiscussionBadges({
  discussion,
  availability,
  ownerLabel,
}: {
  discussion: Discussion;
  availability: OwnerAvailability;
  ownerLabel?: string;
}) {
  const fragment = isFragmentTargeted(discussion);
  return (
    <div className="discussion-badges" data-testid="discussion-badges">
      <span
        className="discussion-badge"
        data-kind={fragment ? "fragment" : "whole"}
        data-testid="discussion-target-badge"
      >
        {fragment ? "Fragment" : "Whole"}
      </span>
      {discussion.locked && (
        <span className="discussion-badge" data-kind="locked">
          <Icon.Lock size={11} /> Locked
        </span>
      )}
      {discussion.resolved && (
        <span className="discussion-badge" data-kind="resolved">
          Resolved
        </span>
      )}
      {availability === "orphaned" && (
        <span className="discussion-badge" data-kind="orphaned">
          Orphaned
        </span>
      )}
      {availability === "unavailable" && (
        <span
          className="discussion-badge"
          data-kind="unavailable"
          role="status"
          data-testid="discussion-owner-unavailable"
        >
          <Icon.AlertTriangle size={11} /> Owner unavailable
          {ownerLabel ? `: ${ownerLabel} no longer exists in this project` : ""}
        </span>
      )}
    </div>
  );
}
