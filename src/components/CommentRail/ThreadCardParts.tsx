/**
 * Two pieces of a thread card that both of its presentations place
 * (`../../../specifications/ui/CMT-comments.md` CMT-FR-15, CMT-FR-16, CMT-FR-HQNV,
 * and `../../../specifications/ui/CTA-comment-agent-turns.md` CTA-FR-MGVJ,
 * CTA-FR-ARBB).
 *
 * They live beside `./ThreadCard.tsx` rather than inside it because the card and
 * the draft discussion column's stream both draw them and neither changes what
 * they say — the stream puts the actions at the trailing end of an author line
 * (per `../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-FKZL) and the
 * outcomes at the foot of its content column, and that placement is the whole of
 * the difference.
 */
import { Icon } from "../icons";
import type { AgentTurn, Discussion } from "../../types";
import { failedTurnDetail } from "./messages";

export interface ThreadActionsProps {
  thread: Discussion;
  blocked: boolean;
  menuOpen: boolean;
  onToggleMenu: () => void;
  onCloseMenu: () => void;
  onSetLock: (threadId: string, locked: boolean) => Promise<void>;
  onSetResolved: (threadId: string, resolved: boolean) => Promise<void>;
}

/** CMT-FR-15 / CMT-FR-16 / CMT-FR-HQNV: the thread's own actions and its menu. */
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
        aria-expanded={menuOpen}
        onClick={(e) => {
          e.stopPropagation();
          onToggleMenu();
        }}
      >
        ⋯
      </button>
      {menuOpen && (
        /* CVP-FR-44: an open menu is a surface Escape dismisses before it dismisses anything around it. */
        <div
          className="comment-card__menu"
          role="menu"
          data-surface-open="true"
        >
          {/* CMT-FR-15/CMT-FR-16: exactly two entries, each reversible
              and each independent of the other. */}
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
    {/* CTA-FR-MGVJ: a turn that could not reach its model leaves a failed
        contribution in the place the answer would have taken — the same shape
        and the same position as the pending contribution it replaced,
        attributed to the same agent, distinguished from both a delivered
        comment and a pending one by its own marker and its own wording. It
        names no failure code and no provider: what the author can act on is
        the Retry beside it, and the diagnosis is in the session log. */}
    {/* CTA-FR-ARBB: a turn whose images the selected provider and model could
        not take renders an unsupported-image notice beside the contribution
        it produced. It is a **status and not an error**: it sits with the
        agent's own contribution so a reader can tell which turn it is about,
        it is announced through a status role, it is reachable and readable
        from the keyboard alone, and it is legible without colour. It blocks
        nothing — the conversation is answered, a running turn is still
        cancellable, a failed turn's Retry is unaffected, and the author may
        ask again at any moment. */}
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
          <span className="comment__failed-marker" aria-hidden="true">
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
        {/* CTA-FR-XZUO: a locked thread takes no further contribution and so no
            further turn — the backend would refuse this `discussion_locked`, and a
            control whose only outcome is a refusal is worse than none. A
            RESOLVED thread keeps it: withholding a composer there is the
            rail's own decision about what to invite, not a refusal, and a turn
            the author already asked for still belongs to them. */}
        {!thread.locked && (
          <div className="comment__failed-actions">
            <button
              className="btn btn--ghost btn--sm comment__retry"
              data-testid="comment-failed-retry"
              /**
               * CTA-FR-ESNJ: off while *this* dispatch is being initiated, so
               * one activation is one turn however impatiently it is pressed —
               * and a second conversation's offer stays takeable meanwhile.
               *
               * `aria-disabled` rather than `disabled`, because a disabled
               * button is not focusable: disabling it under the author's own
               * keyboard blurs it, and focus lands on the document body with
               * nothing to return it — so a refusal that restores this very
               * control leaves a keyboard user tabbing from the top of the
               * document to reach the offer they were already on. The control
               * reads as unavailable, keeps focus, and declines the activation
               * here; the guarantee that one activation is one turn is the
               * hook's synchronous guard rather than this attribute.
               */
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
