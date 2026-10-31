/**
 * The one surface every owner renders a discussion with
 * (`CVP-conversation-presentation.md` CVP-FR-SDMQ, CVP-FR-TWRL, CVP-FR-05).
 *
 * It renders the message list, the question-set block, the Resolve control, the fragment-or-whole badge, the orphaned and unavailable states,
 * the unread divider and indicator, and the shared `DiscussionComposer`. Owners
 * keep their own layouts and place this component inside them. No owner draws
 * its own message list or composer.
 *
 * Three things about its life:
 *
 * - **One instance.** It registers with the focus registry
 *   (`../../state/discussionFocus.ts`). A second surface of one discussion
 *   renders nothing until the first goes.
 * - **State outside the component.** The unsent text, the attachments, the
 *   scroll position, the unread state, and the outstanding turns are in the
 *   session store, so an owner change loses none of them (CVP-FR-47).
 * - **Same markup in every owner.** `variant` picks only the measure the markup
 *   is set at. It is not a presentation mode and nothing moves a discussion
 *   between variants (CVP-FR-HBKV).
 *
 * Moved from `../CommentRail/ThreadCard.tsx`, with the draft column's reading
 * position from `../DraftDiscussion/DiscussionColumn.tsx`.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { agentRoster } from "../agentTags";
import { DiscussionQuestions } from "../DiscussionQuestions";
import { submitQuestionAnswers } from "../DiscussionQuestions/submit";
import { StreamMessages } from "../DraftDiscussion/stream";
import { commentErrorMessage, turnFailureMessage } from "./messages";
import { pairedHalves } from "./questionPairs";
import { useDiscussionRuntime } from "../../hooks/useDiscussionRuntime";
import { usePendingContributions } from "../../hooks/usePendingContributions";
import { ActivityRows } from "./ToolActivity";
import { barDeadlineOf } from "../../state/turnDeadline";
import { logDebug } from "../../logging";
import {
  consumePendingFocus,
  useSurfaceSlot,
  type FocusTarget,
} from "../../state/discussionFocus";
import {
  setComposerQuotes,
  getDiscussionSession,
  useDiscussionSession,
} from "../../state/discussionSession";
import { useQuestionSet } from "../../state/questionSets";
import {
  AGENT_TURN_FAILURES,
  discussionDraftId,
  discussionFragment,
  type AgentTurn,
  type Discussion,
  type FragmentTarget,
  type Participant,
  type ProjectAgent,
  type AttachmentInput,
  type CommentQuote,
} from "../../types";
import { DiscussionComposer } from "./DiscussionComposer";
import { DiscussionMessages, isOwnComment } from "./DiscussionMessages";
import {
  DiscussionBadges,
  ThreadActions,
  TurnOutcomes,
  type OwnerAvailability,
} from "./DiscussionParts";
import { UnreadDivider, UnreadIndicator } from "./UnreadMarks";
import { useDiscussionFollow } from "./useDiscussionFollow";
import { useThreadMenu } from "./useThreadMenu";
import { useParticipantLabel } from "../../state/projectIdentity";

export interface DiscussionSurfaceProps {
  discussion: Discussion;
  /**
   * The owner layout this surface sits in, for the focus registry and for
   * announcements: `"column"`, `"panel"`, `"tab"`, or any name the
   * owner chooses.
   */
  owner: string;
  agents: readonly ProjectAgent[];
  /** CVP-FR-32: who the author writes as, so their own messages are told apart. */
  identity?: Participant | null;
  /** CMT-FR-24: why posting is unavailable, drawn above the messages. */
  identityBlock?: {
    message: string;
    route?: string | null;
    /** CMT-FR-25: the one refusal the author can clear from here. */
    action?: { label: string; onActivate: () => void };
  } | null;
  /** The composer is off for want of an identity. Reading is unaffected. */
  disabled?: boolean;
  /** An operation of the surface is in flight, so Resolve and Post are off. */
  blocked?: boolean;
  /**
   * The measure the one markup is set at. `card` for a margin, `embedded` for a
   * wide surface such as the conversation tab, `stream` for the draft column.
   * It changes where things are set and nothing about what a message says.
   */
  variant?: "card" | "embedded" | "stream";
  /** CVP-FR-45 / CMT-FR-19: whether the owner or the fragment can still be found. */
  availability?: OwnerAvailability;
  /** Names what is missing when the owner is unavailable. */
  ownerLabel?: string;
  /**
   * The fragment quote was activated. The owner scrolls its body to the range
   * and highlights it (CMT-FR-28). Without it the quote is plain text.
   */
  onFocusFragment?: (fragment: FragmentTarget, discussion: Discussion) => void;

  onReply: (
    discussionId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<unknown>;
  onSetResolved: (discussionId: string, resolved: boolean) => Promise<void>;
  /**
   * DDS-FR-MZQJ: where the discussion's menu stands when the owner draws it
   * outside the messages — the draft column's header. Absent, the menu stands
   * on the opening comment (CMT-FR-15). `null` while the host is not mounted.
   */
  actionsHost?: HTMLElement | null;

  /**
   * Overrides for what the surface reads for itself. Absent, the surface reads
   * the outstanding turns from the session store and the failed contribution
   * and the notice from the backend (`../../hooks/useDiscussionRuntime.ts`), so
   * every owner shows the same for one discussion.
   */
  pendingTurns?: readonly AgentTurn[];
  failedTurn?: AgentTurn;
  imageNotice?: AgentTurn;
  retryingTurnIds?: ReadonlySet<string>;
  onRetryTurn?: (turnId: string) => void;
  onCancelTurn?: (turnId: string) => void;
  turnFailure?: string;
  error?: string;
  /** False reads and follows no turn. For an owner that supplies all of them. */
  liveTurns?: boolean;

  /** Whether the owner has this discussion in focus, and how it learns of it. */
  focused?: boolean;
  onFocus?: () => void;

  /** DDS-FR-CLBK: the first message to render, so a long head can be folded. */
  visibleFrom?: number;
  /** DDS-FR-CLBK: the fold row, drawn between messages. */
  beforeComment?: (commentId: string, index: number) => React.ReactNode;
  /** ACT-FR-QWNP: raised to put the caret in the composer. */
  composerFocusSignal?: number;
  /** Show the post accelerator hint beside the composer (DDS-FR-HQTX). */
  showAcceleratorHint?: boolean;
}

export function DiscussionSurface(props: DiscussionSurfaceProps) {
  const participantLabel = useParticipantLabel();
  const { discussion: thread, owner, agents } = props;
  const stream = props.variant === "stream";
  const embedded = props.variant === "embedded";
  const availability = props.availability ?? "available";
  const session = useDiscussionSession(thread.id);
  const runtime = useDiscussionRuntime(thread.id, props.liveTurns !== false);
  const menu = useThreadMenu();
  const rootRef = useRef<HTMLDivElement>(null);

  // CTA-FR-ZOLW, CTA-FR-XMCQ: one pending contribution for each agent.
  const { pendingTurns, onCancelTurn, toolCallsByRow } = usePendingContributions(
    props.pendingTurns ?? runtime.pendingTurns,
    props.onCancelTurn ?? runtime.cancelTurn,
  );
  // CVP-FR-JSHF / CVP-FR-ZRWW: every running turn, not one row per agent.
  const runningTurns = props.pendingTurns ?? runtime.pendingTurns;
  const turnDeadline = useMemo(() => barDeadlineOf(runningTurns), [runningTurns]);
  const failedTurn = props.failedTurn ?? runtime.failedTurn;
  const imageNotice = props.imageNotice ?? runtime.imageNotice;
  const retryingTurnIds = props.retryingTurnIds ?? runtime.retryingTurnIds;
  const onRetryTurn = props.onRetryTurn ?? runtime.retryTurn;
  const turnFailure = props.turnFailure ?? runtime.turnFailure;

  const roster = useMemo(() => agentRoster([...agents]), [agents]);
  const pairing = useMemo(() => pairedHalves(thread.comments), [thread.comments]);
  // DQA-FR-NRZB: the set a discussion holds, whatever it is about.
  const questionSet = useQuestionSet(thread.id);
  const quoted = discussionFragment(thread);

  /**
   * CVP-FR-42: move focus into this surface. On the composer where it held
   * focus when the surface last lost it, and on the discussion otherwise.
   */
  const focusInto = useCallback(
    (target: FocusTarget) => {
      const root = rootRef.current;
      if (!root) return;
      const wantComposer =
        target === "composer" ||
        (target === "discussion" && getDiscussionSession(thread.id).focusWasComposer);
      const field = wantComposer
        ? root.querySelector<HTMLElement>("textarea, [data-voice-record]")
        : null;
      if (field) field.focus();
      else root.focus();
    },
    [thread.id],
  );

  // CVP-FR-02 / CVP-FR-05: one surface per discussion.
  const active = useSurfaceSlot(thread.id, owner, focusInto);

  useEffect(() => {
    if (!active) return;
    logDebug(["frontend"], "a discussion surface mounted", {
      discussionId: thread.id,
      owner,
    });
  }, [active, thread.id, owner]);

  // CVP-FR-42: a reveal that ran before this surface mounted left a request.
  useEffect(() => {
    if (!active) return;
    const wanted = consumePendingFocus(thread.id);
    if (wanted) focusInto(wanted);
  }, [active, thread.id, focusInto]);

  // CVP-FR-TWRL: the ids that count as new, and the ids that add a block.
  const countedIds = useMemo(
    () => [
      ...thread.comments.map((c) => c.id),
      ...(questionSet ? [`qs:${questionSet.setId}`] : []),
    ],
    [thread.comments, questionSet],
  );
  const growthIds = useMemo(
    () => [
      ...countedIds,
      // One block for each agent: a second turn of that agent adds none.
      ...pendingTurns.map((t) => `turn:${t.agentId}`),
      ...(failedTurn ? [`failed:${failedTurn.id}`] : []),
    ],
    [countedIds, pendingTurns, failedTurn],
  );
  const follow = useDiscussionFollow({
    discussionId: thread.id,
    enabled: active,
    countedIds,
    growthIds,
  });

  // NAW-FR-32 / CMT-FR-28: bring a focused card into view.
  useEffect(() => {
    const el = rootRef.current;
    if (!props.focused || !el || !active) return;
    if (typeof el.scrollIntoView !== "function") return;
    el.scrollIntoView({ block: "nearest" });
  }, [props.focused, active]);

  const authorOf = (commentId: string): string => {
    const c = thread.comments.find((x) => x.id === commentId);
    return c ? participantLabel(c.author) : "";
  };

  /**
   * CMT-FR-12: seed the composer from a comment already in this discussion — the
   * part the reader has selected, or the whole message when nothing is selected.
   * A selection outside this comment is not what the reader is quoting.
   */
  const quoteFrom = (commentId: string, host: HTMLElement | null) => {
    const selection = typeof window !== "undefined" ? window.getSelection() : null;
    const inside =
      selection !== null &&
      selection.rangeCount > 0 &&
      host !== null &&
      host.contains(selection.getRangeAt(0).commonAncestorContainer);
    // DQA-FR-VJHT: while a set stands, no human contribution is offered.
    if (questionSet) return;
    const selected = inside ? selection.toString().trim() : "";
    const excerpt =
      selected !== ""
        ? selected
        : (thread.comments.find((c) => c.id === commentId)?.body ?? "").trim();
    if (excerpt === "") return;
    const held = getDiscussionSession(thread.id).quotes;
    if (held.some((q) => q.commentId === commentId && q.excerpt === excerpt)) return;
    setComposerQuotes(thread.id, [...held, { commentId, excerpt }]);
  };

  // CMT-FR-11 / CMT-FR-16: a resolved discussion takes no comment, so it offers
  // no composer and no Quote. A discussion stored as locked arrives resolved
  // (CMS-FR-WKZR).
  const composerOpen = !thread.resolved;
  const quoteOffered = composerOpen && !props.disabled && !questionSet;

  const threadActions = (
    <ThreadActions
      thread={thread}
      blocked={props.blocked ?? false}
      menuOpen={menu.open}
      onToggleMenu={menu.toggle}
      onCloseMenu={menu.close}
      onSetResolved={props.onSetResolved}
    />
  );
  // DDS-FR-MZQJ: an owner that hosts the menu itself takes it out of the
  // messages, so no message carries a second copy of it.
  const hosted = props.actionsHost !== undefined;
  const messageActions = hosted ? undefined : threadActions;
  const turnOutcomes = (
    <TurnOutcomes
      imageNotice={imageNotice}
      failedTurn={failedTurn}
      thread={thread}
      retryingTurnIds={retryingTurnIds}
      onRetryTurn={onRetryTurn}
    />
  );

  // CMT-FR-32: a counter that advances when the overflow menu opens, which is
  // what the Attach menu dismisses on. A counter rather than the boolean,
  // because the boolean also changes on close, and closing the menu is what
  // opening the Attach menu does.
  const [menuOpenings, setMenuOpenings] = useState(0);
  useEffect(() => {
    if (menu.open) setMenuOpenings((n) => n + 1);
  }, [menu.open]);

  /**
   * CVP-FR-TWRL: the unread divider is drawn above the first unread comment. A
   * first unread that is a question set has no comment to sit above, so the
   * divider closes the list instead.
   */
  const unreadOpen = session.unreadCount > 0 && session.firstUnreadId !== null;
  const dividerAbove = (commentId: string): React.ReactNode =>
    unreadOpen && commentId === session.firstUnreadId ? (
      <UnreadDivider count={session.unreadCount} />
    ) : null;
  const beforeComment = (commentId: string, index: number): React.ReactNode => {
    const outer = props.beforeComment?.(commentId, index);
    const divider = dividerAbove(commentId);
    if (!outer && !divider) return null;
    return (
      <>
        {outer}
        {divider}
      </>
    );
  };
  const dividerAtEnd =
    unreadOpen &&
    !thread.comments.some((c) => c.id === session.firstUnreadId) ? (
      <UnreadDivider count={session.unreadCount} />
    ) : null;

  if (!active) return null;

  const composerShown = !questionSet && composerOpen;
  const operationError = props.error ?? session.error;

  return (
    <div
      ref={rootRef}
      className={
        stream
          ? "comment-card comment-card--stream"
          : embedded
            ? "comment-card comment-card--embedded"
            : "comment-card"
      }
      data-focused={props.focused ?? false}
      data-resolved={thread.resolved}
      data-discussion-id={thread.id}
      data-discussion-owner={owner}
      data-target={quoted ? "fragment" : "whole"}
      data-availability={availability}
      data-testid={`comment-thread-${thread.id}`}
      role="region"
      aria-label={`Discussion ${thread.id}`}
      tabIndex={-1}
      onClick={props.onFocus}
    >
      <DiscussionBadges
        discussion={thread}
        availability={availability}
        ownerLabel={props.ownerLabel}
      />

      {/* CMT-FR-19: the quote the discussion was targeted at, in place of an
          alignment when it is orphaned, and the way back to the passage when it
          is not. A whole-target discussion has no quote (CMT-FR-55). In an
          embedded surface the header of the owner carries it. */}
      {quoted !== null &&
        !embedded &&
        (props.onFocusFragment ? (
          <button
            type="button"
            className="comment-card__quote"
            title={quoted.quote}
            data-testid="discussion-fragment-quote"
            onClick={(e) => {
              e.stopPropagation();
              props.onFocusFragment?.(quoted, thread);
            }}
          >
            ❝ {quoted.quote}
          </button>
        ) : (
          <div
            className="comment-card__quote"
            title={quoted.quote}
            data-testid="discussion-fragment-quote"
          >
            ❝ {quoted.quote}
          </div>
        ))}

      {props.identityBlock && (
        <div className="comment-rail__blocked" role="status">
          {props.identityBlock.message}
          {props.identityBlock.route && (
            <div className="comment-rail__route">{props.identityBlock.route}</div>
          )}
          {props.identityBlock.action && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={props.identityBlock.action.onActivate}
            >
              {props.identityBlock.action.label}
            </button>
          )}
        </div>
      )}

      {/* CVP-FR-TWRL: the scrolling half. The composer below is the half that
          stays put. A log region, so a message that arrives is announced
          politely and never takes focus (CVP-FR-43). */}
      <div
        ref={follow.ref}
        className="comment-card__messages"
        role="log"
        aria-live="polite"
        aria-relevant="additions"
        aria-label="Messages"
        onScroll={follow.onScroll}
        data-testid="discussion-messages"
      >
        {stream ? (
          <StreamMessages
            comments={thread.comments}
            pairing={pairing}
            isOwn={(comment) => isOwnComment(comment.author, props.identity)}
            roster={roster}
            threadId={thread.id}
            draftId={discussionDraftId(thread)}
            visibleFrom={props.visibleFrom ?? 0}
            beforeComment={beforeComment}
            onQuote={quoteOffered ? quoteFrom : undefined}
            authorOf={authorOf}
            threadActions={messageActions}
            pendingTurns={pendingTurns}
            toolCallsByRow={toolCallsByRow}
            onCancelTurn={onCancelTurn}
            footer={
              <>
                {dividerAtEnd}
                {turnOutcomes}
                {/* CTA-FR-ZBIB: the tool lists of the turns that ended. */}
                <ActivityRows rows={session.activityRows} variant="stream" />
              </>
            }
          />
        ) : (
          <DiscussionMessages
            discussion={thread}
            roster={roster}
            identity={props.identity}
            embedded={embedded}
            pairing={pairing}
            pendingTurns={pendingTurns}
            toolCallsByRow={toolCallsByRow}
            onCancelTurn={onCancelTurn}
            onQuote={quoteOffered ? quoteFrom : undefined}
            authorOf={authorOf}
            threadActions={messageActions}
            beforeComment={beforeComment}
            visibleFrom={props.visibleFrom ?? 0}
            afterComments={dividerAtEnd}
            footer={
              <>
                {turnOutcomes}
                {/* CTA-FR-ZBIB: the tool lists of the turns that ended. */}
                <ActivityRows rows={session.activityRows} />
              </>
            }
          />
        )}
      </div>

      {/* CVP-FR-TWRL: the way to the first unread message. Outside the scroller,
          so its appearance moves no message. */}
      <UnreadIndicator
        count={session.unreadCount}
        onActivate={follow.goToFirstUnread}
      />

      {/* DQA-FR-NRZB: the block sits between the message list and the composer.
          DQA-FR-TVMH: outside the discussion's history entirely. */}
      {questionSet && composerOpen && (
        <DiscussionQuestions
          set={questionSet}
          onSubmit={(answers) =>
            submitQuestionAnswers(thread.id, questionSet.setId, answers)
          }
          onSubmitted={() => {
            // DQA-FR-IPFD: the block and its unsent draft go together. Both
            // follow from the backend reporting that the discussion now holds
            // no set, which the submission has already published.
          }}
          describeError={commentErrorMessage}
        />
      )}

      {/* DQA-FR-PXNC: while a set stands the composer is replaced by one line
          saying the questions above are to be answered first. */}
      {questionSet && composerOpen && (
        <p
          className="discussion-questions__composer-note"
          data-testid="discussion-questions-composer-note"
        >
          Answer the questions above to continue the discussion.
        </p>
      )}

      {/* CMT-FR-16: a resolved discussion renders no composer, and its
          messages stay fully readable. */}
      {composerShown && (
        <DiscussionComposer
          mode="reply"
          discussion={thread}
          agents={agents}
          disabled={props.disabled}
          blocked={props.blocked}
          error={props.error}
          onReply={props.onReply}
          onAttachOpen={menu.close}
          attachDismissSignal={menuOpenings}
          dismissSignal={menu.open ? 1 : 0}
          focusSignal={props.composerFocusSignal}
          showAcceleratorHint={props.showAcceleratorHint ?? stream}
          turnDeadline={turnDeadline}
        />
      )}

      {/* CMT-FR-34: where there is no composer to carry it, a refused operation
          (a Resolve, a Reopen, a Retry) still renders at the foot. */}
      {!composerShown && operationError && (
        <div className="comment-card__error" role="alert">
          {commentErrorMessage(operationError)}
        </div>
      )}

      {/* CTA-FR-IGNT: a turn that failed renders its typed failure inline at the
          foot, leaving every comment untouched. */}
      {/* CTA-FR-QTNB: a turn refused because the author resolved the
          discussion leaves nothing behind — no failure and no Retry. */}
      {turnFailure && turnFailure !== AGENT_TURN_FAILURES.discussionResolved && (
        <div
          className="comment-card__error"
          role="alert"
          data-testid="comment-turn-error"
        >
          {turnFailureMessage(turnFailure)}
        </div>
      )}
      {hosted && props.actionsHost && createPortal(threadActions, props.actionsHost)}
    </div>
  );
}
