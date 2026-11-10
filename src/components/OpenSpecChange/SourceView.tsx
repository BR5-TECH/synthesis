/**
 * The Source of one file of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-ABUR,
 * OCW-FR-JBFZ, OCW-FR-COLR, OCW-FR-DTRX; `CMT-comments.md` CMT-FR-AYSL).
 *
 * The file's text as stored. Where the pending proposal holds hunks in the
 * file, the text carries the decorations and the chip of the shared review
 * (`../Editor/SourceReview.tsx`). On a stream, a selection offers a fragment
 * discussion of the file, as a range in this text.
 */
import { useRef, useState } from "react";

import { SourceReview } from "../Editor/SourceReview";
import type { EditorReview } from "../Editor/props";
import type { ChangeSelection } from "./ChangeDiscussion";

/** What a decorated text draws that is not the file's own text. */
const NOT_SOURCE = ".hunk--source-block, [role='note'], textarea";

/**
 * CMT-FR-AYSL: the range of the file's text a DOM range covers, or null. The
 * offsets count the file's own characters alone: the proposed text of a hunk
 * and every statement drawn over the text are skipped.
 */
export function sourceOffsets(root: HTMLElement, range: Range): { start: number; end: number } | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest(NOT_SOURCE) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  });
  let offset = 0;
  let start: number | null = null;
  let end: number | null = null;
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const length = node.textContent?.length ?? 0;
    if (node === range.startContainer) start = offset + range.startOffset;
    if (node === range.endContainer) end = offset + range.endOffset;
    offset += length;
  }
  if (start === null || end === null || end <= start) return null;
  return { start, end };
}

export function SourceView({
  path,
  text,
  review,
  refusal,
  onComment,
}: {
  /** The change-relative path of the file. */
  path: string;
  text: string;
  /** OCW-FR-COLR: the review of the hunks of this file, if it holds any. */
  review?: EditorReview;
  /** OCW-FR-DTRX: a refused decision on a hunk of this file. */
  refusal?: string | null;
  /** OCW-FR-JBFZ: present on a stream location alone. */
  onComment?: (selection: ChangeSelection) => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [selection, setSelection] = useState<ChangeSelection | null>(null);

  /** CMT-FR-05: a selection in the text offers a discussion of it. */
  const readSelection = () => {
    if (!onComment || !hostRef.current) return;
    const current = window.getSelection();
    if (!current || current.rangeCount === 0 || current.isCollapsed) {
      setSelection(null);
      return;
    }
    const range = current.getRangeAt(0);
    if (!hostRef.current.contains(range.commonAncestorContainer)) return;
    const found = sourceOffsets(hostRef.current, range);
    if (!found) {
      setSelection(null);
      return;
    }
    const quote = text.slice(found.start, found.end);
    if (quote.trim() === "") {
      setSelection(null);
      return;
    }
    setSelection({ path, start: found.start, end: found.end, quote });
  };

  return (
    <div className="openspec-source" data-testid="openspec-source" data-path={path}>
      {refusal && (
        <p className="openspec-source__refusal" role="alert" data-testid="openspec-hunk-refusal">
          {refusal}
        </p>
      )}
      {onComment && selection && (
        <div className="openspec-source__comment">
          <button
            type="button"
            className="btn btn--sm btn--default"
            onClick={() => {
              onComment(selection);
              setSelection(null);
            }}
          >
            Discuss selection
          </button>
        </div>
      )}
      <div ref={hostRef} onMouseUp={readSelection} onKeyUp={readSelection}>
        {review ? (
          <SourceReview text={text} review={review} note={null} />
        ) : (
          <pre className="openspec-source__text" data-testid="openspec-source-text">
            {text}
          </pre>
        )}
      </div>
    </div>
  );
}
