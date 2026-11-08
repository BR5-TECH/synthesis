/**
 * CVP-FR-TWRL: positional auto-follow and the unread rule of a message list.
 *
 * The list opens at its foot. While the author is at the foot, a message that
 * arrives, content that grows, and a smaller list box keep the list there
 * (CVP-FR-MPDK). Once the author scrolls away, the list does not
 * move: an arrival marks the discussion unread, and reaching the foot or
 * activating the unread indicator clears it. There is no scroll-lock control.
 * Where the author is looking already says what they want.
 *
 * Moved from `../DraftDiscussion/useAutoFollow.ts` and keyed by the discussion
 * id in the session store (`../../state/discussionSession.ts`) instead of by a
 * draft, so the scroll position and the unread state survive a change of owner
 * surface (CVP-FR-47).
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useSyncExternalStore } from "react";

import {
  clearDiscussionUnread,
  getDiscussionSession,
  noteDiscussionArrivals,
  setDiscussionScroll,
  setDiscussionSeen,
  subscribeDiscussionSessions,
} from "../../state/discussionSession";

/**
 * How near the foot still counts as the tail, in pixels.
 *
 * A list pinned to its foot is rarely at exactly zero: a fractional device pixel
 * ratio, a smooth scroll in flight, and a caret in the composer growing the list
 * all leave a pixel or two behind.
 */
export const TAIL_SLACK = 24;

export interface DiscussionFollow {
  /** The scroller the messages are in. */
  ref: React.RefObject<HTMLDivElement | null>;
  /** Attach to the scroller's `onScroll`. */
  onScroll: () => void;
  /** CVP-FR-TWRL: scroll to the first unread message and clear the unread state. */
  goToFirstUnread: () => void;
}

export interface DiscussionFollowInput {
  discussionId: string;
  /** False while the surface renders nothing, so there is no scroller to move. */
  enabled: boolean;
  /**
   * The ids that count as a new message when they appear: the comments and a
   * question set. They mark the discussion unread.
   */
  countedIds: readonly string[];
  /**
   * Every id that adds a block to the list. A pending or a failed contribution
   * moves the list when the author is at the foot and marks nothing unread.
   */
  growthIds: readonly string[];
}

function atFoot(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= TAIL_SLACK;
}

/** Any distance below the visible part, so a follow lands at the very foot. */
function belowFoot(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight > 0.5;
}

export function useDiscussionFollow({
  discussionId,
  enabled,
  countedIds,
  growthIds,
}: DiscussionFollowInput): DiscussionFollow {
  const ref = useRef<HTMLDivElement | null>(null);
  /** The discussion whose scroll position this surface has restored. */
  const restoredFor = useRef<string | null>(null);
  /**
   * CVP-FR-SWDH: the offset of the last move that the list made itself. A scroll
   * event that still reads it was caused by that move, not by the author.
   */
  const pinnedTop = useRef<number | null>(null);

  /** CVP-FR-TWRL, CVP-FR-MPDK: go to the very foot. */
  const pin = useCallback((el: HTMLElement) => {
    el.scrollTop = el.scrollHeight;
    pinnedTop.current = el.scrollTop;
  }, []);

  const onScroll = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    // CVP-FR-SWDH: the event of a move that the list made itself can arrive
    // after the content grew again. It does not take the list off the foot.
    if (
      pinnedTop.current !== null &&
      el.scrollTop === pinnedTop.current &&
      getDiscussionSession(discussionId).atTail
    ) {
      if (belowFoot(el)) pin(el);
      setDiscussionScroll(discussionId, el.scrollTop, true);
      return;
    }
    pinnedTop.current = null;
    setDiscussionScroll(discussionId, el.scrollTop, atFoot(el));
  }, [discussionId, pin]);

  const goToFirstUnread = useCallback(() => {
    const el = ref.current;
    const first = getDiscussionSession(discussionId).firstUnreadId;
    if (el && first !== null) {
      const target =
        Array.from(el.querySelectorAll<HTMLElement>("[data-comment-id]")).find(
          (node) => node.dataset.commentId === first,
        ) ?? null;
      if (target && typeof target.scrollIntoView === "function") {
        target.scrollIntoView({ block: "start" });
      } else if (!target) {
        el.scrollTop = el.scrollHeight;
      }
    }
    clearDiscussionUnread(discussionId);
  }, [discussionId]);

  // Layout rather than effect: the list has grown by the time this runs and the
  // browser has not painted yet, so a followed message is on screen in the frame
  // it arrives in rather than one frame later at the old position.
  const countedKey = countedIds.join("\u0000");
  const growthKey = growthIds.join("\u0000");
  useLayoutEffect(() => {
    if (!enabled) {
      restoredFor.current = null;
      return;
    }
    const el = ref.current;
    if (!el) return;
    const held = getDiscussionSession(discussionId);
    const seen = held.seenIds === null ? null : new Set(held.seenIds);

    if (restoredFor.current !== discussionId) {
      restoredFor.current = discussionId;
      if (seen === null || held.atTail) {
        // CVP-FR-TWRL: a list opens at its foot, because a conversation is
        // opened to see what was said last.
        pin(el);
        setDiscussionScroll(discussionId, el.scrollTop, true);
      } else {
        // CVP-FR-47: a list the author scrolled away from comes back where they
        // left it.
        el.scrollTop = held.scrollTop;
        pinnedTop.current = null;
      }
    }

    if (seen !== null) {
      const arrived = countedIds.filter((id) => !seen.has(id));
      const grown = growthIds.some((id) => !seen.has(id));
      if (grown || arrived.length > 0) {
        if (getDiscussionSession(discussionId).atTail) {
          pin(el);
        } else {
          // The view does not move. The arrivals land below the divider and the
          // indicator says how many there are.
          noteDiscussionArrivals(discussionId, arrived);
        }
      }
    }
    setDiscussionSeen(discussionId, growthIds);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [discussionId, enabled, countedKey, growthKey]);

  // CVP-FR-CUKV: the author posted, so the list goes to its foot. A request
  // made before this surface mounted is not acted on: a list that mounts at the
  // tail opens at its foot anyway (CVP-FR-TWRL), and a list the author scrolled
  // away from after that request comes back where they left it (CVP-FR-47).
  const footRequest = useSyncExternalStore(
    subscribeDiscussionSessions,
    () => getDiscussionSession(discussionId).footRequest,
    () => getDiscussionSession(discussionId).footRequest,
  );
  const handledFoot = useRef<{ id: string; request: number } | null>(null);
  useLayoutEffect(() => {
    const held = handledFoot.current;
    handledFoot.current = { id: discussionId, request: footRequest };
    // Only a request that grew the count is acted on. A session that was
    // cleared starts its count again at zero, and that is no post.
    if (held === null || held.id !== discussionId || footRequest <= held.request) return;
    const el = ref.current;
    if (!enabled || !el) return;
    pin(el);
    setDiscussionScroll(discussionId, el.scrollTop, true);
  }, [discussionId, enabled, footRequest, pin]);

  // CVP-FR-MPDK: content that grows with no new id (a status, a tool list, an
  // image that loads) and a list box that becomes smaller keep a list at its
  // foot. The rows are observed, because the scroller keeps its own size when
  // only its content grows.
  useEffect(() => {
    const el = ref.current;
    if (!enabled || !el || typeof ResizeObserver === "undefined") return;
    const follow = () => {
      if (getDiscussionSession(discussionId).atTail && belowFoot(el)) pin(el);
    };
    const observer = new ResizeObserver(follow);
    observer.observe(el);
    const observeRows = () => {
      for (const row of Array.from(el.children)) observer.observe(row);
    };
    observeRows();
    const rows =
      typeof MutationObserver === "undefined"
        ? null
        : new MutationObserver((records) => {
            for (const record of records) {
              record.removedNodes.forEach((node) => {
                if (node instanceof Element) observer.unobserve(node);
              });
            }
            observeRows();
          });
    rows?.observe(el, { childList: true });
    return () => {
      observer.disconnect();
      rows?.disconnect();
    };
  }, [discussionId, enabled, pin]);

  return { ref, onScroll, goToFirstUnread };
}
