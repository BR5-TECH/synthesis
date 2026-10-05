/**
 * What a review draws **over** the document rather than in it
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-12,
 * DCR-FR-BQNL).
 *
 * The changes themselves are decorations, which ProseMirror owns. These two are
 * not: the action chip is three real controls that must be focusable and
 * disableable, and the gutter map is a track beside the text rather than part
 * of it. Both live inside the scroller, so they travel with the passage they
 * are about instead of hanging in one place while it scrolls away.
 */
import { GutterMap, HunkChip, useScrollToHunk } from "../DraftDiscussion";
import type { DecoratedHunk } from "../DraftDiscussion";
import type { EditorReview } from "./props";

export function ReviewOverlay({
  review,
  placed,
  focused,
  hostRef,
  extent,
}: {
  /** Absent while the draft holds no undecided proposal, which draws nothing. */
  review: EditorReview | undefined;
  placed: DecoratedHunk[];
  /** The change under review, if the document could place it. */
  focused: DecoratedHunk | null;
  hostRef: React.RefObject<HTMLDivElement | null>;
  /** The size of the document the marks are placed within. */
  extent: number;
}) {
  // DCR-FR-30: the document follows the review. Called before the early return
  // so the hook order does not change with the presence of a proposal.
  useScrollToHunk(hostRef, review?.proposalId ?? null, review?.focused ?? null);
  if (!review) return null;
  return (
    <>
      {focused && (
        <HunkChip
          hunk={focused}
          hostRef={hostRef}
          busy={review.busy}
          onAccept={() => review.onAccept(focused.id)}
          onReject={() => review.onReject(focused.id)}
          onDiscuss={() => review.onDiscuss(focused.id)}
        />
      )}
      <GutterMap
        hunks={placed}
        focused={review.focused}
        extent={extent}
        onSelect={review.onFocus}
      />
    </>
  );
}
