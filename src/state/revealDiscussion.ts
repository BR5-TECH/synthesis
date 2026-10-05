/**
 * The one route that reveals a discussion
 * (`CVP-conversation-presentation.md` CVP-FR-06, CVP-FR-54).
 *
 * A Comments panel row, a note's Discuss action, an owner's own affordance, and
 * any other entry that names a conversation call {@link revealDiscussion}. It
 * routes on the surface registry and on the owner:
 *
 * - A surface that already shows the discussion is focused.
 * - A conversation tab that already shows it is focused.
 * - A note, or an owner that no longer resolves, opens the conversation tab.
 * - An available artifact or draft opens its owner surface.
 *
 * No branch creates a second surface for one discussion. The route is
 * synchronous, so two requests that arrive together cannot interleave. The
 * route only decides. The shell supplies the deps that open tabs.
 */
import { logDebug } from "../logging";
import {
  focusDiscussion,
  requestPendingFocus,
  type FocusTarget,
} from "./discussionFocus";
import { announceDiscussion } from "./discussionAnnouncer";
import { isOwnerAvailable } from "./ownerAvailability";
import {
  discussionTargetKey,
  type DiscussionTarget,
  type FragmentTarget,
} from "../types";

/** What a route knows about the discussion it reveals. */
export interface DiscussionReveal {
  /** Absent only for a note that has no discussion yet (CVP-FR-60). */
  discussionId?: string;
  target: DiscussionTarget;
  fragmentTarget?: FragmentTarget | null;
  resolved?: boolean;
  /** The owner's name, for `Chat: <owner>` and for announcements. */
  ownerLabel: string;
  /** The conversation's subject. It follows the name. */
  subject: string;
  /** The caller already knows that the owner no longer resolves. */
  ownerUnavailable?: boolean;
}

/** Where the route landed. */
export type RevealOutcome = "focused" | "tab" | "owner";

/** What the shell does for the route. */
export interface RevealDeps {
  /** The id of the conversation tab already showing this discussion, if any. */
  findConversationTab: (reveal: DiscussionReveal) => string | null;
  /** Open or focus the conversation tab. Never a second tab for one discussion. */
  openConversationTab: (reveal: DiscussionReveal) => void;
  /** Open or focus the owner surface and bring the discussion into view. */
  openOwner: (reveal: DiscussionReveal) => void;
}

/** The key a discussion's session and pending focus are held under. */
export function revealKey(reveal: DiscussionReveal): string {
  return reveal.discussionId ?? discussionTargetKey(reveal.target);
}

/** Whether the route must use the conversation tab for this discussion. */
export function needsConversationTab(reveal: DiscussionReveal): boolean {
  if (reveal.target.kind === "note") return true;
  return reveal.ownerUnavailable === true || !isOwnerAvailable(reveal.target);
}

export function revealDiscussion(
  reveal: DiscussionReveal,
  deps: RevealDeps,
): RevealOutcome {
  const key = revealKey(reveal);
  const land: FocusTarget = reveal.discussionId ? "discussion" : "composer";

  if (
    reveal.discussionId &&
    focusDiscussion(reveal.discussionId, land) === "focused"
  ) {
    logDebug(["frontend"], "a discussion was revealed in its open surface", {
      discussionId: reveal.discussionId,
    });
    announceDiscussion(`${reveal.subject}: focused in ${reveal.ownerLabel}.`);
    return "focused";
  }

  const existingTab = deps.findConversationTab(reveal);
  if (existingTab !== null || needsConversationTab(reveal)) {
    requestPendingFocus(key, land);
    deps.openConversationTab(reveal);
    logDebug(["frontend"], "a discussion was revealed in a conversation tab", {
      key,
      owner: reveal.target.kind,
    });
    announceDiscussion(`Chat: ${reveal.ownerLabel}. ${reveal.subject} opened in a tab.`);
    return "tab";
  }

  requestPendingFocus(key, land);
  deps.openOwner(reveal);
  logDebug(["frontend"], "a discussion was revealed in its owner surface", {
    key,
    owner: reveal.target.kind,
  });
  announceDiscussion(`${reveal.subject}: opened in ${reveal.ownerLabel}.`);
  return "owner";
}
