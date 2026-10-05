import type { PullRequestReviewState, PullRequestTimelineItem } from "../../types";
import { CommentMarkdown } from "../CommentMarkdown";
import { formatIsoInstant } from "./format";

const KIND_LABEL: Record<PullRequestTimelineItem["kind"], string> = {
  comment: "Comment",
  review: "Review",
  review_comment: "Review comment",
  commit: "Commit",
  event: "Event",
};

const REVIEW_LABEL: Record<PullRequestReviewState, string> = {
  approved: "Approved",
  changes_requested: "Changes requested",
  commented: "Commented",
  dismissed: "Dismissed",
  pending: "Pending",
};

/** The words of an event name: `review_requested` reads as "review requested". */
const eventWords = (name: string) => name.replace(/_/g, " ");

/**
 * GIT-FR-FNQA: one entry of a pull request's timeline.
 *
 * Each kind renders its own way, and the kind is always named in words: a
 * comment and a review comment differ by their label and their path, never by
 * colour alone. A body renders as Markdown through the same safe renderer the
 * comment rail uses; nothing is inserted as raw HTML.
 */
export function TimelineItem({ item }: { item: PullRequestTimelineItem }) {
  return (
    <li
      className="git-pr__item"
      data-kind={item.kind}
      data-testid={`git-pr-item-${item.kind}`}
    >
      <div className="git-pr__item-head">
        <span className="badge">{KIND_LABEL[item.kind] ?? item.kind}</span>
        {item.kind === "review" && item.reviewState && (
          <span
            className={
              item.reviewState === "approved"
                ? "badge badge--ok"
                : item.reviewState === "changes_requested"
                  ? "badge badge--danger"
                  : "badge"
            }
          >
            {REVIEW_LABEL[item.reviewState] ?? item.reviewState}
          </span>
        )}
        {item.kind === "event" && item.event && (
          <span className="git-pr__event">{eventWords(item.event)}</span>
        )}
        {item.kind === "commit" && item.commitId && (
          <span className="t-hash">{item.commitId.slice(0, 7)}</span>
        )}
        {item.actor && <span className="git-pr__actor">{item.actor}</span>}
        <time
          className="t-meta git-pr__time"
          dateTime={item.createdAt}
          title={formatIsoInstant(item.createdAt)}
        >
          {formatIsoInstant(item.createdAt)}
        </time>
      </div>
      {item.kind === "review_comment" && item.path && (
        <div className="git__file-path git-pr__path" title={item.path}>
          {item.path}
        </div>
      )}
      {item.kind === "commit" && item.subject && (
        <div className="git-pr__subject">{item.subject}</div>
      )}
      {item.body && item.kind !== "commit" && (
        <CommentMarkdown body={item.body} className="git-pr__body" />
      )}
    </li>
  );
}
