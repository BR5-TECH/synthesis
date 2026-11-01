/**
 * The post of the comment that ends a speech (`CTA-comment-agent-turns.md`
 * CTA-FR-SZOC, CTA-FR-QWLB, `VOM-voice-mode.md` VOM-FR-MVLA).
 *
 * Voice mode posts a transcript through the same path as a typed comment. Each
 * dispatch path asks this module which agents the speech already reached: the
 * comment reaches their turns through "end speech turn", so it is not
 * dispatched to them again. The dispatch path also gives the id of the posted
 * comment, which "end speech turn" needs.
 */
import type { Discussion } from "../types";

interface SpeechPost {
  discussionId: string;
  /** The body of the speech comment. Only the dispatch of this body is claimed. */
  body: string;
  reached: ReadonlySet<string>;
  commentId: string | null;
}

let active: SpeechPost | null = null;

/**
 * Voice mode starts the post of a speech's comment into this discussion, with
 * the body that the post sends.
 */
export function beginSpeechPost(
  discussionId: string,
  reached: readonly string[],
  body: string,
): void {
  active = { discussionId, body: body.trim(), reached: new Set(reached), commentId: null };
}

/** The post ended. Gives the id of the comment that it posted, if one posted. */
export function finishSpeechPost(): string | null {
  const commentId = active?.commentId ?? null;
  active = null;
  return commentId;
}

/**
 * Called by each dispatch path with the discussion as it stands with the new
 * comment appended, and the body of that comment. Gives the nicknames that the
 * dispatch must skip. A comment that is not the speech comment, such as a
 * typed comment that posts during the speech post, claims nothing.
 */
export function claimSpeechPost(
  thread: Pick<Discussion, "id" | "comments">,
  body: string,
): ReadonlySet<string> {
  if (
    active === null ||
    active.discussionId !== thread.id ||
    active.commentId !== null ||
    active.body !== body.trim()
  ) {
    return EMPTY;
  }
  active.commentId = thread.comments[thread.comments.length - 1]?.id ?? null;
  return active.reached;
}

const EMPTY: ReadonlySet<string> = new Set();

export function resetSpeechPostsForTest(): void {
  active = null;
}
