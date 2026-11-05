/**
 * The transcript of a comment that posted as a translation
 * (`../../../specifications/ui/CMT-comments.md` CMT-FR-HCVB).
 *
 * The body holds the translation. The words as the author spoke them stay
 * under an **Original** disclosure, collapsed by default, as plain text.
 */
import type { Comment } from "../../types";

export function SpokenOriginal({ comment }: { comment: Comment }) {
  const original = comment.spokenOriginal?.trim() ?? "";
  if (original === "") return null;
  return (
    <details className="comment__spoken-original" data-testid="comment-spoken-original">
      <summary className="comment__spoken-original-summary">Original</summary>
      <p className="comment__spoken-original-text">
        {original}
      </p>
    </details>
  );
}
