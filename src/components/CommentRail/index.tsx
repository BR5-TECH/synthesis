/**
 * The Editor's comment rail (`CMT-comments.md`).
 *
 * The empty margin the Editor tab's page already leaves beside itself
 * (CMT-FR-01), holding one card per thread anchored to a range of the artifact's
 * Markdown source. The cards take no column of their own and the page never
 * narrows for them. Each card sits at its anchor's line, travels with the body
 * as it scrolls, and is pushed down only as far as it must go to clear the one
 * above (CMT-FR-27), so a reader sees at a glance which paragraph is under
 * discussion.
 *
 * That is the arrangement while there is field enough to hold it. On a tab too
 * narrow for a card even with the page slid as far leading as it goes, the whole
 * rail moves into a column below the page (CMT-FR-64) — the tab decides which,
 * and passes it in as `arrangement`. Below the page the cards align to nothing
 * and travel with nothing; everything else about them is unchanged.
 *
 * The rail is **not** a floating overlay of the main window: opening the search
 * overlay or the project switcher neither closes it nor is closed by it
 * (STB-FR-12). Its own transient surfaces — a card's overflow menu — are anchored
 * within the tab and mutually exclusive with each other alone (CMT-FR-32).
 *
 * Anchoring itself lives in `../state/commentAnchors`, kept pure so the cases
 * that matter (a passage that moved, one that was deleted, one that repeats) are
 * testable without a DOM.
 */
import { useCallback, useMemo, useState } from "react";
import { DiscussionComposer, DiscussionSurface } from "../discussion";
import type { OpenDiscussionRequest } from "../discussion";
import { stackCards, type AnchoredThread } from "../../state/commentAnchors";
import type { CommentArrangement } from "../../hooks/useCommentArrangement";
import { DRAFT_KEY } from "../../hooks/useComments";
import {
  discussionFragment,
  type AgentTurn,
  type AttachmentInput,
  type CommentQuote,
  type Discussion,
  type DiscussionTarget,
  type FragmentTarget,
  type Participant,
  type ProjectAgent,
} from "../../types";

export { commentErrorMessage, turnFailureMessage } from "./messages";
export { useCardMenu } from "./useCardMenu";
export { ThreadCard } from "./ThreadCard";
export type { ControlledComposer, ThreadCardProps } from "./cardTypes";

/** Stable empty default, so the memos below do not re-run on every render. */
const EMPTY_DISCUSSIONS: readonly AnchoredThread[] = [];
const EMPTY_FAILED_TURNS: Readonly<Record<string, AgentTurn>> = {};
const EMPTY_RETRYING: ReadonlySet<string> = new Set();

/** Fallback card height before anything has been measured (CMT-FR-27). */
const DEFAULT_CARD_HEIGHT = 96;
const CARD_GAP = 8;

/** Why the rail cannot accept a comment right now (CMT-FR-24). */
export interface IdentityBlock {
  message: string;
  /** Rendered when the fix is in Global settings → GitHub (CMT-FR-26). */
  route?: string;
  /**
   * CMT-FR-25: offered when the refusal is one the author can clear from here —
   * today, choosing which stored token the project uses. A refusal with no action
   * states its reason and routes elsewhere.
   */
  action?: { label: string; onActivate: () => void };
}

export interface CommentRailProps {
  threads: readonly AnchoredThread[];
  /** Pixel offset of each thread's anchor within the editing surface. */
  anchorTops: Readonly<Record<string, number>>;
  /**
   * How far the editing surface has scrolled. `anchorTops` are in the body's own
   * content coordinates, so subtracting this is what keeps a card level with its
   * passage instead of drifting away from it the moment the author scrolls.
   */
  scrollTop: number;
  /** The identity the rail writes as, or null while none resolves. */
  identity: Participant | null;
  /** Why commenting is unavailable, when it is (CMT-FR-24–CMT-FR-26). */
  identityBlock: IdentityBlock | null;
  /** CMT-FR-33: every operation is inert while a modal blocks the tab. */
  blocked: boolean;
  /** The discussion whose card is focused, if any (CMT-FR-28). */
  focusedThreadId: string | null;
  onFocusThread: (threadId: string | null) => void;
  /**
   * CMT-FR-05: the fragment a selection started a discussion over, or null.
   * The draft card is the shared opening composer for this fragment target.
   */
  draft: { target: DiscussionTarget; fragmentTarget: FragmentTarget } | null;
  /** CMT-FR-07: posts the first comment. Resolves with the new discussion. */
  onOpenDraft: (request: OpenDiscussionRequest) => Promise<Discussion>;
  onCancelDraft: () => void;
  /** CMT-FR-28: the quote of a card was activated, so its fragment is shown. */
  onFocusFragment?: (fragment: FragmentTarget, discussion: Discussion) => void;
  onReply: (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<void>;
  onSetLock: (threadId: string, locked: boolean) => Promise<void>;
  onSetResolved: (threadId: string, resolved: boolean) => Promise<void>;
  /** Per-discussion inline error from the last failed operation (CMT-FR-34). */
  errors: Readonly<Record<string, string>>;
  /**
   * CMT-FR-17: whether the resolved disclosure is expanded. Owned by the caller
   * rather than by this component, because the rail unmounts whenever the tab
   * switches to raw-text mode — component state would collapse the disclosure on
   * every round-trip, and the requirement says the state is per artifact.
   */
  showResolved: boolean;
  onToggleResolved: () => void;
  /** CTA-FR-VQFJ: the agents the composer's mention picker offers. */
  agents: readonly ProjectAgent[];
  /**
   * CMT-FR-53 / CMT-FR-61: the whole-target discussions of the open item. They
   * render in a section pinned at the head of the rail, above the aligned cards.
   * They align to nothing, so they are held apart from `threads`: an unanchored
   * entry among the aligned cards would be read as an orphan (CMT-FR-19).
   */
  discussions?: readonly AnchoredThread[];
  /** CTA-FR-QXIG: the turns outstanding across this artifact's discussions. */
  pendingTurns: readonly AgentTurn[];
  /** CTA-FR-IGNT: the typed failure of the last turn that failed, per discussion. */
  turnFailures: Readonly<Record<string, string>>;
  /** CTA-FR-XSGX: per discussion, the turn whose images could not be sent. */
  imageNotices?: Readonly<Record<string, AgentTurn>>;
  /**
   * CTA-FR-RHPP: the discussion's current terminal **retryable** failure, per
   * discussion — rendered as a failed contribution carrying Retry (CTA-FR-MGVJ).
   */
  failedTurns?: Readonly<Record<string, AgentTurn>>;
  /** CTA-FR-ESNJ: the turns whose Retry is mid-dispatch, so those controls are off. */
  retryingTurnIds?: ReadonlySet<string>;
  onRetryTurn?: (turnId: string) => void;
  onCancelTurn: (turnId: string) => void;
  /**
   * CMT-FR-64: beside the page, or in a column below it. Owned by the tab,
   * because the arrangement is a fact about the tab's width.
   */
  arrangement?: CommentArrangement;
  /** What the owner is called, for the unavailable state (CMT-FR-RPLC). */
  ownerLabel?: string;
  /** CMT-FR-RPLC: the owner file has gone from the project. */
  ownerUnavailable?: boolean;
}

export function CommentRail(props: CommentRailProps) {
  const {
    threads,
    anchorTops,
    scrollTop,
    identity,
    identityBlock,
    blocked,
    focusedThreadId,
    onFocusThread,
    draft,
    onOpenDraft,
    onCancelDraft,
    onFocusFragment,
    errors,
    showResolved,
    onToggleResolved,
    agents,
    discussions = EMPTY_DISCUSSIONS,
    pendingTurns,
    turnFailures,
    failedTurns = EMPTY_FAILED_TURNS,
    imageNotices = EMPTY_FAILED_TURNS,
    retryingTurnIds = EMPTY_RETRYING,
    onRetryTurn,
    onCancelTurn,
    arrangement = "beside",
    ownerLabel,
    ownerUnavailable = false,
  } = props;

  /** CMT-FR-64: below the page there is no margin to align a card in. */
  const beside = arrangement === "beside";

  const [heights, setHeights] = useState<Record<string, number>>({});

  const aligned = useMemo(
    () => threads.filter((t) => t.anchor !== null && !t.thread.resolved),
    [threads],
  );
  // CMT-FR-19: orphaned discussions leave the aligned column but never the rail.
  // A whole-target discussion has no fragment and is never orphaned (CMT-FR-55).
  const orphaned = useMemo(
    () =>
      threads.filter(
        (t) =>
          t.anchor === null &&
          discussionFragment(t.thread) !== null &&
          !t.thread.resolved,
      ),
    [threads],
  );
  // CMT-FR-53: the whole-target discussions, in the order they were opened.
  const openDiscussions = useMemo(
    () => discussions.filter((t) => !t.thread.resolved),
    [discussions],
  );
  // CMT-FR-17 / CMT-FR-56: resolved discussions sit behind a disclosure at the
  // rail's end, whether or not their fragment still resolves.
  const resolved = useMemo(
    () => [...discussions, ...threads].filter((t) => t.thread.resolved),
    [discussions, threads],
  );

  const stacked = useMemo(
    () =>
      stackCards(
        aligned.map((t) => ({
          threadId: t.thread.id,
          anchorTop: anchorTops[t.thread.id] ?? 0,
        })),
        heights,
        DEFAULT_CARD_HEIGHT,
        CARD_GAP,
      ),
    [aligned, anchorTops, heights],
  );

  const measure = useCallback((threadId: string, height: number) => {
    setHeights((prev) =>
      prev[threadId] === height ? prev : { ...prev, [threadId]: height },
    );
  }, []);

  /**
   * CMT-FR-HQNV: every card is the shared surface. The rail adds the alignment,
   * the focus, and the orphaned and unavailable states. `key` is passed by each
   * call site, because React 19 does not read a key that arrives in a spread.
   */
  const renderEntry = (
    entry: AnchoredThread,
    layout: { top?: number; positioned: boolean },
  ) => {
    const id = entry.thread.id;
    const fragment = discussionFragment(entry.thread);
    const availability = ownerUnavailable
      ? "unavailable"
      : fragment !== null && entry.anchor === null
        ? "orphaned"
        : "available";
    return (
      <DiscussionSurface
        key={id}
        owner="rail"
        discussion={entry.thread}
        agents={agents}
        identity={identity}
        availability={availability}
        ownerLabel={ownerLabel}
        top={layout.top}
        positioned={layout.positioned}
        focused={focusedThreadId === id}
        onFocus={() => onFocusThread(id)}
        onFocusFragment={onFocusFragment}
        onMeasure={measure}
        disabled={identityBlock !== null}
        blocked={blocked}
        onReply={props.onReply}
        onSetLock={props.onSetLock}
        onSetResolved={props.onSetResolved}
        error={errors[id]}
        pendingTurns={pendingTurns.filter((t) => t.origin.discussionId === id)}
        turnFailure={turnFailures[id]}
        failedTurn={failedTurns[id]}
        imageNotice={imageNotices[id]}
        retryingTurnIds={retryingTurnIds}
        onRetryTurn={onRetryTurn}
        onCancelTurn={onCancelTurn}
        // The Editor's hooks supply the turns, the failures, and the notices.
        liveTurns={false}
      />
    );
  };

  return (
    <aside
      className="comment-rail"
      aria-label="Comments"
      data-arrangement={arrangement}
    >
      {identityBlock && (
        <div className="comment-rail__blocked" role="status">
          {identityBlock.message}
          {identityBlock.route && (
            <div className="comment-rail__route">{identityBlock.route}</div>
          )}
          {identityBlock.action && (
            <button
              className="btn btn--ghost btn--sm comment-rail__action"
              onClick={identityBlock.action.onActivate}
            >
              {identityBlock.action.label}
            </button>
          )}
        </div>
      )}

      {/* CMT-FR-53 / CMT-FR-62: the Discussion section, pinned at the rail's
          HEAD as the orphaned section and the resolved disclosure are pinned at
          its foot. A rail with no whole-target discussion renders no section. */}
      {openDiscussions.length > 0 && (
        <div className="comment-rail__pinned-head" data-testid="discussion-section">
          <div className="comment-rail__section-title">Discussion</div>
          {openDiscussions.map((entry) =>
            renderEntry(entry, { positioned: false }),
          )}
        </div>
      )}

      {/* The aligned layer. Each card is placed at its fragment's line in the
          body's own coordinates, and the whole layer is shifted by however far
          the body has scrolled (CMT-FR-27). The `transform` also creates a
          stacking context, which traps the draft card's z-index inside this
          layer, so a long draft never paints over the resolved disclosure. */}
      <div
        className="comment-rail__aligned"
        // CMT-FR-64: below the page the cards travel with nothing.
        style={beside ? { transform: `translateY(${-scrollTop}px)` } : undefined}
      >
        {/* CMT-FR-05: the draft card sits where the selection is, like any other.
            Its composer is the shared one in opening mode. */}
        {draft !== null && (
          <div
            className="comment-card comment-card--draft"
            style={beside ? { top: anchorTops[DRAFT_KEY] ?? 0 } : undefined}
            data-positioned={beside}
            data-testid="comment-draft"
          >
            <div className="comment-card__quote">❝ {draft.fragmentTarget.quote}</div>
            <DiscussionComposer
              mode="opening"
              target={draft.target}
              fragmentTarget={draft.fragmentTarget}
              agents={agents}
              disabled={identityBlock !== null || blocked}
              ariaLabel="New comment"
              placeholder="Comment…"
              alwaysShowCancel
              focusSignal={1}
              onCancel={onCancelDraft}
              error={errors[DRAFT_KEY]}
              onOpen={onOpenDraft}
              onOpened={onCancelDraft}
            />
          </div>
        )}
        {aligned.map((entry, i) =>
          renderEntry(entry, { top: stacked[i]?.top ?? 0, positioned: beside }),
        )}
      </div>

      {/* Discussions with no line to sit beside. They stay in the tab rather
          than in the aligned layer, so they neither scroll away nor cover a
          card. */}
      {(orphaned.length > 0 || resolved.length > 0) && (
        <div
          className="comment-rail__unaligned"
          /* CMT-FR-63: the foot is bounded by what else the margin has to show. */
          data-bounded={
            openDiscussions.length > 0 || aligned.length > 0 || draft !== null
          }
        >
          {orphaned.length > 0 && (
            <div className="comment-rail__section">
              <div className="comment-rail__section-title">Orphaned</div>
              {orphaned.map((entry) => renderEntry(entry, { positioned: false }))}
            </div>
          )}

          {resolved.length > 0 && (
            <div className="comment-rail__section">
              <button
                className="comment-rail__disclosure"
                aria-expanded={showResolved}
                onClick={onToggleResolved}
              >
                {showResolved ? "▾" : "▸"} {resolved.length} resolved{" "}
                {resolved.length === 1 ? "thread" : "threads"}
              </button>
              {showResolved &&
                resolved.map((entry) => renderEntry(entry, { positioned: false }))}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}
