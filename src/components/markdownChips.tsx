import { useId, useState } from "react";
import {
  breakLabel,
  commentText,
  COMMENT_CHIP_LABEL,
  type BreakLevel,
} from "../markdown/markers";

/**
 * EDT-FR-UYPS / EDT-FR-IGZL / EDT-FR-PCCA: the chip of an HTML comment on a
 * read-only rich surface — the Document tab, the original side of a Diff tab,
 * and a rendered discussion message.
 *
 * It takes keyboard focus and shows its comment text in a tooltip, as plain
 * text, on hover and on focus. It offers no removal. The comment text is a
 * React text node, never markup (EDT-FR-KMAR).
 */
export function CommentChip({ raw }: { raw: string }) {
  const [shown, setShown] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const id = useId();
  const open = shown && !dismissed;
  const show = () => {
    setDismissed(false);
    setShown(true);
  };
  return (
    <span className="md-chip-anchor">
      <span
        className="md-chip"
        role="img"
        aria-label={COMMENT_CHIP_LABEL}
        aria-describedby={open ? id : undefined}
        tabIndex={0}
        onMouseEnter={show}
        onMouseLeave={() => setShown(false)}
        onFocus={show}
        onBlur={() => setShown(false)}
        onKeyDown={(event) => {
          if (event.key === "Escape" && open) {
            event.stopPropagation();
            setDismissed(true);
          }
        }}
      >
        {/* The raw text is visually hidden, so a selection copies the source. */}
        <span className="md-chip__raw">{raw}</span>
      </span>
      {open && (
        <span className="md-chip-tooltip" role="tooltip" id={id}>
          {commentText(raw)}
        </span>
      )}
    </span>
  );
}

/** EDT-FR-UYPS: a comment on lines of its own, as a chip on a line. */
export function CommentChipLine({ raw }: { raw: string }) {
  return (
    <div className="md-chip-line">
      <CommentChip raw={raw} />
    </div>
  );
}

/** EDT-FR-CSKH / EDT-FR-XKTS: the spacer of a vertical break. */
export function BreakSpacer({ level }: { level: number }) {
  const at = Math.min(Math.max(Math.round(level) || 4, 1), 4) as BreakLevel;
  return (
    <div className="md-break" data-level={at} role="separator" aria-label={breakLabel(at)} />
  );
}
