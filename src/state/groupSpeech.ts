/**
 * What voice mode knows of the agent groups that a comment dispatched
 * (`VOM-voice-mode.md` VOM-FR-QMBN, VOM-FR-ZAHM).
 *
 * The dispatch of a comment reports its group here. Voice mode reads two things
 * from it: which agent comments stay unspoken until the Synthesis arrives, and
 * which agent is the driver of the group. Session state in memory only.
 */
import type { AgentGroup, Comment } from "../types";

/** A `synthesis` group that has not spoken its Synthesis yet. */
interface SynthesisGroup {
  discussionId: string;
  /** The comment that the group answers. */
  triggerCommentId: string;
  /** VOM-FR-QMBN: the agents whose replies post as text and are not spoken. */
  participants: ReadonlySet<string>;
}

let muted: SynthesisGroup | null = null;
/** VOM-FR-ZAHM: the driver of the last group of each discussion, with the group. */
const drivers = new Map<string, { nickname: string; groupId: string | null }>();

/**
 * VOM-FR-QMBN, VOM-FR-ZAHM: a comment dispatched as a group. Another group
 * ends the mute of an earlier one, and a group of any other mode mutes nothing.
 */
export function noteGroupDispatched(
  discussionId: string,
  triggerCommentId: string,
  group: AgentGroup,
): void {
  if (muted?.discussionId === discussionId) muted = null;
  if (group.driver) drivers.set(discussionId, { nickname: group.driver, groupId: group.groupId });
  else drivers.delete(discussionId);
  if (group.mode !== "synthesis") return;
  const participants = group.turns
    .filter((turn) => turn.group?.role === "participant")
    .map((turn) => turn.agentId);
  if (participants.length === 0) return;
  muted = { discussionId, triggerCommentId, participants: new Set(participants) };
}

/**
 * VOM-FR-ZAHM: the driver that the dispatch of the discussion named, if any,
 * with the id of its group. The driver counts only while a turn of that group runs.
 */
export function driverOf(
  discussionId: string | null,
): { nickname: string; groupId: string | null } | null {
  return discussionId === null ? null : (drivers.get(discussionId) ?? null);
}

/**
 * VOM-FR-QMBN: whether a new comment of the discussion stays unspoken. The
 * Synthesis comment is spoken and ends the mute. A later comment of the author
 * ends it too, and the comment that the group answers does not.
 */
export function isMutedReply(discussionId: string, comment: Comment): boolean {
  if (muted === null || muted.discussionId !== discussionId) return false;
  if (comment.author.kind !== "agent") {
    if (comment.id !== muted.triggerCommentId) muted = null;
    return false;
  }
  if (comment.attachments.some((a) => a.kind === "synthesis")) {
    muted = null;
    return false;
  }
  return muted.participants.has(comment.author.agentId);
}

export function resetGroupSpeechForTest(): void {
  muted = null;
  drivers.clear();
}
