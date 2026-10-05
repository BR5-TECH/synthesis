/**
 * CVP-FR-TWRL: positional auto-follow and the unread rule of a message list.
 *
 * The list opens at its foot. While the author is at the foot, a message that
 * arrives keeps the list there. Once the author scrolls away, the list does not
 * move: an arrival marks the discussion unread, and reaching the foot or
 * activating the unread indicator clears it. There is no scroll-lock control.
 * Where the author is looking already says what they want.
 *
 * Moved from `../DraftDiscussion/useAutoFollow.ts` and keyed by the discussion
 * id in the session store (`../../state/discussionSession.ts`) instead of by a
 * draft, so the scroll position and the unread state survive a change of owner
 * surface (CVP-FR-47).
 */
import { useCallback, useLayoutEffect, useRef } from "react";

import {
  clearDiscussionUnread,
  getDiscussionSession,
  noteDiscussionArrivals,
  setDiscussionScroll,
  setDiscussionSeen,
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

export function useDiscussionFollow({
  discussionId,
  enabled,
  countedIds,
  growthIds,
}: DiscussionFollowInput): DiscussionFollow {
  const ref = useRef<HTMLDivElement | null>(null);
  /** The discussion whose scroll position this surface has restored. */
  const restoredFor = useRef<string | null>(null);

  const onScroll = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setDiscussionScroll(discussionId, el.scrollTop, atFoot(el));
  }, [discussionId]);

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
        el.scrollTop = el.scrollHeight;
        setDiscussionScroll(discussionId, el.scrollTop, true);
      } else {
        // CVP-FR-47: a list the author scrolled away from comes back where they
        // left it.
        el.scrollTop = held.scrollTop;
      }
    }

    if (seen !== null) {
      const arrived = countedIds.filter((id) => !seen.has(id));
      const grown = growthIds.some((id) => !seen.has(id));
      if (grown || arrived.length > 0) {
        if (getDiscussionSession(discussionId).atTail) {
          el.scrollTop = el.scrollHeight;
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

  return { ref, onScroll, goToFirstUnread };
}
