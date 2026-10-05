/**
 * The Comments panel's client-side view of the project's threads: grouping by
 * status (CMP-FR-03 / CMP-FR-04), ordering (CMP-FR-09) and filtering
 * (CMP-FR-14).
 *
 * Pure over the list the backend returned, so none of it issues a call and all
 * of it is unit-testable without rendering the panel. The backend already sorts
 * most-recently-active first (CMS-FR-32); these helpers re-establish that order
 * themselves rather than depending on it, so a row's position never turns on the
 * transport.
 */
import { ownerLabelOf } from "../state/ownerAvailability";
import { attachmentName, discussionFragment } from "../types";
import type { Comment, DiscussionListItem } from "../types";

/** CMP-FR-03: the three groups, in the fixed order the panel renders them. */
export type CommentGroupKey = "active" | "locked" | "resolved";

export const COMMENT_GROUP_ORDER: readonly CommentGroupKey[] = [
  "active",
  "locked",
  "resolved",
] as const;

const GROUP_LABELS: Record<CommentGroupKey, string> = {
  active: "Active",
  locked: "Locked",
  resolved: "Resolved",
};

export interface CommentGroup {
  key: CommentGroupKey;
  label: string;
  items: DiscussionListItem[];
}

/**
 * CMP-FR-04: which group a thread belongs to.
 *
 * Lock and resolution are independent booleans (CMS-FR-18), so a thread can
 * satisfy two of these at once. Precedence — resolved, then locked, then active
 * — is what keeps each thread in exactly one group, so the three counts sum to
 * the project's thread total and no conversation is read twice.
 */
export function groupKeyFor(item: DiscussionListItem): CommentGroupKey {
  if (item.discussion.resolved) return "resolved";
  if (item.discussion.locked) return "locked";
  return "active";
}

/** CMP-FR-06: the comment that opened the thread. */
export function firstComment(item: DiscussionListItem): Comment | undefined {
  return item.discussion.comments[0];
}

/**
 * CMP-FR-06: how many comments follow the first. Zero on a thread of one, which
 * is what the row renders no reply count for.
 */
export function replyCount(item: DiscussionListItem): number {
  return Math.max(0, item.discussion.comments.length - 1);
}

/**
 * CMP-FR-25: how many attachments the opening comment carries. Zero renders no
 * marker at all rather than a count of none.
 *
 * The opening comment alone, matching what the row shows of the thread
 * (CMP-FR-06): the row is a recognisable excerpt of a conversation, not a
 * summary of everything in it.
 */
export function attachmentCount(item: DiscussionListItem): number {
  return firstComment(item)?.attachments.length ?? 0;
}

/**
 * CMP-FR-14: does this thread survive the filter? The match is case-insensitive
 * and runs over what the row actually shows — the anchor quote, the opening
 * comment's body, its attachment names, and the artifact path — so a thread the
 * user can see the text of is a thread they can filter to. An empty query
 * matches everything.
 *
 * Attachment *names* rather than content: the panel never loads an attachment's
 * bytes (CMP-FR-26), and a filename is the part of a picture a reviewer would
 * think to type.
 */
export function matchesFilter(
  item: DiscussionListItem,
  query: string,
): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  // CMP-FR-14: a whole-target discussion has no fragment quote, so it is
  // matched on the rest. The owner label is what the row shows as its owner.
  const haystacks: string[] = [
    discussionFragment(item.discussion)?.quote ?? "",
    firstComment(item)?.body ?? "",
    ownerLabelOf(item.discussion.target),
    ...(firstComment(item)?.attachments ?? []).map(attachmentName),
  ];
  return haystacks.some((h) => h.toLowerCase().includes(needle));
}

/**
 * CMP-FR-09: most-recently-active first, across artifacts rather than gathered
 * by artifact. `updatedAt` is fixed-width UTC, so a string comparison is a
 * chronological one; the id breaks a tie so the order is stable across reads.
 */
export function sortMostRecentFirst(
  items: DiscussionListItem[],
): DiscussionListItem[] {
  return [...items].sort(
    (a, b) =>
      b.discussion.updatedAt.localeCompare(a.discussion.updatedAt) ||
      a.discussion.id.localeCompare(b.discussion.id),
  );
}

/**
 * The panel's rendered shape: the surviving threads in their three groups, in
 * fixed order, each group internally most-recently-active first.
 *
 * CMP-FR-05 / CMP-FR-14: a group holding nothing — because the project has none
 * of that kind, or because the filter excluded them all — is omitted entirely
 * rather than rendered as an empty header.
 */
export function groupThreads(
  items: DiscussionListItem[],
  query: string,
): CommentGroup[] {
  const kept = sortMostRecentFirst(items.filter((i) => matchesFilter(i, query)));
  return COMMENT_GROUP_ORDER.map((key) => ({
    key,
    label: GROUP_LABELS[key],
    items: kept.filter((i) => groupKeyFor(i) === key),
  })).filter((group) => group.items.length > 0);
}
